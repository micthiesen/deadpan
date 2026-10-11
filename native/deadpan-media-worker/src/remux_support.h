/* Private stream-copy helpers; included after RemuxInput in converter.c.
   No decoder, encoder, secondary input, or timestamp tolerance for MP4. */

static AVRational remux_packet_clock(const RemuxInput *input) {
    return input->opus ? (AVRational){1, 48000} : input->format->streams[0]->time_base;
}

static int remux_exact_time(int64_t value, AVRational from, AVRational to, int64_t *result) {
    if (value == AV_NOPTS_VALUE || from.num <= 0 || from.den <= 0 || to.num <= 0 || to.den <= 0)
        return fail("invalid_media", "remux clock is missing or invalid");
    __int128 numerator = (__int128)value * from.num * to.den;
    __int128 denominator = (__int128)from.den * to.num;
    __int128 scaled = numerator / denominator;
    if (numerator % denominator || scaled <= INT64_MIN || scaled > INT64_MAX)
        return fail("invalid_media", "remux clock conversion is not exact");
    *result = (int64_t)scaled;
    return 1;
}

static int remux_rescale_packet(AVPacket *packet, AVRational from, AVRational to) {
    return remux_exact_time(packet->pts, from, to, &packet->pts) &&
           remux_exact_time(packet->dts, from, to, &packet->dts) &&
           remux_exact_time(packet->duration, from, to, &packet->duration);
}

