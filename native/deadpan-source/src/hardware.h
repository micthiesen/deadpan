// Included by decoder.c after its owned context, error and geometry helpers.
// Only public FFmpeg APIs are used; no private VideoToolbox session ABI.
static int hardware_codec(enum AVCodecID codec) {
    return codec == AV_CODEC_ID_H264 || codec == AV_CODEC_ID_HEVC || codec == AV_CODEC_ID_VP9;
}
static int hardware_preferred(const DeadpanSource *s, enum AVCodecID codec) {
    return s->limits.acceleration && hardware_codec(codec);
}
static int hardware_layout(enum AVPixelFormat layout) {
    return layout == AV_PIX_FMT_YUV420P || layout == AV_PIX_FMT_YUVJ420P ||
        layout == AV_PIX_FMT_YUV420P10LE;
}
static int hardware_hevc_frame_only(const DeadpanSource *s) {
    const AVCodecParameters *p = s->format->streams[s->stream]->codecpar;
    // Require progressive, non-interlaced, frame-only profile constraints in
    // both hvcC and every admitted SPS. Unknown/field coding stays in software.
    // Parameter-set spans were bounded by parse_hvcc before codec allocation.
    if (p->extradata_size < 23 || (p->extradata[6] & 0xd0) != 0x90) return 0;
    for (int i = 0; i < s->parameter_sets; i++) {
        if (s->parameter_set[i].type != 33) continue;
        const uint8_t *nal = p->extradata + s->parameter_set[i].offset;
        int length = s->parameter_set[i].length, written = 0, zeros = 0;
        uint8_t prefix[7];
        for (int at = 2; at < length && written < 7; at++) {
            uint8_t byte = nal[at];
            if (zeros >= 2 && byte == 3) { zeros = 0; continue; }
            prefix[written++] = byte;
            zeros = byte == 0 ? zeros + 1 : 0;
        }
        if (written != 7 || (prefix[6] & 0xd0) != 0x90) return 0;
    }
    return 1;
}
static int hardware_format_allowed(const AVCodecContext *context) {
    const DeadpanSource *s = context->opaque;
    if (!hardware_preferred(s, context->codec_id) || !context->hw_device_ctx ||
        !hardware_layout(context->sw_pix_fmt)) return 0;
    if (context->field_order != AV_FIELD_UNKNOWN && context->field_order != AV_FIELD_PROGRESSIVE) return 0;
    // BWDIF and picture-timing SEI need software field pictures. Even a
    // frame-only SPS can signal repeats; keep that entire cadence in software.
    if (context->codec_id == AV_CODEC_ID_H264 &&
        (!s->h264_timing.valid || s->h264_timing.field_cadence)) return 0;
    if (context->codec_id == AV_CODEC_ID_HEVC && !s->hevc_frame_only) return 0;
    return 1;
}
static int picture_layout(const DeadpanSource *s) {
    // sw_pix_fmt is the codec's logical decoded layout, not the surface's
    // NV12/P010 storage. In particular full-range H.264/HEVC remains yuvj420p.
    return s->frame->format == AV_PIX_FMT_VIDEOTOOLBOX ? s->decoder->sw_pix_fmt : s->frame->format;
}
static void clear_hardware_pixels(DeadpanSource *s) {
    if (s->canonical) av_frame_unref(s->canonical);
    if (s->download) av_frame_unref(s->download);
}
static int hardware_plane(DeadpanSource *s, AVFrame *f, int plane, size_t columns, size_t rows) {
    if (!f->data[plane] || f->linesize[plane] <= 0 || (size_t)f->linesize[plane] < columns)
        return fail(s, "invalid_picture", "downloaded surface plane has an unsupported stride");
    if (!rows || rows - 1 > (SIZE_MAX - columns) / (size_t)f->linesize[plane])
        return fail(s, "resource_limit", "downloaded surface plane span overflows addressable memory");
    size_t span = (rows - 1) * (size_t)f->linesize[plane] + columns;
    AVBufferRef *buffer = av_frame_get_plane_buffer(f, plane);
    if (!buffer || (uintptr_t)f->data[plane] < (uintptr_t)buffer->data)
        return fail(s, "invalid_picture", "downloaded surface plane has no owning buffer");
    uintptr_t offset = (uintptr_t)f->data[plane] - (uintptr_t)buffer->data;
    if (offset > buffer->size || span > buffer->size - offset)
        return fail(s, "invalid_picture", "downloaded surface plane escapes its owning buffer");
    return 1;
}
static AVFrame *picture_pixels(DeadpanSource *s) {
    if (s->frame->format != AV_PIX_FMT_VIDEOTOOLBOX) return s->frame;
    if (s->canonical && s->canonical->buf[0]) return s->canonical;
    if (!hardware_layout(s->pixel_format) ||
        !geometry_ok(&s->limits, s->frame->width, s->frame->height)) {
        fail(s, "unsupported_conversion", "hardware surface has no qualified bounded pixel layout");
        return NULL;
    }
    if (!s->download) s->download = av_frame_alloc();
    if (!s->canonical) s->canonical = av_frame_alloc();
    if (!s->download || !s->canonical) {
        fail(s, "resource_exhausted", "allocate hardware surface transfer frames");
        return NULL;
    }
    if (check(s) < 0) return NULL;
    int result = av_hwframe_transfer_data(s->download, s->frame, 0);
    if (result < 0) { fferror(s, "download VideoToolbox picture", result); return NULL; }
    // At most one successful download per received frame; a later failure
    // poisons the session. Count the transfer even if unpacking is cancelled.
    s->work.hardware_transfers++;
    if (check(s) < 0) return NULL;
    AVFrame *input = s->download, *output = s->canonical;
    int ten = s->pixel_format == AV_PIX_FMT_YUV420P10LE;
    if (input->format != (ten ? AV_PIX_FMT_P010LE : AV_PIX_FMT_NV12) ||
        input->width != s->info.width || input->height != s->info.height) {
        fail(s, "unsupported_conversion", "downloaded VideoToolbox surface changed layout or visible geometry");
        return NULL;
    }
    size_t bytes = ten ? 2 : 1;
    size_t width = (size_t)input->width, height = (size_t)input->height;
    size_t chroma_width = (width + 1) / 2, chroma_height = (height + 1) / 2;
    if (hardware_plane(s, input, 0, width * bytes, height) < 0 ||
        hardware_plane(s, input, 1, chroma_width * bytes * 2, chroma_height) < 0) return NULL;
    output->format = s->pixel_format;
    output->width = input->width; output->height = input->height;
    if ((result = av_frame_get_buffer(output, 32)) < 0 ||
        (result = av_frame_copy_props(output, s->frame)) < 0) {
        fferror(s, "allocate canonical downloaded picture", result);
        return NULL;
    }
    // Unpack exact codes, without scaling, range conversion or chroma
    // resampling. Both output paths then use the same planar converters as
    // software decoding. Do not relabel a hardware frame as CPU memory.
    for (int component = 0; component < 3; component++) {
        size_t columns = component ? chroma_width : width;
        size_t rows = component ? chroma_height : height;
        size_t step = bytes * (component ? 2 : 1);
        size_t offset = component == 2 ? bytes : 0;
        for (size_t y = 0; y < rows; y++) {
            if (check(s) < 0) return NULL;
            const uint8_t *from = input->data[component != 0] + y * (size_t)input->linesize[component != 0] + offset;
            uint8_t *to = output->data[component] + y * (size_t)output->linesize[component];
            for (size_t x = 0; x < columns; x++) {
                if (ten) {
                    unsigned value = (unsigned)(from[x * step] | (from[x * step + 1] << 8)) >> 6;
                    to[x * 2] = (uint8_t)value; to[x * 2 + 1] = (uint8_t)(value >> 8);
                } else {
                    to[x] = from[x * step];
                }
            }
        }
    }
    return output;
}
