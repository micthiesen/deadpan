/* Pre-decode ProRes 422 admission. Included only by decoder.c.
   RDD 36 frame/picture/slice envelopes are checked before libavcodec sees a
   packet. Entropy decoding remains the pinned decoder's responsibility.
   No allocation, and every offset is bounded by its enclosing packet. */
static unsigned prores_u16(const uint8_t *p) {
    return ((unsigned)p[0] << 8) | p[1];
}
static uint32_t prores_u32(const uint8_t *p) {
    return ((uint32_t)p[0] << 24) | ((uint32_t)p[1] << 16) | ((uint32_t)p[2] << 8) | p[3];
}
static int prores_tag(uint32_t tag) {
    return tag == MKTAG('a','p','c','o') || tag == MKTAG('a','p','c','s') ||
           tag == MKTAG('a','p','c','n') || tag == MKTAG('a','p','c','h');
}
static int prores_packet(DeadpanSource *s) {
    const uint8_t *data = s->packet->data;
    size_t length = (size_t)s->packet->size;
    AVStream *stream = s->format->streams[s->stream];
    const AVCodecParameters *p = stream->codecpar;
    if (length < 36 || prores_u32(data) != length || memcmp(data + 4, "icpf", 4))
        return fail(s, "unsupported_codec", "ProRes packet lacks one complete icpf frame");
    const uint8_t *h = data + 8;
    unsigned header = prores_u16(h), version = prores_u16(h + 2);
    unsigned width = prores_u16(h + 8), height = prores_u16(h + 10);
    unsigned field = (h[12] >> 2) & 3, quant = h[19];
    if (geometry(s, (int)width, (int)height, "ProRes packet") < 0) return -1;
    if (width != (unsigned)p->width || height != (unsigned)p->height)
        return fail(s, "stream_changed", "ProRes packet dimensions differ from its sample entry");
    if (version > 1 || (h[12] >> 6) != 2 || (h[12] & 0x33) || field == 3 ||
        (h[17] != 0 && h[17] != 0x30) || h[18] || quant > 3 || (field && (height & 1)))
        return fail(s, "unsupported_codec", "ProRes requires version 0/1 ten-bit 422 without alpha, reserved flags or odd-height fields");
    // The upper nibble here is reserved in RDD 36:2022. Apple's measured 422
    // encoder writes 0x30; it has no decoding semantics and the alpha nibble
    // remains zero. Do not mistake that reserved value for encoded alpha.
    if (header != 20U + ((quant & 2) ? 64U : 0U) + ((quant & 1) ? 64U : 0U) || header > length - 8)
        return fail(s, "unsupported_codec", "ProRes frame header or quantization matrices are truncated or extended");
    for (unsigned i = 20; i < header; i++)
        if (h[i] < 2 || h[i] > 63)
            return fail(s, "unsupported_codec", "ProRes quantization matrix entry is outside 2..63");
    if (hdr_transfer(h[15]))
        return fail(s, "unsupported_transfer", "ProRes HDR and camera-log sources are not yet qualified");
    if (color(s, AV_CODEC_ID_PRORES, AV_PIX_FMT_YUV422P10LE, AVCOL_RANGE_MPEG, h[16], h[15], h[14]) < 0) return -1;
    if ((p->color_primaries != AVCOL_PRI_UNSPECIFIED && p->color_primaries != h[14]) ||
        (p->color_trc != AVCOL_TRC_UNSPECIFIED && p->color_trc != h[15]) ||
        (p->color_space != AVCOL_SPC_UNSPECIFIED && p->color_space != h[16]) ||
        (p->color_range != AVCOL_RANGE_UNSPECIFIED && p->color_range != AVCOL_RANGE_MPEG))
        return fail(s, "stream_changed", "ProRes frame and container color declarations disagree");
    // Frame header hints may not override the container's exact timing or SAR.
    unsigned aspect = h[13] >> 4, rate = h[13] & 15;
    AVRational sample_aspect = sar(stream->sample_aspect_ratio.num ? stream->sample_aspect_ratio : p->sample_aspect_ratio);
    static const AVRational rates[] = {{0,1},{24000,1001},{24,1},{25,1},{30000,1001},{30,1},
                                      {50,1},{60000,1001},{60,1},{100,1},{120000,1001},{120,1}};
    if (aspect > 3 || rate > 11 ||
        (aspect == 1 && av_cmp_q(sample_aspect, (AVRational){1,1})) ||
        (aspect == 2 && (int64_t)width * sample_aspect.num * 3 != (int64_t)height * sample_aspect.den * 4) ||
        (aspect == 3 && (int64_t)width * sample_aspect.num * 9 != (int64_t)height * sample_aspect.den * 16))
        return fail(s, "unsupported_transform", "ProRes aspect hint disagrees with the sample entry");
    if (rate && av_cmp_q(stream->avg_frame_rate, rates[rate]))
        return fail(s, "unsupported_timing", "ProRes rate hint disagrees with the container timing");
    if ((p->field_order == AV_FIELD_PROGRESSIVE && field) ||
        ((p->field_order == AV_FIELD_TT || p->field_order == AV_FIELD_TB) && field != 1) ||
        ((p->field_order == AV_FIELD_BB || p->field_order == AV_FIELD_BT) && field != 2))
        return fail(s, "unsupported_interlace", "ProRes field order disagrees with the container declaration");
    // An unspecified rate/aspect hint may become explicit on later frames
    // (Apple's encoder does this). Each hint above already agrees with the
    // authoritative container; only the actual decoded interpretation is fixed.
    uint8_t interpretation[4] = {(uint8_t)field,h[14],h[15],h[16]};
    if (s->prores_header_seen && memcmp(s->prores_interpretation, interpretation, sizeof(interpretation)))
        return fail(s, "stream_changed", "ProRes picture interpretation changed within the stream");
    memcpy(s->prores_interpretation, interpretation, sizeof(interpretation));
    s->prores_header_seen = 1;
    size_t position = 8 + header;
    unsigned mb_width = (width + 15) / 16, mb_height = (height + (field ? 31 : 15)) / (field ? 32 : 16);
    for (unsigned picture = 0; picture < (field ? 2U : 1U); picture++) {
        if (length - position < 8)
            return fail(s, "unsupported_codec", "ProRes picture header is truncated");
        const uint8_t *pic = data + position;
        size_t size = prores_u32(pic + 1);
        if (pic[0] != 0x40 || (pic[7] & 0xcf) || size < 8 || size > length - position)
            return fail(s, "unsupported_codec", "ProRes picture length or slice geometry is unsupported");
        unsigned shift = pic[7] >> 4;
        unsigned remainder = mb_width & ((1U << shift) - 1), tail = 0;
        for (; remainder; remainder >>= 1) tail += remainder & 1;
        unsigned slices = mb_height * ((mb_width >> shift) + tail);
        size_t cursor = 8 + (size_t)slices * 2;
        if (!slices || cursor > size)
            return fail(s, "unsupported_codec", "ProRes picture slice table is truncated");
        // RDD 36 marks the stored slice count deprecated; geometry is authoritative.
        for (unsigned i = 0; i < slices; i++) {
            if (!(i & 255) && check(s) < 0) return -1;
            size_t slice_size = prores_u16(pic + 8 + (size_t)i * 2);
            if (slice_size < 6 || slice_size > size - cursor)
                return fail(s, "unsupported_codec", "ProRes slice escapes its picture");
            const uint8_t *slice = pic + cursor;
            unsigned y_size = prores_u16(slice + 2), cb_size = prores_u16(slice + 4);
            if (slice[0] != 0x30 || !slice[1] || slice[1] > 224 || !y_size || !cb_size ||
                (size_t)y_size + cb_size >= slice_size - 6)
                return fail(s, "unsupported_codec", "ProRes slice has invalid plane sizes or quantization");
            cursor += slice_size;
        }
        if (cursor != size)
            return fail(s, "unsupported_codec", "ProRes picture has undeclared trailing bytes");
        position += size;
    }
    for (; position < length; position++) {
        if (!(position & 4095) && check(s) < 0) return -1;
        if (data[position]) return fail(s, "unsupported_codec", "ProRes frame stuffing is not zero");
    }
    return 1;
}