static int remux_opus_packet(RemuxInput *input, const AVStream *stream, AVPacket *packet) {
    const AVCodecParameters *par = stream->codecpar;
    if (packet->size < 1 || input->opus_terminal || packet->pts == AV_NOPTS_VALUE ||
        (packet->flags & ~(AV_PKT_FLAG_KEY | AV_PKT_FLAG_DISCARD)))
        return fail("invalid_media", "Opus packet is absent or follows terminal padding");
    unsigned config = packet->data[0] >> 3;
    static const int silk[] = {480, 960, 1920, 2880};
    static const int celt[] = {120, 240, 480, 960};
    int duration = config < 12 ? silk[config & 3] : config < 16 ? 480 << (config & 1) : celt[config & 3];
    unsigned code = packet->data[0] & 3;
    if (code == 3 && packet->size < 2)
        return fail("invalid_media", "truncated Opus frame count");
    unsigned frames = code == 0 ? 1 : code == 3 ? packet->data[1] & 63 : 2;
    int count = (int)frames * duration;
    if (frames == 0 || frames > 48 || count > 5760)
        return fail("invalid_media", "Opus packet exceeds its physical sample bound");
    size_t skip_size = 0;
    uint8_t *skip = av_packet_get_side_data(packet, AV_PKT_DATA_SKIP_SAMPLES, &skip_size);
    if (skip && skip_size != 10) return fail("invalid_media", "Opus skip evidence has an invalid size");
    uint32_t leading = skip ? AV_RL32(skip) : 0;
    uint32_t trailing = skip ? AV_RL32(skip + 4) : 0;
    if ((skip && (skip[8] || skip[9])) ||
        (leading && (input->packets || leading != (uint32_t)par->initial_padding)) || trailing >= (uint32_t)count)
        return fail("invalid_media", "Opus packet has unsupported skip evidence");
    for (int index = 0; index < packet->side_data_elems; index++) {
        if (packet->side_data[index].type != AV_PKT_DATA_SKIP_SAMPLES)
            return fail("invalid_media", "Opus packet has unqualified side data");
    }
    if (input->packets == 0) {
        if (input->matroska) {
            const DeadpanRemuxRequest *a = input->admission;
            __int128 first = (__int128)a->opus_first_block_timestamp * a->opus_timestamp_scale_ns * 48000;
            if (first % 1000000000 || first / 1000000000 < 0 || first / 1000000000 > INT64_MAX)
                return fail("invalid_media", "Opus initial block does not name a nonnegative exact sample");
            input->opus_origin = (int64_t)(first / 1000000000) - par->initial_padding;
        } else {
            if (stream->time_base.num != 1 || stream->time_base.den != 48000)
                return fail("invalid_media", "MP4 Opus must use an exact 48000 Hz clock");
            input->opus_origin = packet->pts;
        }
        if (input->opus_origin > INT64_MAX - par->initial_padding ||
            input->opus_origin < -(int64_t)par->initial_padding)
            return fail("invalid_media", "Opus available origin is outside the admitted movie clock");
        input->opus_offset = input->opus_origin + par->initial_padding;
    }
    if (input->opus_samples > INT64_MAX || input->opus_origin > INT64_MAX - (int64_t)input->opus_samples)
        return fail("invalid_media", "Opus sample coordinate overflow");
    int64_t exact = input->opus_origin + (int64_t)input->opus_samples;
    int64_t presented = count;
    if (input->matroska) {
        const DeadpanRemuxRequest *a = input->admission;
        if ((__int128)stream->time_base.num * 1000000000 != (__int128)stream->time_base.den * a->opus_timestamp_scale_ns)
            return fail("invalid_media", "Opus demux clock disagrees with admitted timestamp scale");
        int64_t delay = (int64_t)((a->opus_codec_delay_ns + a->opus_timestamp_scale_ns / 2) / a->opus_timestamp_scale_ns);
        __int128 ticks = (__int128)packet->pts + delay - a->opus_first_block_timestamp;
        __int128 observed = ticks * a->opus_timestamp_scale_ns * 48000;
        __int128 expected = (__int128)input->opus_samples * 1000000000;
        __int128 tolerance = (__int128)a->opus_timestamp_scale_ns * 48000;
        __int128 reported = (__int128)packet->duration * a->opus_timestamp_scale_ns * 48000;
        __int128 physical = (__int128)count * 1000000000;
        __int128 trimmed = (__int128)(count - trailing) * 1000000000;
        if ((!input->packets && ticks != 0) || observed < expected - tolerance || observed > expected + tolerance ||
            packet->duration <= 0 || ((reported < physical - tolerance || reported > physical + tolerance) &&
            (reported < trimmed - tolerance || reported > trimmed + tolerance)))
            return fail("invalid_media", "Opus packet clock disagrees with exact sample continuity");
        presented -= trailing;
    } else {
        if (packet->pts != exact || packet->dts != exact || packet->duration <= 0 || packet->duration > count || trailing)
            return fail("invalid_media", "MP4 Opus packet clock or trim is inconsistent");
        presented = packet->duration;
    }
    input->opus_terminal = presented < count;
    av_packet_free_side_data(packet);
    if (!input->packets && par->initial_padding) {
        skip = av_packet_new_side_data(packet, AV_PKT_DATA_SKIP_SAMPLES, 10);
        if (!skip) return fail("resource_exhausted", "allocate Opus pre-skip evidence");
        memset(skip, 0, 10);
        AV_WL32(skip, par->initial_padding);
    }
    packet->pts = packet->dts = exact;
    packet->duration = presented;
    packet->flags = AV_PKT_FLAG_KEY | (input->opus_samples + count <= (uint64_t)par->initial_padding ? AV_PKT_FLAG_DISCARD : 0);
    input->opus_samples += count;
    input->packets++;
    return within_deadline();
}

typedef struct { size_t at, size; } RemuxBox;

static const uint8_t remux_cadence_uuid[16] = {
    0x16, 0x4d, 0x86, 0xbf, 0xc8, 0x98, 0x48, 0x1b,
    0x99, 0x20, 0x91, 0x63, 0x3d, 0xae, 0xe6, 0x97
};
static const uint8_t remux_chroma_uuid[16] = {
    0xfa, 0xb3, 0x5b, 0x69, 0x35, 0x7d, 0x47, 0x8b,
    0x97, 0xe4, 0xc7, 0xf9, 0x6a, 0x65, 0x54, 0x32
};

