/* Bounded VP9 uncompressed headers, before FFmpeg's superframe splitter.
   The admitted MP4 binding has exactly one displayed picture per sample,
   preceded by at most seven hidden reference pictures. All references keep
   the immutable sample-entry raster. FFmpeg ignores VP9 render_size, so do
   not silently admit a different display transform. No compressed data is
   parsed here; the pinned software decoder checks it under our callbacks. */
static int vp9_color(DeadpanSource *s, Bits *b, unsigned profile) {
    unsigned depth = profile == 2 ? 10 + 2 * bit(b) : 8;
    static const unsigned matrices[8] = {2, 5, 1, 6, 7, 9, 3, 0};
    unsigned matrix = matrices[bits(b, 3)];
    unsigned full = bit(b);
    if (depth != s->limits.vp9[1] || matrix != s->limits.vp9[6] || full != s->limits.vp9[3])
        return fail(s, "stream_changed", "VP9 bitstream and vpcC depth, matrix or range disagree");
    return 1;
}
static int vp9_frame(DeadpanSource *s, const uint8_t *data, size_t length, int *shown) {
    Bits b = {data, length * 8, 0, 0};
    if (bits(&b, 2) != 2) return fail(s, "invalid_input", "invalid VP9 frame marker");
    unsigned profile = bit(&b);
    profile |= bit(&b) << 1;
    if (profile != s->limits.vp9[0])
        return fail(s, "unsupported_codec", "VP9 packet profile differs from its admitted configuration");
    if (bit(&b)) { /* show_existing_frame, no new raster or color */
        (void)bits(&b, 3);
        if (b.failed) return fail(s, "invalid_input", "truncated VP9 existing-frame header");
        *shown = 1;
        return 1;
    }
    int key = !bit(&b);
    *shown = (int)bit(&b);
    int resilient = (int)bit(&b);
    unsigned width, height;
    if (key) {
        if (bits(&b, 24) != 0x498342) return fail(s, "invalid_input", "invalid VP9 keyframe sync");
        if (vp9_color(s, &b, profile) < 0) return -1;
        width = bits(&b, 16) + 1;
        height = bits(&b, 16) + 1;
    } else {
        int intra = !*shown && bit(&b);
        if (!resilient) (void)bits(&b, 2);
        if (intra) {
            if (bits(&b, 24) != 0x498342) return fail(s, "invalid_input", "invalid VP9 intra-only sync");
            if (profile) {
                if (vp9_color(s, &b, profile) < 0) return -1;
            } else if (s->limits.vp9[6] != 5 || s->limits.vp9[3]) {
                return fail(s, "stream_changed", "VP9 intra-only picture changes its color interpretation");
            }
            (void)bits(&b, 8); /* refresh_frame_flags */
            width = bits(&b, 16) + 1;
            height = bits(&b, 16) + 1;
        } else {
            (void)bits(&b, 8); /* refresh_frame_flags */
            (void)bits(&b, 12); /* three reference indexes and sign biases */
            int referenced = 0;
            for (unsigned i = 0; i < 3; i++) {
                if (bit(&b)) { referenced = 1; break; }
            }
            AVCodecParameters *p = s->format->streams[s->stream]->codecpar;
            width = referenced ? (unsigned)p->width : bits(&b, 16) + 1;
            height = referenced ? (unsigned)p->height : bits(&b, 16) + 1;
        }
    }
    if (bit(&b)) {
        unsigned display_width = bits(&b, 16) + 1;
        unsigned display_height = bits(&b, 16) + 1;
        if (width != display_width || height != display_height)
            return fail(s, "unsupported_transform", "VP9 render size differs from its coded raster");
    }
    if (b.failed) return fail(s, "invalid_input", "truncated VP9 uncompressed header");
    if (geometry(s, (int)width, (int)height, "VP9 packet") < 0) return -1;
    AVCodecParameters *p = s->format->streams[s->stream]->codecpar;
    if (width != (unsigned)p->width || height != (unsigned)p->height)
        return fail(s, "stream_changed", "VP9 packet raster differs from its sample entry");
    return 1;
}
static int vp9_packet(DeadpanSource *s) {
    const uint8_t *data = s->packet->data;
    size_t length = (size_t)s->packet->size;
    unsigned marker = data[length - 1];
    unsigned frames = 1, bytes = 0;
    size_t payload = length;
    if ((marker & 0xe0) == 0xc0) {
        frames = (marker & 7) + 1;
        bytes = ((marker >> 3) & 3) + 1;
        size_t index = 2 + frames * bytes;
        if (length < index || data[length - index] != marker)
            return fail(s, "invalid_input", "invalid VP9 superframe index");
        payload -= index;
    }
    size_t position = 0;
    for (unsigned frame = 0; frame < frames; frame++) {
        size_t size = payload;
        if (bytes) {
            size = 0;
            for (unsigned i = 0; i < bytes; i++)
                size |= (size_t)data[payload + 1 + frame * bytes + i] << (8 * i);
        }
        if (!size || size > payload - position)
            return fail(s, "invalid_input", "VP9 superframe exceeds its packet");
        int shown = 0;
        if (vp9_frame(s, data + position, size, &shown) < 0) return -1;
        if (shown != (frame + 1 == frames))
            return fail(s, "unsupported_timing", "VP9 MP4 sample needs exactly one final displayed picture");
        position += size;
    }
    if (position != payload) return fail(s, "invalid_input", "VP9 superframe has trailing payload");
    return 1;
}
