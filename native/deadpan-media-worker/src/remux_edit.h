/* Preserve an Opus track's positive presentation offset without exposing its
   priming packets. FFmpeg's muxer only expresses pre-skip when DTS starts below
   zero. It writes this private movie at zero, then this bounded edit-list
   rewrite restores the admitted offset before independent re-demuxing.
   moov must be last, so no media/chunk offset changes. */

typedef struct {
    uint64_t delay, duration, media_time, movie_duration;
    size_t audio_track;
} RemuxEdit;

/* MP4 has no Matroska DefaultDuration field. Retain that declaration in a
   fixed UUID extension; readers still prove the proposed grid against every
   measured picture. This changes no sample, packet, timestamp or chunk. */
static int remux_append_original(int fd, int64_t *length, uint64_t maximum, const uint8_t uuid[16], uint64_t value) {
    uint8_t *bytes = NULL;
    size_t size = 0;
    int64_t position = 0;
    int ok = 0;
    if (!remux_moov(fd, 0, *length, &bytes, &size, &position)) goto done;
    if (position + (int64_t)size != *length || !value || value > 1000000000) {
        fail("invalid_media", "Original interpretation requires a terminal movie box"); goto done;
    }
    if ((uint64_t)*length > maximum || maximum - (uint64_t)*length < 32) {
        fail("output_too_large", "Original cadence exceeds output bound"); goto done;
    }
    uint8_t box[32], header[4];
    AV_WB32(box, sizeof(box)); AV_WB32(box + 4, MKBETAG('u','u','i','d'));
    memcpy(box + 8, uuid, 16); AV_WB64(box + 24, value);
    AV_WB32(header, size + sizeof(box));
    if (!within_deadline()) goto done;
    if (pwrite(fd, box, sizeof(box), *length) != sizeof(box) ||
        pwrite(fd, header, sizeof(header), position) != sizeof(header) || fsync(fd)) {
        fail(io_code(errno), "write Original cadence metadata"); goto done;
    }
    *length += sizeof(box);
    ok = 1;
done:
    av_free(bytes);
    return ok;
}

static int remux_duration(const uint8_t *body, size_t length, int track, uint64_t *duration) {
    if (!length) return fail("invalid_media", "empty MP4 duration header");
    size_t at = body[0] == 1 ? (track ? 28 : 24) : (track ? 20 : 16);
    size_t width = body[0] == 1 ? 8 : 4;
    if (body[0] > 1 || length < at + width)
        return fail("invalid_media", "unsupported MP4 duration header");
    *duration = width == 8 ? AV_RB64(body + at) : AV_RB32(body + at);
    return 1;
}

static int remux_write_duration(uint8_t *out, size_t *used, const uint8_t *box, size_t size,
                                 int track, uint64_t duration) {
    const uint8_t *body = box + 8;
    size_t length = size - 8;
    uint64_t old;
    if (!remux_duration(body, length, track, &old)) return 0;
    uint8_t *target = out + *used;
    if (body[0] == 1 || duration <= UINT32_MAX) {
        memcpy(target, box, size);
        size_t at = body[0] == 1 ? (track ? 28 : 24) : (track ? 20 : 16);
        if (body[0] == 1) AV_WB64(target + 8 + at, duration);
        else AV_WB32(target + 8 + at, duration);
        *used += size;
        return 1;
    }
    /* Upgrade only the fixed time header; every later field stays identical. */
    size_t before_duration = track ? 8 : 4;
    size_t after_duration = track ? 24 : 20;
    memcpy(target, box, 12);
    AV_WB32(target, size + 12);
    target[8] = 1;
    AV_WB64(target + 12, AV_RB32(body + 4));
    AV_WB64(target + 20, AV_RB32(body + 8));
    memcpy(target + 28, body + 12, before_duration);
    AV_WB64(target + 28 + before_duration, duration);
    memcpy(target + 36 + before_duration, body + after_duration, length - after_duration);
    *used += size + 12;
    return 1;
}

static int remux_rewrite_movie(const uint8_t *bytes, size_t start, size_t end,
                              uint8_t *output, size_t capacity, size_t *used,
                              const RemuxEdit *edit, int audio, unsigned depth) {
    if (depth > 2) return fail("invalid_media", "unexpected MP4 edit nesting");
    for (size_t at = start; at < end;) {
        RemuxBox box;
        if (!remux_box(bytes, end, at, &box)) return 0;
        if (*used > capacity || capacity - *used < box.size + 128)
            return fail("resource_exhausted", "MP4 edit rewrite exceeds its allocation");
        uint32_t tag = AV_RB32(bytes + at + 4);
        if (tag == MKBETAG('t','r','a','k')) {
            size_t begin = *used;
            memcpy(output + *used, bytes + at, 8); *used += 8;
            if (!remux_rewrite_movie(bytes, at + 8, at + box.size, output, capacity, used,
                                     edit, at == edit->audio_track, depth + 1)) return 0;
            AV_WB32(output + begin, *used - begin);
        } else if (tag == MKBETAG('m','v','h','d')) {
            if (!remux_write_duration(output, used, bytes + at, box.size, 0, edit->movie_duration)) return 0;
        } else if (audio && tag == MKBETAG('t','k','h','d')) {
            if (!remux_write_duration(output, used, bytes + at, box.size, 1, edit->duration)) return 0;
        } else if (audio && tag == MKBETAG('e','d','t','s')) {
            uint8_t *target = output + *used;
            AV_WB32(target, 64); AV_WB32(target + 4, MKBETAG('e','d','t','s'));
            AV_WB32(target + 8, 56); AV_WB32(target + 12, MKBETAG('e','l','s','t'));
            AV_WB32(target + 16, 0x01000000); AV_WB32(target + 20, 2);
            AV_WB64(target + 24, edit->delay); AV_WB64(target + 32, UINT64_MAX); AV_WB32(target + 40, 0x10000);
            AV_WB64(target + 44, edit->duration - edit->delay); AV_WB64(target + 52, edit->media_time);
            AV_WB32(target + 60, 0x10000);
            *used += 64;
        } else {
            memcpy(output + *used, bytes + at, box.size); *used += box.size;
        }
        at += box.size;
    }
    return within_deadline();
}