static int remux_box(const uint8_t *bytes, size_t length, size_t at, RemuxBox *box) {
    if (at > length || length - at < 8) return fail("invalid_media", "truncated MP4 assembly box");
    uint32_t size = AV_RB32(bytes + at);
    if (size < 8 || size > length - at) return fail("invalid_media", "unsupported MP4 assembly box size");
    *box = (RemuxBox){at, size};
    return within_deadline();
}

static int remux_child(const uint8_t *bytes, size_t start, size_t end, uint32_t type, RemuxBox *found) {
    int seen = 0;
    for (size_t at = start; at < end;) {
        RemuxBox box;
        if (!remux_box(bytes, end, at, &box)) return 0;
        if (AV_RB32(bytes + at + 4) == type) {
            if (seen++) return fail("invalid_media", "duplicate MP4 assembly metadata");
            *found = box;
        }
        at += box.size;
    }
    return seen || fail("invalid_media", "missing MP4 assembly metadata");
}

static int remux_pread(int fd, uint8_t *bytes, size_t length, int64_t offset) {
    size_t done = 0;
    while (done < length) {
        if (!within_deadline()) return 0;
        ssize_t count = pread(fd, bytes + done, length - done, offset + (int64_t)done);
        if (count < 0 && errno == EINTR) continue;
        if (count <= 0) return fail("invalid_media", "cannot read MP4 assembly metadata");
        done += (size_t)count;
    }
    return 1;
}

static int remux_moov(int fd, int64_t base, int64_t length, uint8_t **bytes, size_t *size, int64_t *position) {
    unsigned boxes = 0;
    for (int64_t at = 0; at < length;) {
        if (++boxes > 100000) return fail("invalid_media", "MP4 assembly box count exceeds bound");
        uint8_t header[16];
        if (length - at < 8 || !remux_pread(fd, header, 8, base + at)) return 0;
        uint64_t total = AV_RB32(header);
        if (total == 1) {
            if (length - at < 16 || !remux_pread(fd, header + 8, 8, base + at + 8)) return 0;
            total = AV_RB64(header + 8);
        }
        if (total < (AV_RB32(header) == 1 ? 16u : 8u) || total > (uint64_t)(length - at))
            return fail("invalid_media", "MP4 assembly box escapes input");
        if (AV_RB32(header + 4) == MKBETAG('m','o','o','v')) {
            if (total > 16 * 1024 * 1024 || AV_RB32(header) == 1)
                return fail("invalid_media", "MP4 assembly movie metadata exceeds bound");
            *bytes = av_malloc((size_t)total);
            if (!*bytes) return fail("resource_exhausted", "allocate MP4 assembly metadata");
            *size = (size_t)total;
            *position = at;
            return remux_pread(fd, *bytes, *size, base + at);
        }
        at += (int64_t)total;
    }
    return fail("invalid_media", "MP4 assembly has no movie metadata");
}

static int remux_read_original(int fd, int64_t base, int64_t length, uint64_t *nominal, int *chroma_assumed_left) {
    uint8_t *bytes = NULL;
    size_t size = 0;
    int64_t position = 0;
    int ok = 0;
    *nominal = 0;
    *chroma_assumed_left = 0;
    if (!remux_moov(fd, base, length, &bytes, &size, &position)) goto done;
    for (size_t at = 8; at < size;) {
        RemuxBox box;
        if (!remux_box(bytes, size, at, &box)) goto done;
        if (AV_RB32(bytes + at + 4) == MKBETAG('u','u','i','d')) {
            if (box.size != 32) {
                fail("invalid_media", "malformed Original interpretation"); goto done;
            }
            if (!memcmp(bytes + at + 8, remux_chroma_uuid, 16)) {
                if (*chroma_assumed_left || AV_RB64(bytes + at + 24) != 1) {
                    fail("invalid_media", "invalid Original chroma interpretation"); goto done;
                }
                *chroma_assumed_left = 1;
            } else {
                if (*nominal || memcmp(bytes + at + 8, remux_cadence_uuid, 16) ||
                    !AV_RB64(bytes + at + 24) || AV_RB64(bytes + at + 24) > 1000000000) {
                    fail("invalid_media", "unqualified Original cadence metadata"); goto done;
                }
                *nominal = AV_RB64(bytes + at + 24);
            }
        }
        at += box.size;
    }
    ok = 1;
done:
    av_free(bytes);
    return ok;
}

