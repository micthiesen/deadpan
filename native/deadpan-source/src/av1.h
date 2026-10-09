/* Allocation and interpretation admission for low-overhead AV1 Main streams.
   This parses sequence headers and packet envelopes, never entropy-coded tiles.
   The pinned libdav1d decoder owns that work under strict compliance, the pixel
   limit and one-frame delay. No sequence or packet can change those bounds. */
typedef struct { uint32_t magic; int key; int64_t pts; } Av1PacketIdentity;
#define AV1_PACKET_MAGIC 0x44504131

static int av1_leb(DeadpanSource *s, const uint8_t *data, size_t length, size_t *at, uint32_t *out) {
    uint64_t value = 0;
    for (unsigned i = 0; i < 8 && *at < length; i++) {
        uint8_t byte = data[(*at)++];
        value |= (uint64_t)(byte & 127) << (i * 7);
        if (!(byte & 128)) {
            if (value > UINT32_MAX) break;
            *out = (uint32_t)value;
            return 1;
        }
    }
    return fail(s, "invalid_input", "invalid AV1 LEB128 length");
}
static int av1_sequence(DeadpanSource *s, const uint8_t *data, size_t length) {
    if (!length || length > 1024)
        return fail(s, "resource_limit", "AV1 sequence header exceeds 1024 bytes");
    Bits b = {data, length * 8, 0, 0};
    unsigned profile = bits(&b, 3), still = bit(&b), reduced = bit(&b);
    if (profile || (reduced && !still))
        return fail(s, "unsupported_codec", "AV1 requires a valid Main profile sequence");
    unsigned level, tier = 0;
    if (reduced) {
        level = bits(&b, 5);
    } else {
        unsigned model = 0, delay_bits = 0;
        if (bit(&b)) {
            uint32_t tick = bits(&b, 32), scale = bits(&b, 32);
            if (!tick || !scale) return fail(s, "invalid_input", "invalid AV1 sequence timing");
            if (bit(&b) && ue(&b) == UINT32_MAX)
                return fail(s, "invalid_input", "invalid AV1 picture interval");
            model = bit(&b);
            if (model) {
                delay_bits = bits(&b, 5) + 1;
                if (!bits(&b, 32)) return fail(s, "invalid_input", "invalid AV1 decoding tick");
                (void)bits(&b, 10);
            }
        }
        unsigned display = bit(&b);
        if (bits(&b, 5) || bits(&b, 12))
            return fail(s, "unsupported_codec", "AV1 requires one operating point without layer selection");
        level = bits(&b, 5);
        if (level > 7) tier = bit(&b);
        if (model && bit(&b)) {
            (void)bits(&b, (int)delay_bits);
            (void)bits(&b, (int)delay_bits);
            (void)bit(&b);
        }
        if (display && bit(&b)) (void)bits(&b, 4);
    }
    unsigned width_bits = bits(&b, 4) + 1, height_bits = bits(&b, 4) + 1;
    unsigned width = bits(&b, (int)width_bits) + 1, height = bits(&b, (int)height_bits) + 1;
    /* dav1d bounds the actual pixel product, but a frame-size override can
       exceed the sequence's declared maximum. Bound its entire syntax range
       before dav1d sees any frame; do not rely on our unused buffer callback. */
    if ((1u << width_bits) > s->limits.max_dimension || (1u << height_bits) > s->limits.max_dimension ||
        geometry(s, (int)width, (int)height, "AV1 sequence") < 0)
        return fail(s, "resource_limit", "AV1 frame-size envelope exceeds configured dimensions");
    AVCodecParameters *p = s->format->streams[s->stream]->codecpar;
    if (width != (unsigned)p->width || height != (unsigned)p->height)
        return fail(s, "stream_changed", "AV1 sequence and container raster disagree");
    if (!reduced && bit(&b)) {
        unsigned delta = bits(&b, 4) + 2;
        if (delta + bits(&b, 3) + 1 > 16)
            return fail(s, "invalid_input", "AV1 frame identity exceeds sixteen bits");
    }
    (void)bits(&b, 3); /* superblock, filter intra, intra edge */
    if (!reduced) {
        (void)bits(&b, 4); /* inter intra, masked compound, warped motion, dual filter */
        unsigned order = bit(&b);
        if (order) (void)bits(&b, 2);
        unsigned screen = bit(&b) ? 2 : bit(&b);
        if (screen && !bit(&b)) (void)bit(&b);
        if (order) (void)bits(&b, 3);
    }
    (void)bits(&b, 3); /* superres, CDEF, restoration */
    unsigned high = bit(&b), mono = bit(&b);
    unsigned primaries = 2, transfer = 2, matrix = 2;
    if (bit(&b)) {
        primaries = bits(&b, 8); transfer = bits(&b, 8); matrix = bits(&b, 8);
    }
    if (mono || (primaries == 1 && transfer == 13 && matrix == 0))
        return fail(s, "unsupported_pixel_format", "AV1 requires Main 4:2:0 color pictures");
    unsigned full = bit(&b), chroma = bits(&b, 2);
    (void)bit(&b); /* separate UV delta Q */
    (void)bit(&b); /* film grain */
    if (!bit(&b)) b.failed = 1;
    while (b.position < b.bits && !b.failed) if (bit(&b)) b.failed = 1;
    if (b.failed) return fail(s, "invalid_input", "truncated or invalid AV1 sequence header");
    if (level != (s->av1_config[1] & 31) || tier != (s->av1_config[2] >> 7) ||
        high != ((s->av1_config[2] >> 6) & 1) || chroma != (s->av1_config[2] & 3))
        return fail(s, "stream_changed", "AV1 sequence and av1C interpretation disagree");
    int range = full ? AVCOL_RANGE_JPEG : AVCOL_RANGE_MPEG;
    unsigned interpretation[4] = {primaries, transfer, matrix, full};
    if (s->av1_sequence_seen && (s->av1_reduced != (int)reduced ||
        memcmp(interpretation, s->av1_color, sizeof(interpretation))))
        return fail(s, "stream_changed", "AV1 sequence interpretation changed");
    if ((p->color_primaries != AVCOL_PRI_UNSPECIFIED && (unsigned)p->color_primaries != primaries) ||
        (p->color_trc != AVCOL_TRC_UNSPECIFIED && (unsigned)p->color_trc != transfer) ||
        (p->color_space != AVCOL_SPC_UNSPECIFIED && (unsigned)p->color_space != matrix) ||
        (p->color_range != AVCOL_RANGE_UNSPECIFIED && (int)p->color_range != range) ||
        (p->chroma_location != AVCHROMA_LOC_UNSPECIFIED &&
         p->chroma_location != (chroma == 1 ? AVCHROMA_LOC_LEFT : AVCHROMA_LOC_TOPLEFT)))
        return fail(s, "stream_changed", "AV1 sequence and container color interpretation disagree");
    if (color(s, AV_CODEC_ID_AV1, high ? AV_PIX_FMT_YUV420P10LE : AV_PIX_FMT_YUV420P,
              range, (int)matrix, (int)transfer, (int)primaries) < 0) return -1;
    // Encoders can refine compression-tool flags after writing av1C. Recheck
    // their syntax and allocation envelope; retain the immutable interpretation.
    memcpy(s->av1_color, interpretation, sizeof(interpretation));
    s->av1_sequence_seen = 1;
    s->av1_reduced = (int)reduced;
    return 1;
}
static int av1_metadata(DeadpanSource *s, const uint8_t *data, size_t length, int configuration) {
    size_t at = 0;
    uint32_t type;
    if (av1_leb(s, data, length, &at, &type) < 0) return -1;
    // The common static HDR contract validates values and exact repetition on
    // decoded pictures. Reject dynamic HDR, scalability and timecode here,
    // including forms libdav1d otherwise ignores without frame side data.
    size_t bytes = type == 1 ? 4 : type == 2 ? 24 : 0;
    if (!bytes) return fail(s, "unsupported_metadata", "AV1 metadata type is not qualified");
    if (length - at != bytes + 1 || data[length - 1] != 0x80)
        return fail(s, "invalid_input", "invalid AV1 static metadata length or trailing bits");
    AVCodecParameters *p = s->format->streams[s->stream]->codecpar;
    Bits b = {data + at, bytes * 8, 0, 0};
    enum AVPacketSideDataType kind;
    const void *value;
    size_t value_bytes;
    if (type == 1) {
        AVContentLightMetadata light = {.MaxCLL=bits(&b, 16), .MaxFALL=bits(&b, 16)};
        if ((s->av1_has_light && !same_raw_light(&s->av1_light, &light)) ||
            (!s->av1_has_light && s->inventory_ready && !s->has_raw_light))
            return fail(s, "stream_changed", "AV1 content light metadata changed");
        kind = AV_PKT_DATA_CONTENT_LIGHT_LEVEL;
        const AVPacketSideData *side = av_packet_side_data_get(p->coded_side_data, p->nb_coded_side_data, kind);
        AVContentLightMetadata declared;
        if (side && (read_light(s, side->data, side->size, &declared) < 0 || !same_raw_light(&declared, &light)))
            return fail(s, "stream_changed", "AV1 and container content light metadata disagree");
        s->av1_light = light;
        s->av1_has_light = 1;
        value = &s->av1_light; value_bytes = sizeof(s->av1_light);
    } else {
        AVMasteringDisplayMetadata mastering = {.has_primaries=1, .has_luminance=1};
        for (int c = 0; c < 3; c++)
            for (int axis = 0; axis < 2; axis++)
                mastering.display_primaries[c][axis] = (AVRational){(int)bits(&b, 16), 1 << 16};
        for (int axis = 0; axis < 2; axis++)
            mastering.white_point[axis] = (AVRational){(int)bits(&b, 16), 1 << 16};
        uint32_t maximum = bits(&b, 32), minimum = bits(&b, 32);
        if (maximum > INT_MAX || minimum > INT_MAX)
            return fail(s, "unsupported_hdr", "AV1 mastering luminance exceeds the decoder's rational range");
        mastering.max_luminance = (AVRational){(int)maximum, 1 << 8};
        mastering.min_luminance = (AVRational){(int)minimum, 1 << 14};
        if ((s->av1_has_mastering && !same_raw_mastering(&s->av1_mastering, &mastering)) ||
            (!s->av1_has_mastering && s->inventory_ready && !s->has_raw_mastering))
            return fail(s, "stream_changed", "AV1 mastering display metadata changed");
        kind = AV_PKT_DATA_MASTERING_DISPLAY_METADATA;
        const AVPacketSideData *side = av_packet_side_data_get(p->coded_side_data, p->nb_coded_side_data, kind);
        AVMasteringDisplayMetadata declared;
        if (side && (read_mastering(s, side->data, side->size, &declared) < 0 || !same_raw_mastering(&declared, &mastering)))
            return fail(s, "stream_changed", "AV1 and container mastering metadata disagree");
        s->av1_mastering = mastering;
        s->av1_has_mastering = 1;
        value = &s->av1_mastering; value_bytes = sizeof(s->av1_mastering);
    }
    if (configuration && !av_packet_side_data_get(p->coded_side_data, p->nb_coded_side_data, kind)) {
        // libdav1d only parses sequence fields from extradata. Preserve av1C
        // static metadata as stream side data so it reaches ordinary capture
        // and cannot disappear when absent from the first temporal unit.
        AVPacketSideData *side = av_packet_side_data_new(&p->coded_side_data, &p->nb_coded_side_data, kind, value_bytes, 0);
        if (!side) return fail(s, "resource_exhausted", "retain AV1 configuration metadata");
        memcpy(side->data, value, value_bytes);
    }
    return 1;
}
static int av1_obus(DeadpanSource *s, const uint8_t *data, size_t length, int configuration, int *key) {
    size_t at = 0;
    unsigned count = 0, frames = 0, shown = 0, sequences = 0;
    int tile_header = 0;
    *key = 0;
    while (at < length) {
        if (check(s) < 0) return -1;
        if (++count > 256) return fail(s, "resource_limit", "AV1 packet exceeds 256 OBUs");
        unsigned header = data[at++], type = (header >> 3) & 15;
        if ((header & 0x83) != 2)
            return fail(s, "invalid_input", "AV1 requires sized OBUs with valid reserved bits");
        if (header & 4) {
            if (at == length || data[at++] != 0)
                return fail(s, "unsupported_codec", "AV1 multilayer OBU is not qualified");
        }
        uint32_t size;
        if (av1_leb(s, data, length, &at, &size) < 0) return -1;
        if (size > length - at) return fail(s, "invalid_input", "AV1 OBU escapes its packet");
        const uint8_t *payload = data + at;
        at += size;
        if (configuration && type != 1 && type != 5)
            return fail(s, "unsupported_codec", "AV1 configuration requires only sequence and metadata OBUs");
        if (type == 1) {
            if (++sequences > 1 || frames || (configuration && count != 1))
                return fail(s, "invalid_input", "misplaced or duplicate AV1 sequence header");
            if (av1_sequence(s, payload, size) < 0) return -1;
        } else if (type == 2) {
            if (size || count != 1) return fail(s, "invalid_input", "invalid AV1 temporal delimiter");
        } else if (type == 3 || type == 6) {
            if (!s->av1_sequence_seen) return fail(s, "invalid_input", "AV1 picture precedes its sequence header");
            if (++frames > 32) return fail(s, "resource_limit", "AV1 packet exceeds 32 frame headers");
            if (shown) return fail(s, "unsupported_timing", "AV1 packet has pictures after its displayed frame");
            Bits b = {payload, (size_t)size * 8, 0, 0};
            unsigned existing = s->av1_reduced ? 0 : bit(&b);
            unsigned frame_type = 0;
            if (existing) {
                (void)bits(&b, 3);
                shown = 1;
            } else {
                if (!s->av1_reduced) frame_type = bits(&b, 2);
                shown = s->av1_reduced ? 1 : bit(&b);
            }
            if (!size || b.failed) return fail(s, "invalid_input", "truncated AV1 frame header");
            *key = !existing && !frame_type && shown && frames == 1;
            tile_header = type == 3 && !existing;
        } else if (type == 4) {
            if (!tile_header || !size) return fail(s, "invalid_input", "AV1 tile group has no frame header");
        } else if (type == 5) {
            if (av1_metadata(s, payload, size, configuration) < 0) return -1;
        } else if (type == 15) {
            for (uint32_t i = 0; i < size; i++)
                if (payload[i]) return fail(s, "invalid_input", "AV1 padding is not zero");
        } else {
            return fail(s, "unsupported_codec", "AV1 OBU type is not qualified");
        }
    }
    if (!configuration && shown != 1)
        return fail(s, "unsupported_timing", "AV1 packet requires exactly one displayed picture");
    if (!configuration) {
        // libdav1d doesn't call our get_buffer2 counter. Charge every submitted
        // frame header (including show-existing/film-grain work), conservatively.
        uint64_t pictures = atomic_load(&s->pictures);
        if (pictures > UINT64_MAX - frames) return fail(s, "resource_limit", "AV1 picture work overflow");
        atomic_fetch_add(&s->pictures, frames);
    }
    return 1;
}
static int av1_configuration(DeadpanSource *s, const uint8_t *data, size_t length) {
    if (length < 4 || data[0] != 0x81 || (data[1] >> 5) ||
        ((data[1] & 31) > 23 && (data[1] & 31) != 31) ||
        (data[2] & 0x30) || (data[2] & 12) != 12 ||
        !(data[2] & 3) || (data[2] & 3) == 3 ||
        ((data[1] & 31) <= 7 && (data[2] & 128)) ||
        (data[3] & 0xe0) || (!(data[3] & 16) && (data[3] & 15)))
        return fail(s, "unsupported_codec", "AV1 requires valid Main eight/ten-bit 4:2:0 av1C with explicit chroma siting");
    memcpy(s->av1_config, data, 4);
    int key;
    return av1_obus(s, data + 4, length - 4, 1, &key);
}
static int av1_packet(DeadpanSource *s) {
    int key;
    if (av1_obus(s, s->packet->data, (size_t)s->packet->size, 0, &key) < 0) return -1;
    av_buffer_unref(&s->packet->opaque_ref);
    s->packet->opaque_ref = av_buffer_alloc(sizeof(Av1PacketIdentity));
    if (!s->packet->opaque_ref) return fail(s, "resource_exhausted", "retain AV1 packet identity");
    Av1PacketIdentity identity = {.magic=AV1_PACKET_MAGIC, .key=key, .pts=s->packet->pts};
    memcpy(s->packet->opaque_ref->data, &identity, sizeof(identity));
    return 1;
}
static int av1_picture(DeadpanSource *s) {
    AVFrame *frame = s->frame;
    if (!frame->opaque_ref || frame->opaque_ref->size != sizeof(Av1PacketIdentity))
        return fail(s, "decode_protocol", "AV1 picture lost its guarded packet identity");
    Av1PacketIdentity identity;
    memcpy(&identity, frame->opaque_ref->data, sizeof(identity));
    if (identity.magic != AV1_PACKET_MAGIC || identity.pts != frame->pts)
        return fail(s, "decode_protocol", "AV1 picture has a mismatched packet identity");
    // A show-existing key picture depends on an earlier hidden key. It is not
    // a seek anchor, despite libdav1d reporting its referenced KEY frame type.
    if (!identity.key) frame->flags &= ~AV_FRAME_FLAG_KEY;
    else if (!(frame->flags & AV_FRAME_FLAG_KEY))
        return fail(s, "decode_protocol", "AV1 decoder disagrees with the guarded key picture");
    return 1;
}
