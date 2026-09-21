#define _POSIX_C_SOURCE 200809L
#include "decoder.h"
#include <errno.h>
#include <inttypes.h>
#include <limits.h>
#include <stdarg.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>
#include <libavcodec/avcodec.h>
#include <libavcodec/version.h>
#include <libavformat/avformat.h>
#include <libavformat/version.h>
#include <libavutil/avutil.h>
#include <libavutil/display.h>
#include <libavutil/error.h>
#include <libavutil/mem.h>
#include <libavutil/opt.h>
#include <libavutil/pixdesc.h>
#include <libavutil/version.h>
#include <libswscale/swscale.h>
#include <libswscale/version.h>
#if LIBAVCODEC_VERSION_INT != AV_VERSION_INT(62, 11, 103) || LIBAVFORMAT_VERSION_INT != AV_VERSION_INT(62, 3, 103) || LIBAVUTIL_VERSION_INT != AV_VERSION_INT(60, 8, 103) || LIBSWSCALE_VERSION_INT != AV_VERSION_INT(9, 1, 103)
#error "deadpan-source requires exactly FFmpeg 8.0.3 headers"
#endif
#define IO_BUFFER_BYTES 32768
#define MAX_PROBE_BYTES (1024 * 1024)
#define MAX_STREAMS (DEADPAN_SOURCE_MAX_AUDIO_STREAMS + 1)
#define DEMUXERS "mov,matroska,webm"
#define CODECS "h264,ffv1"