static int remux_vp9_mp4(RemuxInput *input, AVCodecParameters *par) {
    uint8_t *bytes = NULL;
    size_t size = 0;
    int64_t position = 0;
    int ok = 0;
    RemuxBox box = {0};
    if (!remux_moov(input->io.descriptor.fd, input->io.descriptor.base,
                    input->io.descriptor.length, &bytes, &size, &position)) goto done;
    size_t start = 0, end = 0;
    for (size_t at = 8; at < size;) {
        RemuxBox track, media, handler;
        if (!remux_box(bytes, size, at, &track)) goto done;
        if (AV_RB32(bytes + at + 4) == MKBETAG('t','r','a','k')) {
            if (!remux_child(bytes, at + 8, at + track.size, MKBETAG('m','d','i','a'), &media) ||
                !remux_child(bytes, media.at + 8, media.at + media.size, MKBETAG('h','d','l','r'), &handler)) goto done;
            if (handler.size < 20) { fail("invalid_media", "truncated VP9 track handler"); goto done; }
            if (AV_RB32(bytes + handler.at + 16) == MKBETAG('v','i','d','e')) {
                if (start) { fail("invalid_media", "multiple VP9 picture tracks"); goto done; }
                start = at + 8; end = at + track.size;
            }
        }
        at += track.size;
    }
    if (!start) { fail("invalid_media", "missing VP9 picture track"); goto done; }
    const uint32_t path[] = {MKBETAG('m','d','i','a'),
        MKBETAG('m','i','n','f'), MKBETAG('s','t','b','l'), MKBETAG('s','t','s','d')};
    for (size_t index = 0; index < sizeof(path) / sizeof(path[0]); index++) {
        if (!remux_child(bytes, start, end, path[index], &box)) goto done;
        start = box.at + 8; end = box.at + box.size;
    }
    if (end - start < 8 || AV_RB32(bytes + start) || AV_RB32(bytes + start + 4) != 1 ||
        !remux_box(bytes, end, start + 8, &box) || box.at + box.size != end ||
        box.size < 86 || AV_RB32(bytes + box.at + 4) != MKBETAG('v','p','0','9')) {
        fail("invalid_media", "VP9 assembly requires one visual sample entry"); goto done;
    }
    if (!remux_child(bytes, box.at + 86, box.at + box.size, MKBETAG('v','p','c','C'), &box)) goto done;
    const uint8_t *config = bytes + box.at + 8;
    if (box.size != 20 || AV_RB32(config) != 0x01000000 || AV_RB16(config + 10) ||
        (config[4] != 0 && config[4] != 2) || ((config[6] >> 1) & 7) > 1 ||
        (config[6] >> 4) != (config[4] ? 10 : 8)) {
        fail("invalid_media", "unqualified VP9 assembly configuration"); goto done;
    }
    if ((par->color_range != AVCOL_RANGE_UNSPECIFIED && par->color_range != (config[6] & 1 ? AVCOL_RANGE_JPEG : AVCOL_RANGE_MPEG)) ||
        (par->color_primaries != AVCOL_PRI_UNSPECIFIED && par->color_primaries != config[7]) ||
        (par->color_trc != AVCOL_TRC_UNSPECIFIED && par->color_trc != config[8]) ||
        (par->color_space != AVCOL_SPC_UNSPECIFIED && par->color_space != config[9])) {
        fail("invalid_media", "VP9 container color declarations conflict"); goto done;
    }
    par->profile = config[4]; par->level = config[5];
    par->format = config[4] ? AV_PIX_FMT_YUV420P10LE : AV_PIX_FMT_YUV420P;
    par->chroma_location = ((config[6] >> 1) & 7) ? AVCHROMA_LOC_TOPLEFT : AVCHROMA_LOC_LEFT;
    par->color_range = config[6] & 1 ? AVCOL_RANGE_JPEG : AVCOL_RANGE_MPEG;
    par->color_primaries = config[7]; par->color_trc = config[8]; par->color_space = config[9];
    ok = 1;
done:
    av_free(bytes);
    return ok;
}

