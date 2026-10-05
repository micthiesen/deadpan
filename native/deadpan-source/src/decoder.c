#define _POSIX_C_SOURCE 200809L
#ifdef __APPLE__
#define _DARWIN_C_SOURCE
#endif
#include "decoder.h"
#include <errno.h>
#include <inttypes.h>
#include <limits.h>
#include <stdatomic.h>
#include <stdarg.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>
#ifdef __APPLE__
#include <pthread/qos.h>
#endif
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
    int pending_first_frame, h264_length_bytes;
    int fresh_keyframe, fresh_key_packet_pending;
    int64_t fresh_key_pts;
    // Packets whose PTS precedes this target may skip non-reference pictures,
    // only while every admitted SPS makes that safe (see sps_allows_skip).
    int skip_nonref, skip_safe;
    _Atomic uint64_t pictures;
    int64_t skip_before_pts;
    // Set by codec callbacks, which may run on FFmpeg frame threads outside
    // any exported call. Only this atomic code crosses that thread boundary.
    _Atomic int async_failure;
    unsigned int stream_count;
    uint64_t frames, packets, io_bytes, deadline;
    DeadpanDecodeWork work;
    DeadpanCancelled cancelled;
    const void *cancel_opaque;
    DeadpanSourceError *error;
};
enum { ASYNC_NONE, ASYNC_GEOMETRY, ASYNC_DEPTH, ASYNC_FORMAT };
static void adopt_async(DeadpanSource *s) {
    // A codec callback failure is the cause of whatever FFmpeg reports next.
    if (!s->error || s->error->code[0]) return;
    const char *code = NULL, *message = NULL;
    switch (atomic_load(&s->async_failure)) {
        case ASYNC_GEOMETRY: code = "resource_limit"; message = "decoder geometry exceeds max_dimension or max_pixels"; break;
        case ASYNC_DEPTH: code = "unsupported_depth"; message = "only eight-bit SDR decode is qualified"; break;
        case ASYNC_FORMAT: code = "unsupported_pixel_format"; message = "no bounded software picture format"; break;
        default: return;
    }
    (void)snprintf(s->error->code, sizeof(s->error->code), "%s", code);
    (void)snprintf(s->error->message, sizeof(s->error->message), "%s", message);
}
static void async_fail(DeadpanSource *s, int code) {
    int none = ASYNC_NONE;
    (void)atomic_compare_exchange_strong(&s->async_failure, &none, code);
}
static int fail(DeadpanSource *s, const char *code, const char *format, ...) {
    adopt_async(s);
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
    if (atomic_load(&s->async_failure) != ASYNC_NONE) return fail(s, "internal_error", "codec callback failed");
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
    if ((uint64_t)count > UINT64_MAX - s->work.io_bytes) {
        fail(s, "resource_limit", "cumulative input byte count overflow"); return AVERROR(EFBIG);
    }
    s->work.io_bytes += (uint64_t)count;
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
static int geometry(DeadpanSource *s, int width, int height, const char *stage) {
    if (width <= 0 || height <= 0 || (unsigned)width > s->limits.max_dimension || (unsigned)height > s->limits.max_dimension ||
        (uint64_t)width * (uint64_t)height > s->limits.max_pixels)
        return fail(s, "resource_limit",
                    "%s geometry %dx%d exceeds max_dimension=%u or max_pixels=%" PRIu64,
                    stage, width, height, s->limits.max_dimension, s->limits.max_pixels);
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
            if (geometry(s, p->width, p->height, "container video stream") < 0) return -1;
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
static int geometry_ok(const DeadpanSourceLimits *limits, int width, int height) {
    return width > 0 && height > 0 && (unsigned)width <= limits->max_dimension && (unsigned)height <= limits->max_dimension &&
        (uint64_t)width * (uint64_t)height <= limits->max_pixels;
}
// These callbacks run on the controlled decoder. In pinned FFmpeg 8.0.3,
// h264_init_ps calls get_format after installing SPS coded dimensions and before
// h264_slice_header_init allocates macroblock tables. Frame-buffer max_pixels
// alone is later than that allocation. No hidden probing decoder may bypass us.
// With frame threading both callbacks may run on a codec thread after the
// exported call has returned, so they read only immutable limits and report
// through the atomic code. Deadline and cancellation stay on the caller.
static enum AVPixelFormat bounded_format(AVCodecContext *context, const enum AVPixelFormat *formats) {
    DeadpanSource *s = context->opaque;
    if (atomic_load(&s->async_failure) != ASYNC_NONE) return AV_PIX_FMT_NONE;
    if (!geometry_ok(&s->limits, context->width, context->height) ||
        !geometry_ok(&s->limits, context->coded_width, context->coded_height)) {
        async_fail(s, ASYNC_GEOMETRY);
        return AV_PIX_FMT_NONE;
    }
    for (unsigned int i = 0; i < 64 && formats[i] != AV_PIX_FMT_NONE; i++) {
        const AVPixFmtDescriptor *pixel = av_pix_fmt_desc_get(formats[i]);
        if (!pixel || (pixel->flags & AV_PIX_FMT_FLAG_HWACCEL)) continue;
        for (unsigned int component = 0; component < pixel->nb_components; component++) {
            if (pixel->comp[component].depth != 8) {
                async_fail(s, ASYNC_DEPTH);
                return AV_PIX_FMT_NONE;
            }
        }
        return formats[i];
    }
    async_fail(s, ASYNC_FORMAT);
    return AV_PIX_FMT_NONE;
}
static int bounded_buffer(AVCodecContext *context, AVFrame *frame, int flags) {
    DeadpanSource *s = context->opaque;
    if (atomic_load(&s->async_failure) != ASYNC_NONE) return AVERROR(EINVAL);
    if (!geometry_ok(&s->limits, frame->width, frame->height)) {
        async_fail(s, ASYNC_GEOMETRY);
        return AVERROR(EINVAL);
    }
    atomic_fetch_add(&s->pictures, 1);
    return avcodec_default_get_buffer2(context, frame, flags);
}
// Bounded big-endian bit reader over an SPS RBSP (emulation bytes removed).
typedef struct { const uint8_t *data; size_t bits, position; int failed; } Bits;
static uint32_t bit(Bits *b) {
    if (b->position >= b->bits) { b->failed = 1; return 0; }
    uint32_t value = (b->data[b->position >> 3] >> (7 - (b->position & 7))) & 1;
    b->position++;
    return value;
}
static uint32_t bits(Bits *b, int count) {
    uint32_t value = 0;
    for (int i = 0; i < count; i++) value = (value << 1) | bit(b);
    return value;
}
static uint32_t ue(Bits *b) {
    int zeros = 0;
    while (!b->failed && !bit(b)) if (++zeros > 31) { b->failed = 1; return 0; }
    if (b->failed) return 0;
    return (uint32_t)(((uint64_t)1 << zeros) - 1 + bits(b, zeros));
}
static void se(Bits *b) { (void)ue(b); }
static void hrd(Bits *b) {
    uint32_t count = ue(b) + 1;
    if (count > 32) { b->failed = 1; return; }
    (void)bits(b, 8);
    for (uint32_t i = 0; i < count && !b->failed; i++) { (void)ue(b); (void)ue(b); (void)bit(b); }
    (void)bits(b, 20);
}
// Skipping non-reference preroll is safe only when the decoder's reorder
// depth is declared rather than estimated from the POCs it happens to see
// (FFmpeg raises an estimated depth only without bitstream_restriction), and
// when every picture is a frame (no PAFF/MBAFF field pairing). Returns 1 only
// for a complete SPS proving both; anything unparsed or unusual returns 0.
static int sps_allows_skip(const uint8_t *nal, size_t length) {
    if (length < 4 || length > 4096 || (nal[0] & 31) != 7) return 0;
    uint8_t rbsp[4096];
    size_t size = 0, zeros = 0;
    for (size_t i = 1; i < length; i++) {
        if (zeros >= 2 && nal[i] == 3) { zeros = 0; continue; }
        zeros = nal[i] ? 0 : zeros + 1;
        rbsp[size++] = nal[i];
    }
    Bits b = {rbsp, size * 8, 0, 0};
    uint32_t profile = bits(&b, 8);
    (void)bits(&b, 16);
    (void)ue(&b);
    uint32_t chroma = 1;
    if (profile == 100 || profile == 110 || profile == 122 || profile == 244 || profile == 44 || profile == 83 ||
        profile == 86 || profile == 118 || profile == 128 || profile == 138 || profile == 139 || profile == 134 || profile == 135) {
        chroma = ue(&b);
        if (chroma == 3) (void)bit(&b);
        (void)ue(&b); (void)ue(&b); (void)bit(&b);
        if (bit(&b)) {
            for (int i = 0; i < (chroma != 3 ? 8 : 12) && !b.failed; i++) {
                if (!bit(&b)) continue;
                int last = 8, next = 8;
                for (int j = 0; j < (i < 6 ? 16 : 64) && !b.failed; j++) {
                    if (next) {
                        uint32_t code = ue(&b);
                        int32_t delta = code & 1 ? (int32_t)((code + 1) / 2) : -(int32_t)(code / 2);
                        next = (last + delta + 256) % 256;
                    }
                    last = next ? next : last;
                }
            }
        }
    }
    (void)ue(&b);
    uint32_t poc = ue(&b);
    if (poc == 0) (void)ue(&b);
    else if (poc == 1) {
        (void)bit(&b); se(&b); se(&b);
        uint32_t cycle = ue(&b);
        if (cycle > 255) return 0;
        for (uint32_t i = 0; i < cycle && !b.failed; i++) se(&b);
    } else if (poc != 2) return 0;
    (void)ue(&b); (void)bit(&b); (void)ue(&b); (void)ue(&b);
    if (!bit(&b)) return 0;  // frame_mbs_only_flag
    (void)bit(&b);
    if (bit(&b)) { (void)ue(&b); (void)ue(&b); (void)ue(&b); (void)ue(&b); }
    if (!bit(&b)) return 0;  // vui_parameters_present_flag
    if (bit(&b) && bits(&b, 8) == 255) (void)bits(&b, 32);
    if (bit(&b)) (void)bit(&b);
    if (bit(&b)) { (void)bits(&b, 4); if (bit(&b)) (void)bits(&b, 24); }
    if (bit(&b)) { (void)ue(&b); (void)ue(&b); }
    if (bit(&b)) (void)bits(&b, 32), (void)bits(&b, 32), (void)bit(&b);
    int nal_hrd = (int)bit(&b);
    if (nal_hrd) hrd(&b);
    int vcl_hrd = (int)bit(&b);
    if (vcl_hrd) hrd(&b);
    if (nal_hrd || vcl_hrd) (void)bit(&b);
    (void)bit(&b);
    int restricted = (int)bit(&b);  // bitstream_restriction_flag
    if (restricted) {
        (void)bit(&b);
        for (int i = 0; i < 6; i++) (void)ue(&b);
    }
    return restricted && !b.failed;
}
// Every SPS in the admitted AVC configuration must allow skipping.
static int avcc_allows_skip(const uint8_t *data, int size) {
    if (size < 7) return 0;
    int count = data[5] & 31, position = 6;
    if (!count) return 0;
    for (int i = 0; i < count; i++) {
        if (position + 2 > size) return 0;
        int length = (data[position] << 8) | data[position + 1];
        position += 2;
        if (length <= 0 || position + length > size || !sps_allows_skip(data + position, (size_t)length)) return 0;
        position += length;
    }
    return 1;
}
static int receive_frame(DeadpanSource *s);
static int check_frame(DeadpanSource *s);
static int allocate_decoder(DeadpanSource *s) {
    // Any previous codec and its threads are gone; a failure they reported
    // poisoned that session, and a fresh codec starts clean.
    atomic_store(&s->async_failure, ASYNC_NONE);
    AVStream *stream = s->format->streams[s->stream];
    const AVCodec *codec = avcodec_find_decoder(stream->codecpar->codec_id);
    if (!codec) return fail(s, "unsupported_codec", "required source software decoder is unavailable");
    s->decoder = avcodec_alloc_context3(codec);
    if (!s->decoder) return fail(s, "resource_exhausted", "allocate source decode context");
    int result = avcodec_parameters_to_context(s->decoder, stream->codecpar);
    if (result < 0) return fferror(s, "copy source codec parameters", result);
    // FFmpeg's frame and slice threading are deterministic: pictures match a
    // single-threaded decode bit for bit. Threads join in avcodec_free_context.
    s->decoder->thread_count = (int)s->limits.threads;
    s->decoder->thread_type = s->limits.threads > 1 ? FF_THREAD_FRAME | FF_THREAD_SLICE : 0;
    s->decoder->opaque = s;
    s->decoder->get_format = bounded_format;
    s->decoder->get_buffer2 = bounded_buffer;
    int64_t max_pixels = (int64_t)s->limits.max_dimension * s->limits.max_dimension;
    if ((uint64_t)max_pixels > s->limits.max_pixels) max_pixels = (int64_t)s->limits.max_pixels;
    s->decoder->max_pixels = max_pixels;
    s->decoder->err_recognition = AV_EF_EXPLODE | AV_EF_CAREFUL;
    s->decoder->pkt_timebase = stream->time_base;
    // Keep the coded crop available for the shared visible-rectangle check.
    s->decoder->apply_cropping = 0;
    if ((result = avcodec_open2(s->decoder, codec, NULL)) < 0)
        return fferror(s, "open source decoder", result);
    return check(s);
}
static int seek_fresh_keyframe(DeadpanSource *s, int64_t pts) {
    if (pts == AV_NOPTS_VALUE) return fail(s, "invalid_timestamp", "seek timestamp is reserved for unknown PTS");
    if (!s->h264_length_bytes) return fail(s, "unsupported_codec", "fresh export GOP checks require H264 AVC");
    int result = av_seek_frame(s->format, s->stream, pts, AVSEEK_FLAG_BACKWARD);
    if (result < 0) return fferror(s, "seek fresh source GOP", result);
    s->fresh_key_pts = pts;
    s->fresh_keyframe = 1;
    s->fresh_key_packet_pending = 1;
    s->pending_first_frame = 0;
    s->draining = 0; s->ended = 0; s->frames = 0; s->packets = 0;
    return check(s);
}
static int check_fresh_keyframe(DeadpanSource *s) {
    if (s->fresh_key_packet_pending || s->frame->pts != s->fresh_key_pts ||
        !(s->frame->flags & AV_FRAME_FLAG_KEY) || s->frame->pict_type != AV_PICTURE_TYPE_I)
        return fail(s, "invalid_keyframe", "fresh GOP must begin at the exact requested key I picture");
    s->fresh_keyframe = 0;
    return 1;
}
static int open_impl(DeadpanSource *s) {
    if (runtime(s) < 0) return -1;
    if (!s->limits.max_input_bytes || s->limits.max_input_bytes > 64ULL*1024*1024*1024 ||
        !s->limits.max_frames || s->limits.max_frames > 10000000 || !s->limits.max_packets || s->limits.max_packets > 40000000 ||
        !s->limits.max_io_bytes_per_call || s->limits.max_io_bytes_per_call > 1024ULL*1024*1024 ||
        !s->limits.max_packet_bytes || s->limits.max_packet_bytes > 16ULL*1024*1024 ||
        !s->limits.max_pixels || s->limits.max_pixels > 8192ULL*8192 ||
        !s->limits.max_dimension || s->limits.max_dimension > 8192 || !s->limits.max_packets_per_frame || s->limits.max_packets_per_frame > 10000 ||
        !s->limits.threads || s->limits.threads > 16)
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
    // Container admission has already bounded every declared packet/table. Do
    // not let libavformat invoke an opaque parser or probing decoder before our
    // per-packet codec checks and allocation callbacks.
    s->format->flags |= AVFMT_FLAG_CUSTOM_IO | AVFMT_FLAG_NOPARSE | AVFMT_FLAG_NOFILLIN;
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
    if (check(s) < 0 || table(s, 0) < 0) return -1;
    AVStream *stream = s->format->streams[s->stream];
    AVCodecParameters *p = stream->codecpar;
    if (stream->time_base.num <= 0 || stream->time_base.den <= 0) return fail(s, "invalid_time_base", "source stream has no positive time base");
    if (p->field_order != AV_FIELD_UNKNOWN && p->field_order != AV_FIELD_PROGRESSIVE) return fail(s, "unsupported_interlace", "interlaced source requires a qualified deinterlacer");
    const AVCodec *codec = avcodec_find_decoder(p->codec_id);
    if (!codec) return fail(s, "unsupported_codec", "required source software decoder is unavailable");
    if (p->extradata_size <= 0 || p->extradata_size > 65536)
        return fail(s, "resource_limit", "source codec configuration is absent or exceeds its bound");
    if (p->codec_id == AV_CODEC_ID_H264) {
        if (p->extradata_size < 7 || p->extradata[0] != 1)
            return fail(s, "unsupported_codec", "H264 source requires admitted AVC configuration");
        s->h264_length_bytes = (p->extradata[4] & 3) + 1;
        s->skip_safe = avcc_allows_skip(p->extradata, p->extradata_size);
        if (s->h264_length_bytes == 3) return fail(s, "unsupported_codec", "unsupported AVC NAL length field");
    }
    s->packet = av_packet_alloc(); s->frame = av_frame_alloc();
    if (!s->packet || !s->frame) return fail(s, "resource_exhausted", "allocate source decode buffers");
    if (allocate_decoder(s) < 0) return -1;
    // In fresh mode no packet has ever reached this new codec. Seek before the
    // ordinary first-picture observation, which must not warm a GOP decoder.
    if (s->fresh_keyframe && seek_fresh_keyframe(s, s->fresh_key_pts) < 0) return -1;
    // Decode once under the same opening deadline/input allowance and retain
    // this frame for the first caller. Probe work cannot create a second decoder,
    // skip initial content, or bypass the configured frame/packet counters.
    result = receive_frame(s);
    if (result < 0) return -1;
    if (!result) return fail(s, "invalid_stream", "source contains no decoded picture");
    if (s->fresh_keyframe && check_fresh_keyframe(s) < 0) return -1;
    AVFrame *f = s->frame;
    if (color(s, f->format, f->color_range, f->colorspace, f->color_trc, f->color_primaries) < 0) return -1;
    const AVPixFmtDescriptor *pixel = av_pix_fmt_desc_get(f->format);
    if (!(pixel->flags & AV_PIX_FMT_FLAG_RGB) && (pixel->log2_chroma_w || pixel->log2_chroma_h)) {
        int x, y;
        if (av_chroma_location_enum_to_pos(&x, &y, f->chroma_location) < 0)
            return fail(s, "missing_interpretation", "subsampled YUV needs an explicit chroma location");
    }
    s->chroma_location = f->chroma_location;
    AVRational declared_aspect = stream->sample_aspect_ratio.num ? stream->sample_aspect_ratio : p->sample_aspect_ratio;
    AVRational aspect = declared_aspect.num ? declared_aspect : sar(f->sample_aspect_ratio);
    if (aspect.num <= 0 || aspect.den <= 0 || aspect.num > 1000000 || aspect.den > 1000000)
        return fail(s, "unsupported_aspect", "invalid or excessive sample aspect ratio");
    s->info = (DeadpanSourceInfo){.width=p->width,.height=p->height,.stream_index=stream->index,.time_base_num=stream->time_base.num,.time_base_den=stream->time_base.den,.sar_num=aspect.num,.sar_den=aspect.den,.range=f->color_range,.matrix=f->colorspace,.transfer=f->color_trc,.primaries=f->color_primaries,.stream_start=stream->start_time,.stream_duration=stream->duration,.container_start=s->format->start_time,.container_duration=s->format->duration};
    if (capture_audio_inventory(s) < 0 || rotation(s, p->coded_side_data, p->nb_coded_side_data, &s->info.rotation) < 0) return -1;
    s->pixel_format = f->format;
    (void)snprintf(s->info.codec, sizeof(s->info.codec), "%s", codec->name);
    (void)snprintf(s->info.pixel_format, sizeof(s->info.pixel_format), "%s", av_get_pix_fmt_name(f->format));
    if (check_frame(s) < 0) return -1;
    s->pending_first_frame = 1;
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
static int open_source(int fd, int64_t length, const DeadpanSourceLimits *limits, uint64_t preflight_io_bytes, uint64_t timeout,
                        DeadpanCancelled cancelled, const void *opaque, DeadpanSource **out,
                        DeadpanSourceInfo *info, DeadpanSourceError *error, int fresh, int64_t pts) {
    *out = NULL; memset(error, 0, sizeof(*error));
    DeadpanSource *s = av_mallocz(sizeof(*s));
    if (!s) { (void)snprintf(error->code, sizeof(error->code), "resource_exhausted"); (void)snprintf(error->message, sizeof(error->message), "allocate source session"); return -1; }
    s->fd = fd; s->length = length; s->limits = *limits;
    s->fresh_keyframe = fresh; s->fresh_key_pts = pts;
    int result = begin(s, timeout, cancelled, opaque, error);
    if (result > 0 && preflight_io_bytes > s->limits.max_io_bytes_per_call)
        result = fail(s, "resource_limit", "container admission exhausted the opening input budget");
    s->io_bytes = preflight_io_bytes;
    s->work.io_bytes = preflight_io_bytes;
    if (result < 0 || open_impl(s) < 0) { deadpan_source_close(s); return -1; }
    *info = s->info; *out = s;
    return finish(s, 1);
}
int deadpan_source_open(int fd, int64_t length, const DeadpanSourceLimits *limits, uint64_t preflight_io_bytes, uint64_t timeout,
                        DeadpanCancelled cancelled, const void *opaque, DeadpanSource **out,
                        DeadpanSourceInfo *info, DeadpanSourceError *error) {
    return open_source(fd, length, limits, preflight_io_bytes, timeout, cancelled, opaque, out, info, error, 0, 0);
}
int deadpan_source_open_at_keyframe(int fd, int64_t length, const DeadpanSourceLimits *limits, uint64_t preflight_io_bytes, uint64_t timeout,
                        DeadpanCancelled cancelled, const void *opaque, DeadpanSource **out,
                        DeadpanSourceInfo *info, DeadpanSourceError *error, int64_t pts) {
    return open_source(fd, length, limits, preflight_io_bytes, timeout, cancelled, opaque, out, info, error, 1, pts);
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
static void export_metadata(DeadpanSource *s, DeadpanExportFrame *out) {
    AVFrame *f = s->frame;
    AVStream *stream = s->format->streams[s->stream];
    AVCodecParameters *p = stream->codecpar;
    *out = (DeadpanExportFrame){
        .best_effort_pts=f->best_effort_timestamp,
        .picture_type=f->pict_type,.decoder_profile=s->decoder->profile,.codec_profile=p->profile,
        .chroma_location=f->chroma_location,
        .stream_sar_num=stream->sample_aspect_ratio.num,.stream_sar_den=stream->sample_aspect_ratio.den,
        .codec_sar_num=p->sample_aspect_ratio.num,.codec_sar_den=p->sample_aspect_ratio.den,
        .frame_sar_num=f->sample_aspect_ratio.num,.frame_sar_den=f->sample_aspect_ratio.den,
        .flags=f->flags,.decode_error_flags=f->decode_error_flags,
        .interlaced=!!(f->flags & AV_FRAME_FLAG_INTERLACED),
        .top_field_first=!!(f->flags & AV_FRAME_FLAG_TOP_FIELD_FIRST),
        .corrupt=!!(f->flags & AV_FRAME_FLAG_CORRUPT)
    };
    metadata(s, &out->source);
}
static int i420(DeadpanSource *s, uint8_t *pixels, size_t length) {
    AVFrame *f = s->frame;
    if (f->format != AV_PIX_FMT_YUV420P || f->color_range != AVCOL_RANGE_MPEG ||
        f->colorspace != AVCOL_SPC_BT709 || f->color_trc != AVCOL_TRC_BT709 ||
        f->color_primaries != AVCOL_PRI_BT709 || (f->width & 1) || (f->height & 1))
        return fail(s, "unsupported_export_format", "export observations require even eight-bit limited Rec709 YUV420 pictures");
    size_t luma = (size_t)f->width * (size_t)f->height;
    if (!pixels || length != luma + luma / 2)
        return fail(s, "invalid_configuration", "I420 output must have the exact tight frame size");
    size_t output = 0;
    for (int plane = 0; plane < 3; plane++) {
        size_t columns = (size_t)f->width >> (plane != 0);
        size_t rows = (size_t)f->height >> (plane != 0);
        if (!f->data[plane] || f->linesize[plane] <= 0 || (size_t)f->linesize[plane] < columns)
            return fail(s, "invalid_picture", "decoded I420 plane has an unsupported stride");
        AVBufferRef *buffer = av_frame_get_plane_buffer(f, plane);
        if (rows - 1 > (SIZE_MAX - columns) / (size_t)f->linesize[plane])
            return fail(s, "resource_limit", "decoded I420 plane span overflows addressable memory");
        size_t span = (rows - 1) * (size_t)f->linesize[plane] + columns;
        if (!buffer || (uintptr_t)f->data[plane] < (uintptr_t)buffer->data)
            return fail(s, "invalid_picture", "decoded I420 plane has no owning buffer");
        uintptr_t offset = (uintptr_t)f->data[plane] - (uintptr_t)buffer->data;
        if (offset > buffer->size || span > buffer->size - offset)
            return fail(s, "invalid_picture", "decoded I420 plane escapes its owning buffer");
        for (size_t row = 0; row < rows; row++) {
            if (check(s) < 0) return -1;
            memcpy(pixels + output, f->data[plane] + row * (size_t)f->linesize[plane], columns);
            output += columns;
        }
    }
    return check(s);
}
static int packet_budget(DeadpanSource *s) {
    if (s->packet->size <= 0 || (uint64_t)s->packet->size > s->limits.max_packet_bytes)
        return fail(s, "resource_limit", "source packet exceeds configured byte bound");
    uint64_t bytes = (uint64_t)s->packet->size;
    for (int i = 0; i < s->packet->side_data_elems; i++) {
        const AVPacketSideData *side = &s->packet->side_data[i];
        if (side->size > s->limits.max_packet_bytes - bytes)
            return fail(s, "resource_limit", "source packet side data exceeds configured byte bound");
        bytes += side->size;
        if (side->type == AV_PKT_DATA_NEW_EXTRADATA || side->type == AV_PKT_DATA_PARAM_CHANGE)
            return fail(s, "stream_changed", "packet changes admitted codec configuration");
    }
    if (s->packet->stream_index == s->stream && s->h264_length_bytes) {
        size_t position = 0;
        uint32_t count = 0;
        size_t length = (size_t)s->packet->size;
        const uint8_t *data = s->packet->data;
        int idr = 0, other_vcl = 0;
        // A packet that FFmpeg would reinterpret as avcC is a configuration
        // change, not an ordinary length-prefixed picture packet.
        if (length >= 7 && data[0] == 1 && (data[4] & 0xfc) == 0xfc && (data[5] & 0xe0) == 0xe0)
            return fail(s, "stream_changed", "packet carries replacement AVC configuration");
        while (position < length) {
            if (check(s) < 0) return -1;
            if (++count > 4096) return fail(s, "resource_limit", "H264 packet exceeds 4096 NAL units");
            if ((size_t)s->h264_length_bytes > length - position)
                return fail(s, "invalid_input", "truncated H264 NAL length");
            uint32_t amount = 0;
            for (int i = 0; i < s->h264_length_bytes; i++) amount = (amount << 8) | data[position++];
            if (!amount || amount > length - position)
                return fail(s, "invalid_input", "H264 NAL escapes its packet");
            int type = data[position] & 31;
            if (type == 5) idr = 1;
            // An in-band SPS governs the following pictures; it must also
            // prove skipping safe, before this packet's skip decision.
            if (type == 7 && !sps_allows_skip(data + position, amount)) s->skip_safe = 0;
            if (type >= 1 && type <= 4) other_vcl = 1;
            position += amount;
        }
        if (s->fresh_key_packet_pending) {
            if (s->packet->pts != s->fresh_key_pts || !(s->packet->flags & AV_PKT_FLAG_KEY) || !idr || other_vcl)
                return fail(s, "invalid_keyframe", "fresh GOP requires an exact key packet containing only IDR picture slices");
            s->fresh_key_packet_pending = 0;
        }
    }
    return check(s);
}
static int receive_frame(DeadpanSource *s) {
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
            if (s->work.frames == UINT64_MAX) return fail(s, "resource_limit", "cumulative decoded frame count overflow");
            s->frames++;
            s->work.frames++;
            return 1;
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
            if (s->work.packets == UINT64_MAX) { av_packet_unref(s->packet); return fail(s, "resource_limit", "cumulative packet count overflow"); }
            packets++; s->packets++; s->work.packets++;
            if (packet_budget(s) < 0) { av_packet_unref(s->packet); return -1; }
            if (s->packet->flags & AV_PKT_FLAG_CORRUPT) { av_packet_unref(s->packet); return fail(s, "corrupt_packet", "demuxer reported a corrupt packet"); }
            if (s->packet->stream_index == s->stream) {
                if (table(s, 0) < 0 || packet_color_metadata(s, s->packet->side_data, s->packet->side_data_elems, "packet") < 0) {
                    av_packet_unref(s->packet);
                    return -1;
                }
                // A non-reference picture presented before the seek target is
                // never returned and no other picture depends on it. Pictures at
                // or after the target always decode, so forward steps continue.
                s->decoder->skip_frame = s->skip_nonref && s->skip_safe && s->packet->pts != AV_NOPTS_VALUE &&
                    s->packet->pts < s->skip_before_pts ? AVDISCARD_NONREF : AVDISCARD_DEFAULT;
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
static int next_impl(DeadpanSource *s, DeadpanSourceFrame *out, uint8_t *pixels, size_t length) {
    if (s->pending_first_frame) s->pending_first_frame = 0;
    else {
        int result = receive_frame(s);
        if (result <= 0) return result;
    }
    if (table(s, 0) < 0 || check_frame(s) < 0) return -1;
    metadata(s, out);
    return pixels ? rgba(s, pixels, length) : 1;
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
    if (s->pending_first_frame || !s->frame->buf[0]) return finish(s, fail(s, "no_current_frame", "decode a frame before copying its pixels"));
    metadata(s, frame);
    return finish(s, rgba(s, pixels, length));
}
static int seek_impl(DeadpanSource *s, int64_t pts, int skip, int64_t target) {
    if (pts == AV_NOPTS_VALUE || (skip && target == AV_NOPTS_VALUE))
        return fail(s, "invalid_timestamp", "seek timestamp is reserved for unknown PTS");
    av_frame_unref(s->frame); av_packet_unref(s->packet);
    s->pending_first_frame = 0;
    s->skip_nonref = 0;
    int result = av_seek_frame(s->format, s->stream, pts, AVSEEK_FLAG_BACKWARD);
    if (result < 0) return fferror(s, "seek source", result);
    avcodec_flush_buffers(s->decoder);
    s->draining = 0; s->ended = 0; s->frames = 0; s->packets = 0;
    s->skip_nonref = skip; s->skip_before_pts = target;
    return check(s);
}
int deadpan_source_seek(DeadpanSource *s, int64_t pts, uint64_t timeout, DeadpanCancelled cancelled,
                        const void *opaque, DeadpanSourceError *error) {
    if (begin(s, timeout, cancelled, opaque, error) < 0) return finish(s, -1);
    return finish(s, seek_impl(s, pts, 0, 0));
}
int deadpan_source_seek_to(DeadpanSource *s, int64_t pts, int64_t target, uint64_t timeout, DeadpanCancelled cancelled,
                        const void *opaque, DeadpanSourceError *error) {
    if (begin(s, timeout, cancelled, opaque, error) < 0) return finish(s, -1);
    return finish(s, seek_impl(s, pts, 1, target));
}
int deadpan_source_restart_at_keyframe(DeadpanSource *s, int64_t pts, uint64_t timeout, DeadpanCancelled cancelled,
                        const void *opaque, DeadpanSourceError *error) {
    if (begin(s, timeout, cancelled, opaque, error) < 0) return finish(s, -1);
    av_frame_unref(s->frame); av_packet_unref(s->packet);
    // Retain only admitted demux/index state. Replacing the whole codec, rather
    // than flushing it, removes every decoded reference picture and codec cache.
    avcodec_free_context(&s->decoder);
    s->skip_nonref = 0;
    if (seek_fresh_keyframe(s, pts) < 0 || allocate_decoder(s) < 0)
        return finish(s, -1);
    int result = receive_frame(s);
    if (result < 0) return finish(s, -1);
    if (!result) return finish(s, fail(s, "invalid_keyframe", "fresh GOP has no decoded picture"));
    if (table(s, 0) < 0 || check_frame(s) < 0 || check_fresh_keyframe(s) < 0)
        return finish(s, -1);
    s->pending_first_frame = 1;
    return finish(s, check(s));
}
int deadpan_source_next_i420(DeadpanSource *s, uint64_t timeout, DeadpanCancelled cancelled,
                        const void *opaque, DeadpanExportFrame *frame, uint8_t *pixels,
                        size_t length, DeadpanSourceError *error) {
    if (begin(s, timeout, cancelled, opaque, error) < 0) return finish(s, -1);
    DeadpanSourceFrame source;
    int result = next_impl(s, &source, NULL, 0);
    if (result <= 0) return finish(s, result);
    export_metadata(s, frame);
    return finish(s, i420(s, pixels, length));
}
int deadpan_source_copy_i420(DeadpanSource *s, uint64_t timeout, DeadpanCancelled cancelled,
                        const void *opaque, DeadpanExportFrame *frame, uint8_t *pixels,
                        size_t length, DeadpanSourceError *error) {
    if (begin(s, timeout, cancelled, opaque, error) < 0) return finish(s, -1);
    if (s->pending_first_frame || !s->frame->buf[0]) return finish(s, fail(s, "no_current_frame", "decode a frame before copying its pixels"));
    if (table(s, 0) < 0 || check_frame(s) < 0) return finish(s, -1);
    export_metadata(s, frame);
    return finish(s, i420(s, pixels, length));
}
void deadpan_source_work(const DeadpanSource *s, DeadpanDecodeWork *work) {
    *work = s->work;
    work->pictures = atomic_load(&((DeadpanSource *)s)->pictures);
}
void deadpan_source_runtime(DeadpanDecoderRuntime *runtime) {
    *runtime = (DeadpanDecoderRuntime){
        .avcodec=avcodec_version(),.avformat=avformat_version(),
        .avutil=avutil_version(),.swscale=swscale_version()
    };
}
int deadpan_source_lower_thread_priority(void) {
#ifdef __APPLE__
    return pthread_set_qos_class_self_np(QOS_CLASS_UTILITY, 0) == 0;
#else
    return 0;
#endif
}