struct DeadpanSource {
    int fd;
    int64_t length, position;
    DeadpanSourceLimits limits;
    DeadpanSourceInfo info;
    AVIOContext *io;
    AVFormatContext *format;
    AVCodecContext *decoder;
    AVPacket *packet;
    AVFrame *frame;
    struct SwsContext *scaler;
    int stream, pixel_format, chroma_location, draining, ended, poisoned, inventory_ready;
    unsigned int stream_count;
    uint64_t frames, packets, io_bytes, deadline;
    DeadpanCancelled cancelled;
    const void *cancel_opaque;
    DeadpanSourceError *error;
};
static int fail(DeadpanSource *s, const char *code, const char *format, ...) {
    if (s->error && !s->error->code[0]) {
        va_list args;
        (void)snprintf(s->error->code, sizeof(s->error->code), "%s", code);
        va_start(args, format);
        (void)vsnprintf(s->error->message, sizeof(s->error->message), format, args);
        va_end(args);
    }
    return -1;
}
static int fferror(DeadpanSource *s, const char *operation, int error) {
    char message[AV_ERROR_MAX_STRING_SIZE];
    if (av_strerror(error, message, sizeof(message)) < 0)
        (void)snprintf(message, sizeof(message), "error %d", error);
    return fail(s, "ffmpeg_failure", "%s: %s", operation, message);
}
static uint64_t monotonic_ns(void) {
    struct timespec now;
    if (clock_gettime(CLOCK_MONOTONIC, &now) || now.tv_sec < 0 ||
        (uint64_t)now.tv_sec > UINT64_MAX / 1000000000ULL) return UINT64_MAX;
    return (uint64_t)now.tv_sec * 1000000000ULL + (uint64_t)now.tv_nsec;
}
static int check(DeadpanSource *s) {
    // Demuxers may turn an AVIO error into EOF or return buffered frames. A host
    // I/O/budget failure must still fail this operation instead of being hidden.
    if (s->error && s->error->code[0]) return -1;
    if (s->cancelled && s->cancelled(s->cancel_opaque)) return fail(s, "cancelled", "source decode cancelled");
    uint64_t now = monotonic_ns();
    if (now == UINT64_MAX || now >= s->deadline) return fail(s, "deadline_exceeded", "source decode exceeded its cooperative deadline");
    return 1;
}
static int interrupt(void *opaque) { return check(opaque) < 0; }
static int begin(DeadpanSource *s, uint64_t timeout, DeadpanCancelled cancelled,
                 const void *opaque, DeadpanSourceError *error) {
    memset(error, 0, sizeof(*error));
    s->error = error;
    s->cancelled = cancelled;
    s->cancel_opaque = opaque;
    s->io_bytes = 0;
    if (s->poisoned) return fail(s, "session_failed", "reopen a decoder after an operation failure");
    if (!timeout || timeout > 60000) return fail(s, "invalid_configuration", "timeout outside 1..60000ms");
    uint64_t now = monotonic_ns();
    if (now == UINT64_MAX || now > UINT64_MAX - timeout * 1000000ULL)
        return fail(s, "internal_error", "monotonic deadline overflow");
    s->deadline = now + timeout * 1000000ULL;
    return check(s);
}
static int finish(DeadpanSource *s, int result) {
    if (result < 0) s->poisoned = 1;
    s->error = NULL;
    s->cancelled = NULL;
    s->cancel_opaque = NULL;
    return result;
}
static int read_descriptor(void *opaque, uint8_t *buffer, int size) {
    DeadpanSource *s = opaque;
    if (check(s) < 0) return AVERROR_EXIT;
    if (size <= 0 || s->position >= s->length) return AVERROR_EOF;
    size_t amount = (size_t)size;
    if ((int64_t)amount > s->length - s->position) amount = (size_t)(s->length - s->position);
    if (s->io_bytes > s->limits.max_io_bytes_per_call || amount > s->limits.max_io_bytes_per_call - s->io_bytes) {
        fail(s, "resource_limit", "per-call input byte budget exceeded"); return AVERROR(EFBIG);
    }
    ssize_t count;
    do {
        if (check(s) < 0) return AVERROR_EXIT;
        count = pread(s->fd, buffer, amount, (off_t)s->position);
    } while (count < 0 && errno == EINTR);
    if (count <= 0) { fail(s, "io_failure", "input snapshot read failed or ended before its stated length"); return AVERROR(EIO); }
    s->position += count;
    s->io_bytes += (uint64_t)count;
    return (int)count;
}
static int64_t seek_descriptor(void *opaque, int64_t offset, int whence) {
    DeadpanSource *s = opaque;
    if (check(s) < 0) return AVERROR_EXIT;
    if (whence & AVSEEK_SIZE) return s->length;
    int64_t base;
    switch (whence & ~AVSEEK_FORCE) {
        case SEEK_SET: base = 0; break;
        case SEEK_CUR: base = s->position; break;
        case SEEK_END: base = s->length; break;
        default: return AVERROR(EINVAL);
    }
    if ((offset > 0 && base > INT64_MAX - offset) || (offset < 0 && base < INT64_MIN - offset)) return AVERROR(EOVERFLOW);
    int64_t position = base + offset;
    if (position < 0 || position > s->length) return AVERROR(EINVAL);
    s->position = position;
    return position;
}
static int deny_external_io(AVFormatContext *format, AVIOContext **io, const char *url, int flags, AVDictionary **options) {
    (void)format; (void)io; (void)url; (void)flags; (void)options;
    return AVERROR(EPERM);
}
static int runtime(DeadpanSource *s) {
    if (avcodec_version() != LIBAVCODEC_VERSION_INT || avformat_version() != LIBAVFORMAT_VERSION_INT ||
        avutil_version() != LIBAVUTIL_VERSION_INT || swscale_version() != LIBSWSCALE_VERSION_INT)
        return fail(s, "runtime_mismatch", "loaded FFmpeg libraries differ from pinned 8.0.3");
    const char *required[] = {"--disable-gpl", "--disable-nonfree", "--disable-version3", "--disable-network"};
    const char *forbidden[] = {"--enable-gpl", "--enable-nonfree", "--enable-version3", "--enable-network"};
    const char *configs[] = {avcodec_configuration(), avformat_configuration(), avutil_configuration(), swscale_configuration()};
    const char *licenses[] = {avcodec_license(), avformat_license(), avutil_license(), swscale_license()};
    for (size_t i = 0; i < 4; i++) {
        if (strcmp(licenses[i], "LGPL version 2.1 or later")) return fail(s, "runtime_mismatch", "loaded FFmpeg license differs from LGPL 2.1+");
        for (size_t j = 0; j < 4; j++)
            if (!strstr(configs[i], required[j]) || strstr(configs[i], forbidden[j]))
                return fail(s, "runtime_mismatch", "loaded FFmpeg configuration violates %s", required[j]);
    }
    return 1;
}
static int allowed_codec(enum AVCodecID id) {
    switch (id) {
        case AV_CODEC_ID_H264: case AV_CODEC_ID_FFV1: return 1;
        default: return 0;
    }
}
static int geometry(DeadpanSource *s, int width, int height) {
    if (width <= 0 || height <= 0 || (unsigned)width > s->limits.max_dimension || (unsigned)height > s->limits.max_dimension ||
        (uint64_t)width * (uint64_t)height > s->limits.max_pixels)
        return fail(s, "resource_limit", "source dimensions exceed configured bounds");
    return 1;
}
static int rotation(DeadpanSource *s, const AVPacketSideData *side, int count, int *out) {
    *out = 0;
    const AVPacketSideData *matrix = av_packet_side_data_get(side, count, AV_PKT_DATA_DISPLAYMATRIX);
    if (!matrix) return 1;
    if (matrix->size != 9 * sizeof(int32_t)) return fail(s, "unsupported_transform", "invalid display matrix length");
    int32_t m[9]; memcpy(m, matrix->data, sizeof(m));
    // Accept only exact orthogonal unit rotations. Reject reflection, translation,
    // shear, perspective, and scaling instead of silently losing those transforms.
    if (m[2] || m[5] || m[6] || m[7] || m[8] != (1 << 30)) return fail(s, "unsupported_transform", "display matrix has an unsupported transform");
    const int32_t linear[4][4] = {{65536,0,0,65536},{0,65536,-65536,0},{-65536,0,0,-65536},{0,-65536,65536,0}};
    for (int i = 0; i < 4; i++) {
        if (m[0] == linear[i][0] && m[1] == linear[i][1] && m[3] == linear[i][2] && m[4] == linear[i][3]) { *out = i; return 1; }
    }
    return fail(s, "unsupported_transform", "only exact right-angle rotations are supported");
}
static int color(DeadpanSource *s, int format, int range, int matrix, int transfer, int primaries) {
    const AVPixFmtDescriptor *desc = av_pix_fmt_desc_get(format);
    if (!desc || (desc->flags & (AV_PIX_FMT_FLAG_HWACCEL | AV_PIX_FMT_FLAG_FLOAT | AV_PIX_FMT_FLAG_BAYER | AV_PIX_FMT_FLAG_PAL | AV_PIX_FMT_FLAG_BITSTREAM)))
        return fail(s, "unsupported_pixel_format", "source pixel layout is unsupported");
    if (desc->nb_components != 3 && desc->nb_components != 4)
        return fail(s, "unsupported_pixel_format", "source must have three color components");
    // Alpha needs a separate representation and composition contract.
    if (desc->flags & AV_PIX_FMT_FLAG_ALPHA) return fail(s, "unsupported_pixel_format", "source alpha is not qualified");
    for (int i = 0; i < desc->nb_components; i++)
        if (desc->comp[i].depth != 8) return fail(s, "unsupported_depth", "only eight-bit SDR decode is qualified");
    if (range != AVCOL_RANGE_MPEG && range != AVCOL_RANGE_JPEG)
        return fail(s, "missing_interpretation", "source color range needs an explicit interpretation");
    if (transfer != AVCOL_TRC_BT709 && transfer != AVCOL_TRC_IEC61966_2_1 && transfer != AVCOL_TRC_LINEAR)
        return fail(s, "unsupported_transfer", "source needs a qualified SDR transfer interpretation");
    if (primaries != AVCOL_PRI_BT709 && primaries != AVCOL_PRI_BT2020 && primaries != AVCOL_PRI_SMPTE432)
        return fail(s, "unsupported_primaries", "source color primaries are missing or unsupported");
    if (desc->flags & AV_PIX_FMT_FLAG_RGB) {
        if (matrix != AVCOL_SPC_RGB || range != AVCOL_RANGE_JPEG)
            return fail(s, "unsupported_matrix", "RGB input requires explicit full-range RGB matrix tags");
    } else if (matrix != AVCOL_SPC_BT709 && matrix != AVCOL_SPC_BT470BG && matrix != AVCOL_SPC_SMPTE170M && matrix != AVCOL_SPC_BT2020_NCL) {
        return fail(s, "unsupported_matrix", "source YUV matrix is missing or unsupported");
    }
    return 1;
}
static AVRational sar(AVRational value) { return value.num == 0 ? (AVRational){1,1} : value; }
static int packet_color_metadata(DeadpanSource *s, const AVPacketSideData *side, int count, const char *origin) {
    for (int i = 0; i < count; i++) {
        switch (side[i].type) {
            case AV_PKT_DATA_MASTERING_DISPLAY_METADATA:
            case AV_PKT_DATA_CONTENT_LIGHT_LEVEL:
            case AV_PKT_DATA_DOVI_CONF:
            case AV_PKT_DATA_DYNAMIC_HDR10_PLUS:
                return fail(s, "unsupported_hdr", "source %s carries unqualified HDR metadata: %s", origin, av_packet_side_data_name(side[i].type));
            case AV_PKT_DATA_ICC_PROFILE:
                return fail(s, "unsupported_icc", "source %s carries an unqualified ICC profile", origin);
            case AV_PKT_DATA_AMBIENT_VIEWING_ENVIRONMENT:
                return fail(s, "unsupported_interpretation", "source %s carries an unqualified ambient viewing environment", origin);
            default: break;
        }
    }
    return 1;
}
static int table(DeadpanSource *s, int initial) {
    if (!initial && s->format->nb_streams != s->stream_count) return fail(s, "stream_changed", "source stream table changed");
    int selected = -1;
    for (unsigned int i = 0; i < s->format->nb_streams; i++) {
        AVStream *stream = s->format->streams[i];
        AVCodecParameters *p = stream->codecpar;
        if (p->codec_type == AVMEDIA_TYPE_VIDEO) {
            if (selected >= 0 || (stream->disposition & AV_DISPOSITION_ATTACHED_PIC)) return fail(s, "unsupported_streams", "source requires exactly one non-attached video stream");
            if (!allowed_codec(p->codec_id)) return fail(s, "unsupported_codec", "source codec is outside the qualified decoder allowlist");
            // Container-level metadata must fail before stream probing can open
            // a decoder, and must be rechecked if demuxing changes the table.
            if (packet_color_metadata(s, p->coded_side_data, p->nb_coded_side_data, "stream") < 0) return -1;
            if (geometry(s, p->width, p->height) < 0) return -1;
            selected = (int)i;
        } else if (p->codec_type != AVMEDIA_TYPE_AUDIO) return fail(s, "unsupported_streams", "source contains an unsupported stream type");
        else stream->discard = AVDISCARD_ALL;
    }
    if (selected < 0) return fail(s, "unsupported_streams", "source has no video stream");
    if (initial) { s->stream = selected; s->stream_count = s->format->nb_streams; }
    else if (selected != s->stream) return fail(s, "stream_changed", "source video stream changed");
    if (s->inventory_ready) {
        AVStream *video = s->format->streams[s->stream];
        if (video->index != s->info.stream_index) return fail(s, "stream_changed", "source video stream identity changed");
        uint32_t audio = 0;
        for (unsigned int i = 0; i < s->format->nb_streams; i++) {
            AVStream *stream = s->format->streams[i];
            AVCodecParameters *p = stream->codecpar;
            if (p->codec_type != AVMEDIA_TYPE_AUDIO) continue;
            if (audio >= s->info.audio_stream_count) return fail(s, "stream_changed", "source audio stream table changed");
            DeadpanSourceAudioInfo *expected = &s->info.audio_streams[audio++];
            if (stream->index != expected->stream_index ||
                stream->time_base.num != expected->time_base_num || stream->time_base.den != expected->time_base_den ||
                p->sample_rate != expected->sample_rate || p->ch_layout.nb_channels != expected->channel_count ||
                strcmp(avcodec_get_name(p->codec_id), expected->codec))
                return fail(s, "stream_changed", "source audio stream identity changed");
        }
        if (audio != s->info.audio_stream_count) return fail(s, "stream_changed", "source audio stream table changed");
    }
    return 1;
}
static int capture_audio_inventory(DeadpanSource *s) {
    uint32_t count = 0;
    for (unsigned int i = 0; i < s->format->nb_streams; i++) {
        AVStream *stream = s->format->streams[i];
        AVCodecParameters *p = stream->codecpar;
        if (p->codec_type != AVMEDIA_TYPE_AUDIO) continue;
        if (count >= DEADPAN_SOURCE_MAX_AUDIO_STREAMS || stream->index < 0 ||
            stream->time_base.num <= 0 || stream->time_base.den <= 0 ||
            p->sample_rate < 0 || p->ch_layout.nb_channels < 0)
            return fail(s, "invalid_stream", "source audio stream metadata is invalid or exceeds its bound");
        DeadpanSourceAudioInfo *audio = &s->info.audio_streams[count++];
        audio->stream_index = stream->index;
        audio->time_base_num = stream->time_base.num;
        audio->time_base_den = stream->time_base.den;
        audio->stream_start = stream->start_time;
        audio->stream_duration = stream->duration;
        audio->sample_rate = p->sample_rate;
        audio->channel_count = p->ch_layout.nb_channels;
        int codec_length = snprintf(audio->codec, sizeof(audio->codec), "%s", avcodec_get_name(p->codec_id));
        if (codec_length < 0 || (size_t)codec_length >= sizeof(audio->codec))
            return fail(s, "invalid_stream", "source audio codec name exceeds its bound");
    }
    s->info.audio_stream_count = count;
    s->inventory_ready = 1;
    return 1;
}
static int open_impl(DeadpanSource *s) {
    if (runtime(s) < 0) return -1;
    if (!s->limits.max_input_bytes || s->limits.max_input_bytes > 64ULL*1024*1024*1024 ||
        !s->limits.max_frames || s->limits.max_frames > 10000000 || !s->limits.max_packets || s->limits.max_packets > 40000000 ||
        !s->limits.max_io_bytes_per_call || s->limits.max_io_bytes_per_call > 1024ULL*1024*1024 ||
        !s->limits.max_pixels || s->limits.max_pixels > 8192ULL*8192 ||
        !s->limits.max_dimension || s->limits.max_dimension > 8192 || !s->limits.max_packets_per_frame || s->limits.max_packets_per_frame > 10000)
        return fail(s, "invalid_configuration", "source decode limits exceed hard bounds");
    struct stat status;
    if (fstat(s->fd, &status) || !S_ISREG(status.st_mode) || status.st_size != s->length || s->length <= 0 || (uint64_t)s->length > s->limits.max_input_bytes)
        return fail(s, "invalid_input", "source must be a nonempty regular snapshot within its byte bound");
    uint8_t *buffer = av_malloc(IO_BUFFER_BYTES);
    if (!buffer) return fail(s, "resource_exhausted", "allocate source I/O buffer");
    s->io = avio_alloc_context(buffer, IO_BUFFER_BYTES, 0, s, read_descriptor, NULL, seek_descriptor);
    if (!s->io) { av_free(buffer); return fail(s, "resource_exhausted", "allocate source I/O context"); }
    s->io->seekable = AVIO_SEEKABLE_NORMAL;
    s->format = avformat_alloc_context();
    if (!s->format) return fail(s, "resource_exhausted", "allocate demux context");
    s->format->pb = s->io;
    s->format->flags |= AVFMT_FLAG_CUSTOM_IO;
    s->format->probesize = s->length < MAX_PROBE_BYTES ? s->length : MAX_PROBE_BYTES;
    s->format->format_probesize = MAX_PROBE_BYTES;
    s->format->max_analyze_duration = 5000000;
    s->format->max_streams = MAX_STREAMS;
    s->format->max_probe_packets = 256;
    s->format->max_ts_probe = 256;
    s->format->max_index_size = 16 * 1024 * 1024;
    s->format->max_picture_buffer = 64 * 1024 * 1024;
    s->format->interrupt_callback = (AVIOInterruptCB){interrupt,s};
    s->format->io_open = deny_external_io;
    s->format->protocol_whitelist = av_strdup("");
    s->format->format_whitelist = av_strdup(DEMUXERS);
    s->format->codec_whitelist = av_strdup(CODECS);
    if (!s->format->protocol_whitelist || !s->format->format_whitelist || !s->format->codec_whitelist)
        return fail(s, "resource_exhausted", "allocate format allowlists");
    AVDictionary *options = NULL;
    // MOV's external data references remain disabled even if a file carries them.
    av_dict_set(&options, "enable_drefs", "0", 0);
    av_dict_set(&options, "use_absolute_path", "0", 0);
    int result = avformat_open_input(&s->format, NULL, NULL, &options);
    av_dict_free(&options);
    if (result < 0) return fferror(s, "open source descriptor", result);
    if (table(s, 1) < 0) return -1;
    unsigned int count = s->stream_count;
    AVDictionary **probe_options = av_calloc(count, sizeof(*probe_options));
    if (!probe_options) return fail(s, "resource_exhausted", "allocate bounded probe options");
    int64_t max_pixels = (int64_t)s->limits.max_dimension * s->limits.max_dimension;
    if ((uint64_t)max_pixels > s->limits.max_pixels) max_pixels = (int64_t)s->limits.max_pixels;
    int options_result = 0;
    for (unsigned int i = 0; i < count; i++) {
        if ((int)i != s->stream) continue;
        if (av_dict_set(&probe_options[i], "threads", "1", 0) < 0 ||
            av_dict_set(&probe_options[i], "err_detect", "explode", 0) < 0 ||
            av_dict_set_int(&probe_options[i], "max_pixels", max_pixels, 0) < 0) options_result = AVERROR(ENOMEM);
    }
    result = options_result < 0 ? options_result : avformat_find_stream_info(s->format, probe_options);
    for (unsigned int i = 0; i < count; i++) av_dict_free(&probe_options[i]);
    av_free(probe_options);
    if (result < 0) return fferror(s, "probe source stream", result);
    if (check(s) < 0 || table(s, 0) < 0) return -1;
    AVStream *stream = s->format->streams[s->stream];
    AVCodecParameters *p = stream->codecpar;
    if (stream->time_base.num <= 0 || stream->time_base.den <= 0) return fail(s, "invalid_time_base", "source stream has no positive time base");
    if (p->field_order != AV_FIELD_UNKNOWN && p->field_order != AV_FIELD_PROGRESSIVE) return fail(s, "unsupported_interlace", "interlaced source requires a qualified deinterlacer");
    if (color(s, p->format, p->color_range, p->color_space, p->color_trc, p->color_primaries) < 0) return -1;
    const AVPixFmtDescriptor *pixel = av_pix_fmt_desc_get(p->format);
    if (!(pixel->flags & AV_PIX_FMT_FLAG_RGB) && (pixel->log2_chroma_w || pixel->log2_chroma_h)) {
        int x, y;
        if (av_chroma_location_enum_to_pos(&x, &y, p->chroma_location) < 0)
            return fail(s, "missing_interpretation", "subsampled YUV needs an explicit chroma location");
    }
    s->chroma_location = p->chroma_location;
    AVRational aspect = sar(stream->sample_aspect_ratio.num ? stream->sample_aspect_ratio : p->sample_aspect_ratio);
    if (aspect.num <= 0 || aspect.den <= 0 || aspect.num > 1000000 || aspect.den > 1000000) return fail(s, "unsupported_aspect", "invalid or excessive sample aspect ratio");
    s->info = (DeadpanSourceInfo){.width=p->width,.height=p->height,.stream_index=stream->index,.time_base_num=stream->time_base.num,.time_base_den=stream->time_base.den,.sar_num=aspect.num,.sar_den=aspect.den,.range=p->color_range,.matrix=p->color_space,.transfer=p->color_trc,.primaries=p->color_primaries,.stream_start=stream->start_time,.stream_duration=stream->duration,.container_start=s->format->start_time,.container_duration=s->format->duration};
    if (capture_audio_inventory(s) < 0) return -1;
    if (rotation(s, p->coded_side_data, p->nb_coded_side_data, &s->info.rotation) < 0) return -1;
    s->pixel_format = p->format;
    const AVCodec *codec = avcodec_find_decoder(p->codec_id);
    if (!codec) return fail(s, "unsupported_codec", "required source software decoder is unavailable");
    (void)snprintf(s->info.codec, sizeof(s->info.codec), "%s", codec->name);
    (void)snprintf(s->info.pixel_format, sizeof(s->info.pixel_format), "%s", av_get_pix_fmt_name(p->format));
    s->decoder = avcodec_alloc_context3(codec);
    s->packet = av_packet_alloc(); s->frame = av_frame_alloc();
    if (!s->decoder || !s->packet || !s->frame) return fail(s, "resource_exhausted", "allocate source decode context");
    if ((result = avcodec_parameters_to_context(s->decoder, p)) < 0) return fferror(s, "copy source codec parameters", result);
    s->decoder->thread_count = 1;
    s->decoder->thread_type = 0;
    s->decoder->max_pixels = max_pixels;
    s->decoder->err_recognition = AV_EF_EXPLODE | AV_EF_CAREFUL;
    s->decoder->pkt_timebase = stream->time_base;
    // Preserve crop fields for explicit rejection instead of applying an untracked
    // aperture change inside libavcodec.
    s->decoder->apply_cropping = 0;
    if ((result = avcodec_open2(s->decoder, codec, NULL)) < 0) return fferror(s, "open source decoder", result);
    return check(s);
}
void deadpan_source_close(DeadpanSource *s) {
    if (!s) return;
    sws_freeContext(s->scaler);
    av_frame_free(&s->frame);
    av_packet_free(&s->packet);
    avcodec_free_context(&s->decoder);
    if (s->format) { s->format->pb = NULL; avformat_close_input(&s->format); }
    if (s->io) { av_freep(&s->io->buffer); avio_context_free(&s->io); }
    av_free(s);
}
int deadpan_source_open(int fd, int64_t length, const DeadpanSourceLimits *limits, uint64_t timeout,
                        DeadpanCancelled cancelled, const void *opaque, DeadpanSource **out,
                        DeadpanSourceInfo *info, DeadpanSourceError *error) {
    *out = NULL; memset(error, 0, sizeof(*error));
    DeadpanSource *s = av_mallocz(sizeof(*s));
    if (!s) { (void)snprintf(error->code, sizeof(error->code), "resource_exhausted"); (void)snprintf(error->message, sizeof(error->message), "allocate source session"); return -1; }
    s->fd = fd; s->length = length; s->limits = *limits;
    if (begin(s, timeout, cancelled, opaque, error) < 0 || open_impl(s) < 0) { deadpan_source_close(s); return -1; }
    *info = s->info; *out = s;
    return finish(s, 1);
}
static int check_frame(DeadpanSource *s) {
    AVFrame *f = s->frame;
    AVStream *stream = s->format->streams[s->stream];
    if (f->decode_error_flags || (f->flags & AV_FRAME_FLAG_CORRUPT)) return fail(s, "corrupt_frame", "decoder reported a corrupt or concealed frame");
    if (f->flags & AV_FRAME_FLAG_INTERLACED) return fail(s, "unsupported_interlace", "interlaced frame requires a qualified deinterlacer");
    if (f->crop_top || f->crop_bottom || f->crop_left || f->crop_right) {
        // Codec padding is part of decoding the declared visible rectangle.
        // Admit it only when it resolves exactly to the immutable stream size.
        if (f->crop_left > (size_t)f->width || f->crop_right > (size_t)f->width - f->crop_left ||
            f->crop_top > (size_t)f->height || f->crop_bottom > (size_t)f->height - f->crop_top ||
            (size_t)f->width - f->crop_left - f->crop_right != (size_t)s->info.width ||
            (size_t)f->height - f->crop_top - f->crop_bottom != (size_t)s->info.height)
            return fail(s, "unsupported_transform", "crop does not resolve to the declared visible rectangle");
        int result = av_frame_apply_cropping(f, AV_FRAME_CROP_UNALIGNED);
        if (result < 0) return fferror(s, "apply declared decoder crop", result);
    }
    if (f->width != s->info.width || f->height != s->info.height || f->format != s->pixel_format ||
        (int)f->color_range != s->info.range || (int)f->colorspace != s->info.matrix || (int)f->color_trc != s->info.transfer || (int)f->color_primaries != s->info.primaries)
        return fail(s, "stream_changed", "source geometry, pixel layout, or color interpretation changed");
    if ((int)f->chroma_location != s->chroma_location) return fail(s, "stream_changed", "source chroma location changed");
    AVRational aspect = sar(f->sample_aspect_ratio);
    if (av_cmp_q(aspect, (AVRational){s->info.sar_num,s->info.sar_den})) return fail(s, "stream_changed", "frame sample aspect ratio differs from source metadata");
    if (stream->time_base.num != s->info.time_base_num || stream->time_base.den != s->info.time_base_den)
        return fail(s, "stream_changed", "source time base changed");
    if (f->pts == AV_NOPTS_VALUE) return fail(s, "missing_pts", "frame has no original presentation timestamp");
    const AVFrameSideData *matrix = av_frame_get_side_data(f, AV_FRAME_DATA_DISPLAYMATRIX);
    if (matrix) {
        AVPacketSideData side = {.data=matrix->data,.size=matrix->size,.type=AV_PKT_DATA_DISPLAYMATRIX};
        int turn;
        if (rotation(s, &side, 1, &turn) < 0) return -1;
        if (turn != s->info.rotation) return fail(s, "stream_changed", "frame orientation differs from source metadata");
    }
    // HDR side data is rejected even if a malformed stream tags its transfer SDR.
    if (av_frame_get_side_data(f, AV_FRAME_DATA_MASTERING_DISPLAY_METADATA) ||
        av_frame_get_side_data(f, AV_FRAME_DATA_CONTENT_LIGHT_LEVEL) ||
        av_frame_get_side_data(f, AV_FRAME_DATA_DYNAMIC_HDR_PLUS) ||
        av_frame_get_side_data(f, AV_FRAME_DATA_DOVI_RPU_BUFFER) ||
        av_frame_get_side_data(f, AV_FRAME_DATA_DOVI_METADATA) ||
        av_frame_get_side_data(f, AV_FRAME_DATA_DYNAMIC_HDR_VIVID))
        return fail(s, "unsupported_hdr", "source frame carries unqualified HDR metadata");
    if (av_frame_get_side_data(f, AV_FRAME_DATA_ICC_PROFILE))
        return fail(s, "unsupported_icc", "source frame carries an unqualified ICC profile");
    if (av_frame_get_side_data(f, AV_FRAME_DATA_AMBIENT_VIEWING_ENVIRONMENT))
        return fail(s, "unsupported_interpretation", "source frame carries an unqualified ambient viewing environment");
    return 1;
}
static int rgba(DeadpanSource *s, uint8_t *pixels, size_t length) {
    size_t expected = (size_t)s->info.width * (size_t)s->info.height * 4;
    if (!pixels || length != expected) return fail(s, "invalid_configuration", "RGBA output must have the exact packed frame size");
    if (!s->scaler) {
        s->scaler = sws_alloc_context();
        if (!s->scaler) return fail(s, "unsupported_conversion", "source cannot be converted to packed RGBA8");
        if (av_opt_set_int(s->scaler, "srcw", s->info.width, 0) < 0 ||
            av_opt_set_int(s->scaler, "srch", s->info.height, 0) < 0 ||
            av_opt_set_int(s->scaler, "src_format", s->pixel_format, 0) < 0 ||
            av_opt_set_int(s->scaler, "dstw", s->info.width, 0) < 0 ||
            av_opt_set_int(s->scaler, "dsth", s->info.height, 0) < 0 ||
            av_opt_set_int(s->scaler, "dst_format", AV_PIX_FMT_RGBA, 0) < 0 ||
            av_opt_set_int(s->scaler, "sws_flags", SWS_BILINEAR | SWS_ACCURATE_RND | SWS_BITEXACT | SWS_FULL_CHR_H_INT, 0) < 0)
            return fail(s, "unsupported_conversion", "configure bounded RGBA conversion");
        const AVPixFmtDescriptor *pixel = av_pix_fmt_desc_get(s->pixel_format);
        if (!(pixel->flags & AV_PIX_FMT_FLAG_RGB) && (pixel->log2_chroma_w || pixel->log2_chroma_h)) {
            int x, y;
            if (av_chroma_location_enum_to_pos(&x, &y, s->chroma_location) < 0 ||
                av_opt_set_int(s->scaler, "src_h_chr_pos", x, 0) < 0 ||
                av_opt_set_int(s->scaler, "src_v_chr_pos", y, 0) < 0)
                return fail(s, "unsupported_conversion", "configure explicit source chroma location");
        }
        if (sws_init_context(s->scaler, NULL, NULL) < 0)
            return fail(s, "unsupported_conversion", "initialize explicit RGBA conversion");
        int matrix = SWS_CS_ITU709;
        switch (s->info.matrix) {
            case AVCOL_SPC_BT470BG: case AVCOL_SPC_SMPTE170M: matrix = SWS_CS_ITU601; break;
            case AVCOL_SPC_BT2020_NCL: matrix = SWS_CS_BT2020; break;
            default: break;
        }
        // libswscale does only YUV matrix/range conversion here, preserving the
        // source transfer and primaries for the shared compositor's transform.
        const int *coefficients = sws_getCoefficients(matrix);
        if (sws_setColorspaceDetails(s->scaler, coefficients, s->info.range == AVCOL_RANGE_JPEG,
            coefficients, 1, 0, 1 << 16, 1 << 16) < 0)
            return fail(s, "unsupported_conversion", "configure explicit source matrix and range");
    }
    if (check(s) < 0) return -1;
    uint8_t *planes[4] = {pixels,NULL,NULL,NULL};
    int strides[4] = {s->info.width * 4,0,0,0};
    int rows = sws_scale(s->scaler, (const uint8_t * const *)s->frame->data, s->frame->linesize,
        0, s->info.height, planes, strides);
    if (rows != s->info.height) return fail(s, "conversion_failure", "RGBA conversion did not write the complete frame");
    return check(s);
}
static void metadata(DeadpanSource *s, DeadpanSourceFrame *out) {
    *out = (DeadpanSourceFrame){.pts=s->frame->pts,.duration=s->frame->duration,.dts=s->frame->pkt_dts,.keyframe=!!(s->frame->flags & AV_FRAME_FLAG_KEY)};
}
static int next_impl(DeadpanSource *s, DeadpanSourceFrame *out, uint8_t *pixels, size_t length) {
    av_frame_unref(s->frame);
    if (s->ended) return 0;
    uint32_t packets = 0;
    // Both packet work and receive/send progress have explicit bounds.
    for (uint32_t step = 0; step <= s->limits.max_packets_per_frame + 1; step++) {
        if (check(s) < 0) return -1;
        int result = avcodec_receive_frame(s->decoder, s->frame);
        if (check(s) < 0) return -1;
        if (result == 0) {
            if (s->frames >= s->limits.max_frames) return fail(s, "resource_limit", "decoded frame count exceeds configured bound");
            s->frames++;
            if (table(s, 0) < 0 || check_frame(s) < 0) return -1;
            metadata(s, out);
            return pixels ? rgba(s, pixels, length) : 1;
        }
        if (result == AVERROR_EOF) { s->ended = 1; return 0; }
        if (result != AVERROR(EAGAIN)) return fferror(s, "receive source frame", result);
        if (s->draining) return fail(s, "decode_protocol", "decoder requested packets after draining");
        for (;;) {
            if (check(s) < 0) return -1;
            if (packets >= s->limits.max_packets_per_frame || s->packets >= s->limits.max_packets)
                return fail(s, "resource_limit", "source packet count exceeds configured bound");
            result = av_read_frame(s->format, s->packet);
            if (result == AVERROR_EOF) {
                result = avcodec_send_packet(s->decoder, NULL);
                if (result < 0) return fferror(s, "drain source decoder", result);
                s->draining = 1;
                break;
            }
            if (result < 0) return fferror(s, "read source packet", result);
            packets++; s->packets++;
            if (s->packet->flags & AV_PKT_FLAG_CORRUPT) { av_packet_unref(s->packet); return fail(s, "corrupt_packet", "demuxer reported a corrupt packet"); }
            if (s->packet->stream_index == s->stream) {
                if (table(s, 0) < 0 || packet_color_metadata(s, s->packet->side_data, s->packet->side_data_elems, "packet") < 0) {
                    av_packet_unref(s->packet);
                    return -1;
                }
                // receive EAGAIN means send must accept this packet. Never drop a
                // packet on send EAGAIN, which would corrupt reorder semantics.
                result = avcodec_send_packet(s->decoder, s->packet);
                av_packet_unref(s->packet);
                if (result < 0) return fferror(s, "send source packet", result);
                break;
            }
            av_packet_unref(s->packet);
        }
    }
    return fail(s, "resource_limit", "bounded decode progress budget exhausted");
}
int deadpan_source_next(DeadpanSource *s, uint64_t timeout, DeadpanCancelled cancelled,
                        const void *opaque, DeadpanSourceFrame *frame, uint8_t *pixels,
                        size_t length, DeadpanSourceError *error) {
    if (begin(s, timeout, cancelled, opaque, error) < 0) return finish(s, -1);
    return finish(s, next_impl(s, frame, pixels, length));
}
int deadpan_source_copy(DeadpanSource *s, uint64_t timeout, DeadpanCancelled cancelled,
                        const void *opaque, DeadpanSourceFrame *frame, uint8_t *pixels,
                        size_t length, DeadpanSourceError *error) {
    if (begin(s, timeout, cancelled, opaque, error) < 0) return finish(s, -1);
    if (!s->frame->buf[0]) return finish(s, fail(s, "no_current_frame", "decode a frame before copying its pixels"));
    metadata(s, frame);
    return finish(s, rgba(s, pixels, length));
}
int deadpan_source_seek(DeadpanSource *s, int64_t pts, uint64_t timeout, DeadpanCancelled cancelled,
                        const void *opaque, DeadpanSourceError *error) {
    if (begin(s, timeout, cancelled, opaque, error) < 0) return finish(s, -1);
    if (pts == AV_NOPTS_VALUE) return finish(s, fail(s, "invalid_timestamp", "seek timestamp is reserved for unknown PTS"));
    av_frame_unref(s->frame); av_packet_unref(s->packet);
    int result = av_seek_frame(s->format, s->stream, pts, AVSEEK_FLAG_BACKWARD);
    if (result < 0) return finish(s, fferror(s, "seek source", result));
    avcodec_flush_buffers(s->decoder);
    s->draining = 0; s->ended = 0; s->frames = 0; s->packets = 0;
    return finish(s, check(s));
}