static int remux_prepare_metadata(RemuxInput inputs[2], const DeadpanRemuxRequest *request) {
    AVCodecParameters *video = inputs[0].format->streams[0]->codecpar;
    AVCodecParameters *audio = inputs[1].format->streams[0]->codecpar;
    inputs[1].admission = request;
    inputs[0].nominal_frame_duration_ns = request->nominal_frame_duration_ns;
    inputs[0].chroma_assumed_left = request->chroma_assumed_left;
    if (!inputs[0].matroska) {
        if (!remux_read_original(inputs[0].io.descriptor.fd, 0, inputs[0].io.descriptor.length,
                                &inputs[0].nominal_frame_duration_ns, &inputs[0].chroma_assumed_left)) return 0;
        if (inputs[0].nominal_frame_duration_ns)
            inputs[0].format->flags |= AVFMT_FLAG_NOPARSE | AVFMT_FLAG_NOFILLIN;
    }
    if (inputs[0].nominal_frame_duration_ns > 1000000000)
        return fail("invalid_media", "Original cadence declaration exceeds one second");
    if (video->codec_id == AV_CODEC_ID_VP9) {
        if (inputs[0].matroska) {
            if (!request->vp9_profile_plus_one) return fail("invalid_media", "VP9 WebM lacks admitted metadata");
            video->profile = (int)request->vp9_profile_plus_one - 1;
            video->format = request->vp9_bit_depth == 10 ? AV_PIX_FMT_YUV420P10LE : AV_PIX_FMT_YUV420P;
            video->chroma_location = request->vp9_chroma ? AVCHROMA_LOC_TOPLEFT : AVCHROMA_LOC_LEFT;
            video->color_primaries = request->vp9_primaries; video->color_trc = request->vp9_transfer;
            video->color_space = request->vp9_matrix;
            video->color_range = request->vp9_full_range ? AVCOL_RANGE_JPEG : AVCOL_RANGE_MPEG;
        } else if (!remux_vp9_mp4(&inputs[0], video)) return 0;
    }
    if (inputs[0].chroma_assumed_left &&
        (video->codec_id != AV_CODEC_ID_VP9 || video->chroma_location != AVCHROMA_LOC_LEFT))
        return fail("invalid_media", "Original chroma interpretation requires left-sited VP9");
    if (video->codec_id == AV_CODEC_ID_AV1) {
        if (video->extradata_size < 4 || video->extradata[0] != 0x81 || video->extradata[1] >> 5 ||
            (video->extradata[2] & 0x30) || (video->extradata[2] & 0x0c) != 0x0c)
            return fail("invalid_media", "AV1 assembly requires Main eight/ten-bit 4:2:0");
        video->format = video->extradata[2] & 0x40 ? AV_PIX_FMT_YUV420P10LE : AV_PIX_FMT_YUV420P;
    }
    if (audio->codec_id == AV_CODEC_ID_OPUS) {
        if (audio->extradata_size != 19 || memcmp(audio->extradata, "OpusHead", 8) ||
            audio->extradata[8] != 1 || audio->extradata[18] || audio->sample_rate != 48000 ||
            audio->ch_layout.nb_channels < 1 || audio->ch_layout.nb_channels > 2 ||
            audio->extradata[9] != audio->ch_layout.nb_channels ||
            AV_RL16(audio->extradata + 10) != audio->initial_padding || audio->seek_preroll != 3840 ||
            (inputs[1].matroska && (!request->opus_present || request->opus_pre_skip != (uint32_t)audio->initial_padding)))
            return fail("invalid_media", "unqualified Opus assembly header");
        inputs[1].opus = 1;
    } else if (inputs[1].matroska || request->opus_present) {
        return fail("invalid_media", "Matroska assembly audio must be admitted Opus");
    }
    return within_deadline();
}

#include "remux_edit.h"