static int remux_offset_edit(int fd, int64_t *length, uint64_t maximum, int64_t offset) {
    uint8_t *bytes = NULL, *rewritten = NULL;
    size_t size = 0;
    int64_t position = 0;
    int ok = 0;
    RemuxBox movie_header, audio = {0}, mdia, handler, edits, list;
    RemuxEdit edit = {0};
    if (!remux_moov(fd, 0, *length, &bytes, &size, &position)) goto done;
    if (position + (int64_t)size != *length ||
        !remux_child(bytes, 8, size, MKBETAG('m','v','h','d'), &movie_header)) {
        fail("invalid_media", "Opus offset rewrite requires a terminal movie box"); goto done;
    }
    const uint8_t *mvhd = bytes + movie_header.at + 8;
    if (movie_header.size < 40 || mvhd[0] > 1) {
        fail("invalid_media", "invalid movie clock for Opus offset"); goto done;
    }
    uint32_t timescale = AV_RB32(mvhd + (mvhd[0] == 1 ? 20 : 12));
    int64_t delay;
    if (!timescale || timescale > INT_MAX || offset <= 0) {
        fail("invalid_media", "invalid Opus presentation offset clock"); goto done;
    }
    if (!remux_exact_time(offset, (AVRational){1,48000},
        (AVRational){1,(int)timescale}, &delay)) goto done;
    edit.delay = (uint64_t)delay;
    for (size_t at = 8; at < size;) {
        RemuxBox box;
        if (!remux_box(bytes, size, at, &box)) goto done;
        if (AV_RB32(bytes + at + 4) == MKBETAG('t','r','a','k')) {
            if (!remux_child(bytes, at + 8, at + box.size, MKBETAG('m','d','i','a'), &mdia) ||
                !remux_child(bytes, mdia.at + 8, mdia.at + mdia.size, MKBETAG('h','d','l','r'), &handler)) goto done;
            if (handler.size < 20) { fail("invalid_media", "truncated assembly handler"); goto done; }
            if (AV_RB32(bytes + handler.at + 16) == MKBETAG('s','o','u','n')) {
                if (audio.size) { fail("invalid_media", "multiple assembly audio tracks"); goto done; }
                audio = box;
            }
        }
        at += box.size;
    }
    if (!audio.size) { fail("invalid_media", "missing assembly audio track"); goto done; }
    if (!remux_child(bytes, audio.at + 8, audio.at + audio.size, MKBETAG('e','d','t','s'), &edits) ||
        !remux_child(bytes, edits.at + 8, edits.at + edits.size, MKBETAG('e','l','s','t'), &list)) goto done;
    const uint8_t *body = bytes + list.at + 8;
    if (list.size < 28 || body[0] > 1 || (AV_RB32(body) & 0xffffff) || AV_RB32(body + 4) != 1 ||
        list.size != (body[0] == 1 ? 36 : 28)) {
        fail("invalid_media", "Opus offset rewrite requires one exact presentation edit"); goto done;
    }
    uint64_t duration = body[0] == 1 ? AV_RB64(body + 8) : AV_RB32(body + 8);
    edit.media_time = body[0] == 1 ? AV_RB64(body + 16) : AV_RB32(body + 12);
    if (edit.media_time > UINT16_MAX || AV_RB32(body + (body[0] == 1 ? 24 : 16)) != 0x10000 ||
        !checked_add_u64(duration, edit.delay, &edit.duration)) {
        fail("invalid_media", "invalid Opus presentation edit or duration overflow"); goto done;
    }
    if (!remux_duration(mvhd, movie_header.size - 8, 0, &edit.movie_duration)) goto done;
    if (edit.duration > edit.movie_duration) edit.movie_duration = edit.duration;
    edit.audio_track = audio.at;
    rewritten = av_malloc(size + 256);
    if (!rewritten) { fail("resource_exhausted", "allocate Opus edit rewrite"); goto done; }
    memcpy(rewritten, bytes, 8);
    size_t used = 8;
    if (!remux_rewrite_movie(bytes, 8, size, rewritten, size + 256, &used, &edit, 0, 0)) goto done;
    if ((uint64_t)position + used > maximum) { fail("output_too_large", "Opus edit exceeds output bound"); goto done; }
    AV_WB32(rewritten, used);
    size_t written_bytes = 0;
    while (written_bytes < used) {
        if (!within_deadline()) goto done;
        ssize_t written = pwrite(fd, rewritten + written_bytes, used - written_bytes, position + (int64_t)written_bytes);
        if (written < 0 && errno == EINTR) continue;
        if (written <= 0) { fail(io_code(errno), "write Opus presentation edit"); goto done; }
        written_bytes += (size_t)written;
    }
    *length = position + (int64_t)used;
    if (ftruncate(fd, *length) || fsync(fd)) { fail(io_code(errno), "synchronize Opus presentation edit"); goto done; }
    ok = 1;
done:
    av_free(bytes); av_free(rewritten);
    return ok;
}
