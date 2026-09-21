#define _POSIX_C_SOURCE 200809L

#include "converter.h"

#include <errno.h>
#include <inttypes.h>
#include <limits.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

#include <libavcodec/avcodec.h>
#include <libavcodec/version.h>
#include <libavformat/avformat.h>
#include <libavformat/version.h>
#include <libavutil/avutil.h>
#include <libavutil/error.h>
#include <libavutil/imgutils.h>
#include <libavutil/mem.h>
#include <libavutil/opt.h>
#include <libavutil/sha.h>
#include <libavutil/version.h>
#include <libswscale/swscale.h>
#include <libswscale/version.h>

#if LIBAVCODEC_VERSION_INT != AV_VERSION_INT(62, 11, 103)
#error "deadpan-media-worker requires libavcodec 62.11.103"
#endif
#if LIBAVFORMAT_VERSION_INT != AV_VERSION_INT(62, 3, 103)
#error "deadpan-media-worker requires libavformat 62.3.103"
#endif
#if LIBAVUTIL_VERSION_INT != AV_VERSION_INT(60, 8, 103)
#error "deadpan-media-worker requires libavutil 60.8.103"
#endif
#if LIBSWSCALE_VERSION_INT != AV_VERSION_INT(9, 1, 103)
#error "deadpan-media-worker requires libswscale 9.1.103"
#endif

#define IO_BUFFER_BYTES 32768
#define MAX_PROBE_BYTES (1024 * 1024)
#define MAX_STREAMS 8
#define MAX_DIMENSION 4096U
#define MAX_FRAMES 10000U
#define MAX_RATE 240U
#define MAX_FILE_BYTES (16ULL * 1024ULL * 1024ULL * 1024ULL)
#define OUTPUT_TIME_BASE_NUM 1
#define OUTPUT_TIME_BASE_DEN 1000

/* This state is process-global by design: the helper performs one conversion. */
typedef struct {
    DeadpanConversionError *error;
    uint64_t deadline_ns;
    int observed_ffv1_version;
    int observed_ffv1_ec;
    char ffmpeg_diagnostic[160];
} WorkerState;

static WorkerState state;

typedef struct {
    int fd;
    int64_t position;
    int64_t length;
    int64_t maximum;
    int writing;
} DescriptorIo;

typedef struct {
    AVIOContext *avio;
    DescriptorIo descriptor;
} OwnedAvio;

typedef struct {
    uint64_t frame_bytes;
    struct AVSHA *input_sha;
    uint32_t frame_count;
    uint32_t discarded_audio_streams;
    AVRational input_time_base;
    int64_t input_frame_ticks;
} DecodeResult;

static int fail(const char *code, const char *format, ...) {
    va_list arguments;
    if (state.error != NULL && state.error->code[0] == '\0') {
        (void)snprintf(state.error->code, sizeof(state.error->code), "%s", code);
        va_start(arguments, format);
        (void)vsnprintf(state.error->message, sizeof(state.error->message), format, arguments);
        va_end(arguments);
    }
    return 0;
}

static int fail_ffmpeg(const char *operation, int error) {
    char detail[AV_ERROR_MAX_STRING_SIZE];
    if (av_strerror(error, detail, sizeof(detail)) < 0) {
        (void)snprintf(detail, sizeof(detail), "FFmpeg error %d", error);
    }
    if (state.ffmpeg_diagnostic[0] != '\0') {
        return fail("ffmpeg_failure", "%s: %s; %s", operation, detail,
                    state.ffmpeg_diagnostic);
    }
    return fail("ffmpeg_failure", "%s: %s", operation, detail);
}

static uint64_t monotonic_ns(void) {
    struct timespec now;
    if (clock_gettime(CLOCK_MONOTONIC, &now) != 0) {
        return UINT64_MAX;
    }
    if ((uint64_t)now.tv_sec > UINT64_MAX / 1000000000ULL) {
        return UINT64_MAX;
    }
    return (uint64_t)now.tv_sec * 1000000000ULL + (uint64_t)now.tv_nsec;
}

static int within_deadline(void) {
    uint64_t now = monotonic_ns();
    if (now == UINT64_MAX) {
        return fail("internal_error", "read monotonic clock");
    }
    if (now > state.deadline_ns) {
        return fail("deadline_exceeded", "conversion exceeded its cooperative deadline");
    }
    return 1;
}

static int deadline_interrupt(void *opaque) {
    const WorkerState *worker = opaque;
    uint64_t now = monotonic_ns();
    if (now == UINT64_MAX) {
        fail("internal_error", "read monotonic clock");
        return 1;
    }
    if (now > worker->deadline_ns) {
        fail("deadline_exceeded", "conversion exceeded its cooperative deadline");
        return 1;
    }
    return 0;
}

static int deny_external_io(AVFormatContext *format, AVIOContext **io, const char *url,
                            int flags, AVDictionary **options) {
    (void)format;
    (void)io;
    (void)url;
    (void)flags;
    (void)options;
    return AVERROR(EPERM);
}

static int checked_mul_u64(uint64_t left, uint64_t right, uint64_t *result) {
    if (right != 0 && left > UINT64_MAX / right) {
        return 0;
    }
    *result = left * right;
    return 1;
}

static int checked_add_u64(uint64_t left, uint64_t right, uint64_t *result) {
    if (left > UINT64_MAX - right) {
        return 0;
    }
    *result = left + right;
    return 1;
}

static uint32_t gcd_u32(uint32_t left, uint32_t right) {
    while (right != 0) {
        uint32_t remainder = left % right;
        left = right;
        right = remainder;
    }
    return left;
}

static int valid_rate(uint32_t numerator, uint32_t denominator) {
    return numerator > 0 && denominator > 0 && numerator <= INT_MAX &&
           denominator <= INT_MAX && numerator >= denominator &&
           (uint64_t)numerator <= (uint64_t)MAX_RATE * denominator &&
           gcd_u32(numerator, denominator) == 1;
}

static int validate_request(const DeadpanConversionRequest *request) {
    if (request->width == 0 || request->height == 0 ||
        request->width > MAX_DIMENSION || request->height > MAX_DIMENSION ||
        request->frames == 0 || request->frames > MAX_FRAMES ||
        request->output_frames == 0 || request->output_frames > MAX_FRAMES ||
        !valid_rate(request->rate_num, request->rate_den) ||
        !valid_rate(request->output_rate_num, request->output_rate_den)) {
        return fail("invalid_request", "video contract is outside worker bounds");
    }
    if (request->sample_bridge > 1 ||
        (!request->sample_bridge &&
         (request->frames != request->output_frames ||
          request->rate_num != request->output_rate_num ||
          request->rate_den != request->output_rate_den)) ||
        (request->sample_bridge && request->frames < 2)) {
        return fail("invalid_request", "bridge sampling configuration is inconsistent");
    }
    if (request->input_byte_length == 0 || request->max_input_bytes == 0 ||
        request->max_output_bytes == 0 || request->max_scratch_bytes == 0 ||
        request->timeout_ms == 0 || request->input_byte_length > request->max_input_bytes ||
        request->max_input_bytes > MAX_FILE_BYTES ||
        request->max_output_bytes > MAX_FILE_BYTES ||
        request->max_scratch_bytes > MAX_FILE_BYTES ||
        request->timeout_ms > 24ULL * 60ULL * 60ULL * 1000ULL) {
        return fail("invalid_request", "conversion limits are invalid");
    }
    return 1;
}

static int64_t decoder_pixel_bound(const DeadpanConversionRequest *request) {
    uint64_t aligned_width = ((uint64_t)request->width + 63U) & ~63ULL;
    uint64_t aligned_height = ((uint64_t)request->height + 63U) & ~63ULL;
    return (int64_t)(aligned_width * aligned_height);
}

static void worker_log(void *context, int level, const char *format, va_list arguments) {
    char message[256];
    int version;
    int ec;
    (void)context;
    (void)vsnprintf(message, sizeof(message), format, arguments);
    if (sscanf(message, "ver:%d keyframe:%*d coder:%*d ec:%d", &version, &ec) == 2) {
        state.observed_ffv1_version = version;
        state.observed_ffv1_ec = ec;
    }
    if (level <= AV_LOG_ERROR && state.ffmpeg_diagnostic[0] == '\0') {
        size_t length = strcspn(message, "\r\n");
        if (length >= sizeof(state.ffmpeg_diagnostic)) {
            length = sizeof(state.ffmpeg_diagnostic) - 1;
        }
        memcpy(state.ffmpeg_diagnostic, message, length);
        state.ffmpeg_diagnostic[length] = '\0';
    }
}

static int descriptor_read(void *opaque, uint8_t *buffer, int buffer_size) {
    DescriptorIo *io = opaque;
    if (!within_deadline()) {
        return AVERROR(ETIMEDOUT);
    }
    if (buffer_size <= 0 || io->position >= io->length) {
        return AVERROR_EOF;
    }
    int64_t remaining = io->length - io->position;
    size_t amount = (size_t)buffer_size;
    if ((int64_t)amount > remaining) {
        amount = (size_t)remaining;
    }
    ssize_t count;
    do {
        count = pread(io->fd, buffer, amount, (off_t)io->position);
    } while (count < 0 && errno == EINTR);
    if (count < 0) {
        return AVERROR(errno);
    }
    if (count == 0) {
        return AVERROR_EOF;
    }
    io->position += count;
    return (int)count;
}

static int descriptor_write(void *opaque, const uint8_t *buffer, int buffer_size) {
    DescriptorIo *io = opaque;
    int written = 0;
    if (!within_deadline()) {
        return AVERROR(ETIMEDOUT);
    }
    if (buffer_size < 0 || io->position < 0 ||
        (int64_t)buffer_size > io->maximum - io->position) {
        fail("output_too_large", "FFV1 output exceeds its byte budget");
        return AVERROR(ENOSPC);
    }
    while (written < buffer_size) {
        ssize_t count = pwrite(io->fd, buffer + written, (size_t)(buffer_size - written),
                               (off_t)(io->position + written));
        if (count < 0 && errno == EINTR) {
            continue;
        }
        if (count <= 0) {
            return AVERROR(count < 0 ? errno : EIO);
        }
        written += (int)count;
    }
    io->position += written;
    if (io->position > io->length) {
        io->length = io->position;
    }
    return written;
}

static int64_t descriptor_seek(void *opaque, int64_t offset, int whence) {
    DescriptorIo *io = opaque;
    int64_t base;
    int flags = whence & ~AVSEEK_FORCE;
    if (!within_deadline()) {
        return AVERROR(ETIMEDOUT);
    }
    if (flags == AVSEEK_SIZE) {
        return io->length;
    }
    switch (flags) {
    case SEEK_SET:
        base = 0;
        break;
    case SEEK_CUR:
        base = io->position;
        break;
    case SEEK_END:
        base = io->length;
        break;
    default:
        return AVERROR(EINVAL);
    }
    if ((offset > 0 && base > INT64_MAX - offset) ||
        (offset < 0 && base < INT64_MIN - offset)) {
        return AVERROR(EOVERFLOW);
    }
    int64_t position = base + offset;
    int64_t boundary = io->writing ? io->maximum : io->length;
    if (position < 0 || position > boundary) {
        if (io->writing && position > boundary) {
            fail("output_too_large", "FFV1 output seek exceeds its byte budget");
        }
        return AVERROR(EINVAL);
    }
    io->position = position;
    return position;
}

static int owned_avio_open(OwnedAvio *owned, int fd, int64_t length, int64_t maximum,
                           int writing) {
    uint8_t *buffer = av_malloc(IO_BUFFER_BYTES);
    memset(owned, 0, sizeof(*owned));
    if (buffer == NULL) {
        return fail("resource_exhausted", "allocate descriptor I/O buffer");
    }
    owned->descriptor.fd = fd;
    owned->descriptor.length = length;
    owned->descriptor.maximum = maximum;
    owned->descriptor.writing = writing;
    owned->avio = avio_alloc_context(buffer, IO_BUFFER_BYTES, writing, &owned->descriptor,
                                     writing ? NULL : descriptor_read,
                                     writing ? descriptor_write : NULL, descriptor_seek);
    if (owned->avio == NULL) {
        av_free(buffer);
        return fail("resource_exhausted", "allocate descriptor I/O context");
    }
    owned->avio->seekable = AVIO_SEEKABLE_NORMAL;
    return 1;
}

static void owned_avio_close(OwnedAvio *owned) {
    if (owned->avio != NULL) {
        av_freep(&owned->avio->buffer);
        avio_context_free(&owned->avio);
    }
    memset(owned, 0, sizeof(*owned));
}

static int open_input_format(AVFormatContext **format, OwnedAvio *io, int fd, int64_t length,
                             const char *demuxer_name, const char *codec_name,
                             enum AVCodecID codec_id, const DeadpanConversionRequest *request,
                             int allow_audio) {
    AVDictionary **decoder_options = NULL;
    const AVInputFormat *demuxer = av_find_input_format(demuxer_name);
    int result;
    if (demuxer == NULL) {
        return fail("runtime_mismatch", "required %s demuxer is unavailable", demuxer_name);
    }
    if (!owned_avio_open(io, fd, length, length, 0)) {
        return 0;
    }
    *format = avformat_alloc_context();
    if (*format == NULL) {
        return fail("resource_exhausted", "allocate input format context");
    }
    (*format)->pb = io->avio;
    (*format)->flags |= AVFMT_FLAG_CUSTOM_IO;
    (*format)->probesize = length < MAX_PROBE_BYTES ? length : MAX_PROBE_BYTES;
    (*format)->max_analyze_duration = 5000000;
    (*format)->max_streams = MAX_STREAMS;
    (*format)->max_probe_packets = 256;
    (*format)->interrupt_callback.callback = deadline_interrupt;
    (*format)->interrupt_callback.opaque = &state;
    (*format)->io_open = deny_external_io;
    (*format)->protocol_whitelist = av_strdup("");
    (*format)->format_whitelist = av_strdup(demuxer_name);
    (*format)->codec_whitelist = av_strdup(codec_name);
    if ((*format)->protocol_whitelist == NULL || (*format)->format_whitelist == NULL ||
        (*format)->codec_whitelist == NULL) {
        return fail("resource_exhausted", "allocate FFmpeg input allowlists");
    }
    result = avformat_open_input(format, NULL, demuxer, NULL);
    if (result < 0) {
        return fail_ffmpeg("open input descriptor", result);
    }
    unsigned int videos = 0;
    for (unsigned int index = 0; index < (*format)->nb_streams; index++) {
        AVCodecParameters *parameters = (*format)->streams[index]->codecpar;
        if (parameters->codec_type == AVMEDIA_TYPE_VIDEO) {
            videos++;
            if (parameters->codec_id != codec_id || parameters->width != (int)request->width ||
                parameters->height != (int)request->height) {
                return fail("invalid_media", "video header violates the codec or dimension allowlist");
            }
        } else if (parameters->codec_type != AVMEDIA_TYPE_AUDIO || !allow_audio) {
            return fail("invalid_media", "container header has a forbidden stream type");
        }
    }
    if (videos != 1) {
        return fail("invalid_media", "container must have exactly one video stream");
    }
    unsigned int options_count = (*format)->nb_streams;
    decoder_options = av_calloc(options_count, sizeof(*decoder_options));
    if (decoder_options == NULL) {
        return fail("resource_exhausted", "allocate stream probe options");
    }
    int64_t max_pixels = decoder_pixel_bound(request);
    int options_error = 0;
    for (unsigned int index = 0; index < options_count; index++) {
        if ((*format)->streams[index]->codecpar->codec_type == AVMEDIA_TYPE_VIDEO) {
            if (av_dict_set(&decoder_options[index], "threads", "1", 0) < 0 ||
                av_dict_set(&decoder_options[index], "err_detect", "explode", 0) < 0 ||
                av_dict_set_int(&decoder_options[index], "max_pixels", max_pixels, 0) < 0) {
                options_error = 1;
            }
        } else {
            (*format)->streams[index]->discard = AVDISCARD_ALL;
        }
    }
    result = options_error ? AVERROR(ENOMEM)
                           : avformat_find_stream_info(*format, decoder_options);
    for (unsigned int index = 0; index < options_count; index++) {
        av_dict_free(&decoder_options[index]);
    }
    av_free(decoder_options);
    if (result < 0) {
        return fail_ffmpeg("read input stream information", result);
    }
    if ((*format)->nb_streams != options_count) {
        return fail("invalid_media", "container stream table changed during probing");
    }
    return within_deadline();
}

static void close_input_format(AVFormatContext **format, OwnedAvio *io) {
    if (*format != NULL) {
        (*format)->pb = NULL;
        avformat_close_input(format);
    }
    owned_avio_close(io);
}

static int exact_write_at(int fd, const uint8_t *bytes, uint64_t amount, uint64_t offset) {
    uint64_t written = 0;
    while (written < amount) {
        size_t part = amount - written > (uint64_t)SSIZE_MAX ? (size_t)SSIZE_MAX
                                                             : (size_t)(amount - written);
        ssize_t count = pwrite(fd, bytes + written, part, (off_t)(offset + written));
        if (count < 0 && errno == EINTR) {
            continue;
        }
        if (count <= 0) {
            return fail("io_failure", "write RGB scratch: %s",
                        count < 0 ? strerror(errno) : "short write");
        }
        written += (uint64_t)count;
    }
    return 1;
}

static int exact_read_at(int fd, uint8_t *bytes, uint64_t amount, uint64_t offset) {
    uint64_t read = 0;
    while (read < amount) {
        size_t part = amount - read > (uint64_t)SSIZE_MAX ? (size_t)SSIZE_MAX
                                                          : (size_t)(amount - read);
        ssize_t count = pread(fd, bytes + read, part, (off_t)(offset + read));
        if (count < 0 && errno == EINTR) {
            continue;
        }
        if (count <= 0) {
            return fail("io_failure", "read RGB scratch: %s",
                        count < 0 ? strerror(errno) : "unexpected end of file");
        }
        read += (uint64_t)count;
    }
    return 1;
}

static int packed_rgb(AVFrame *rgb, uint32_t width, uint32_t height, uint8_t *packed) {
    size_t row_bytes = (size_t)width * 3;
    for (uint32_t row = 0; row < height; row++) {
        memcpy(packed + (size_t)row * row_bytes,
               rgb->data[0] + (size_t)row * (size_t)rgb->linesize[0], row_bytes);
    }
    return 1;
}

static int check_frame_tags(const AVFrame *frame) {
    return frame->color_range == AVCOL_RANGE_JPEG && frame->colorspace == AVCOL_SPC_RGB &&
           frame->color_trc == AVCOL_TRC_IEC61966_2_1 &&
           frame->color_primaries == AVCOL_PRI_BT709;
}

static int64_t expected_input_pts(uint32_t index, const DecodeResult *result) {
    return (int64_t)index * result->input_frame_ticks;
}

static int receive_input_frames(AVCodecContext *decoder, AVFrame *decoded, AVFrame *rgb,
                                struct SwsContext *to_rgb, int scratch_fd, uint8_t *packed,
                                const DeadpanConversionRequest *request, DecodeResult *result) {
    for (;;) {
        int code = avcodec_receive_frame(decoder, decoded);
        if (code == AVERROR(EAGAIN) || code == AVERROR_EOF) {
            return 1;
        }
        if (code < 0) {
            return fail_ffmpeg("decode input frame", code);
        }
        if (!within_deadline()) {
            return 0;
        }
        uint32_t index = result->frame_count;
        if (index >= request->frames) {
            return fail("invalid_media", "input contains more than %u video frames",
                        request->frames);
        }
        if (decoded->width != (int)request->width ||
            decoded->height != (int)request->height || decoded->format != AV_PIX_FMT_GBRP ||
            decoded->best_effort_timestamp == AV_NOPTS_VALUE || !check_frame_tags(decoded)) {
            return fail("invalid_media",
                        "input frame changes dimensions, pixel format, color tags, or lacks PTS");
        }
        int64_t wanted_pts = expected_input_pts(index, result);
        int64_t wanted_next = expected_input_pts(index + 1, result);
        if (decoded->best_effort_timestamp != wanted_pts || decoded->duration <= 0 ||
            wanted_next <= wanted_pts || decoded->duration != wanted_next - wanted_pts) {
            return fail("invalid_media", "input frame %u is not exact zero-based CFR", index);
        }
        if (sws_scale(to_rgb, (const uint8_t *const *)decoded->data, decoded->linesize, 0,
                      decoded->height, rgb->data, rgb->linesize) != decoded->height) {
            return fail("ffmpeg_failure", "convert input frame to RGB8");
        }
        packed_rgb(rgb, request->width, request->height, packed);
        uint64_t offset;
        if (!checked_mul_u64(index, result->frame_bytes, &offset) ||
            !exact_write_at(scratch_fd, packed, result->frame_bytes, offset)) {
            return 0;
        }
        av_sha_update(result->input_sha, packed, (size_t)result->frame_bytes);
        result->frame_count++;
        av_frame_unref(decoded);
    }
}

static int decode_input(int input_fd, int scratch_fd, const DeadpanConversionRequest *request,
                        DecodeResult *result) {
    OwnedAvio io = {0};
    AVFormatContext *format = NULL;
    AVCodecContext *decoder = NULL;
    const AVCodec *codec = NULL;
    AVPacket *packet = NULL;
    AVFrame *decoded = NULL;
    AVFrame *rgb = NULL;
    struct SwsContext *to_rgb = NULL;
    uint8_t *packed = NULL;
    int video_index = -1;
    int success = 0;

    memset(result, 0, sizeof(*result));
    if (!checked_mul_u64(request->width, request->height, &result->frame_bytes) ||
        !checked_mul_u64(result->frame_bytes, 3, &result->frame_bytes)) {
        return fail("invalid_request", "RGB frame size overflow");
    }
    if (!open_input_format(&format, &io, input_fd, (int64_t)request->input_byte_length, "mov",
                           "h264", AV_CODEC_ID_H264, request, 1)) {
        goto cleanup;
    }
    for (unsigned int index = 0; index < format->nb_streams; index++) {
        enum AVMediaType kind = format->streams[index]->codecpar->codec_type;
        if (kind == AVMEDIA_TYPE_VIDEO) {
            if (video_index >= 0) {
                fail("invalid_media", "input contains multiple video streams");
                goto cleanup;
            }
            video_index = (int)index;
        } else if (kind == AVMEDIA_TYPE_AUDIO) {
            if (result->discarded_audio_streams == UINT32_MAX) {
                fail("invalid_media", "input contains too many audio streams");
                goto cleanup;
            }
            result->discarded_audio_streams++;
        } else {
            fail("invalid_media", "input contains unsupported non-audio/video stream type");
            goto cleanup;
        }
    }
    if (video_index < 0) {
        fail("invalid_media", "input contains no video stream");
        goto cleanup;
    }
    AVStream *stream = format->streams[video_index];
    AVRational requested_rate = {(int)request->rate_num, (int)request->rate_den};
    if (stream->time_base.num <= 0 || stream->time_base.den <= 0 ||
        av_cmp_q(stream->avg_frame_rate, requested_rate) != 0 ||
        stream->codecpar->width != (int)request->width ||
        stream->codecpar->height != (int)request->height ||
        stream->codecpar->format != AV_PIX_FMT_GBRP ||
        stream->codecpar->bits_per_raw_sample != 8 ||
        stream->codecpar->color_range != AVCOL_RANGE_JPEG ||
        stream->codecpar->color_space != AVCOL_SPC_RGB ||
        stream->codecpar->color_trc != AVCOL_TRC_IEC61966_2_1 ||
        stream->codecpar->color_primaries != AVCOL_PRI_BT709) {
        fail("invalid_media", "input stream does not match the requested GBRP8 video contract");
        goto cleanup;
    }
    result->input_time_base = stream->time_base;
    uint64_t tick_numerator = (uint64_t)request->rate_den * (uint64_t)stream->time_base.den;
    uint64_t tick_denominator =
        (uint64_t)request->rate_num * (uint64_t)stream->time_base.num;
    if (tick_denominator == 0 || tick_numerator % tick_denominator != 0 ||
        tick_numerator / tick_denominator > (uint64_t)INT64_MAX / request->frames) {
        fail("invalid_media", "input time base cannot represent the requested CFR exactly");
        goto cleanup;
    }
    result->input_frame_ticks = (int64_t)(tick_numerator / tick_denominator);
    if (result->input_frame_ticks <= 0) {
        fail("invalid_media", "input time base has no positive exact frame duration");
        goto cleanup;
    }
    for (int index = 0; index < stream->codecpar->nb_coded_side_data; index++) {
        if (stream->codecpar->coded_side_data[index].type == AV_PKT_DATA_DISPLAYMATRIX) {
            fail("invalid_media", "input contains an unsupported display transform");
            goto cleanup;
        }
    }
    codec = avcodec_find_decoder(stream->codecpar->codec_id);
    if (codec == NULL) {
        fail("unsupported_media", "input video decoder is unavailable");
        goto cleanup;
    }
    decoder = avcodec_alloc_context3(codec);
    if (decoder == NULL) {
        fail("resource_exhausted", "allocate input decoder");
        goto cleanup;
    }
    int code = avcodec_parameters_to_context(decoder, stream->codecpar);
    if (code < 0) {
        fail_ffmpeg("copy input decoder parameters", code);
        goto cleanup;
    }
    decoder->thread_count = 1;
    decoder->err_recognition = AV_EF_EXPLODE;
    decoder->max_pixels = decoder_pixel_bound(request);
    code = avcodec_open2(decoder, codec, NULL);
    if (code < 0) {
        fail_ffmpeg("open input decoder", code);
        goto cleanup;
    }
    if (decoder->width != (int)request->width || decoder->height != (int)request->height ||
        decoder->pix_fmt != AV_PIX_FMT_GBRP) {
        fail("invalid_media", "input decoder changed dimensions or pixel format");
        goto cleanup;
    }
    packet = av_packet_alloc();
    decoded = av_frame_alloc();
    rgb = av_frame_alloc();
    result->input_sha = av_sha_alloc();
    packed = av_malloc((size_t)result->frame_bytes);
    if (packet == NULL || decoded == NULL || rgb == NULL || result->input_sha == NULL ||
        packed == NULL || av_sha_init(result->input_sha, 256) < 0 ||
        av_image_alloc(rgb->data, rgb->linesize, (int)request->width, (int)request->height,
                       AV_PIX_FMT_RGB24, 32) < 0) {
        fail("resource_exhausted", "allocate bounded input conversion buffers");
        goto cleanup;
    }
    rgb->width = (int)request->width;
    rgb->height = (int)request->height;
    rgb->format = AV_PIX_FMT_RGB24;
    to_rgb = sws_getContext((int)request->width, (int)request->height, AV_PIX_FMT_GBRP,
                            (int)request->width, (int)request->height, AV_PIX_FMT_RGB24,
                            SWS_POINT, NULL, NULL, NULL);
    if (to_rgb == NULL) {
        fail("resource_exhausted", "create input RGB converter");
        goto cleanup;
    }

    for (;;) {
        if (!within_deadline()) {
            goto cleanup;
        }
        code = av_read_frame(format, packet);
        if (code == AVERROR_EOF) {
            code = avcodec_send_packet(decoder, NULL);
            if (code < 0 && code != AVERROR_EOF) {
                fail_ffmpeg("flush input decoder", code);
                goto cleanup;
            }
            if (!receive_input_frames(decoder, decoded, rgb, to_rgb, scratch_fd, packed, request,
                                      result)) {
                goto cleanup;
            }
            break;
        }
        if (code < 0) {
            fail_ffmpeg("read input packet", code);
            goto cleanup;
        }
        if (packet->stream_index == video_index) {
            code = avcodec_send_packet(decoder, packet);
            av_packet_unref(packet);
            if (code < 0) {
                fail_ffmpeg("send input packet", code);
                goto cleanup;
            }
            if (!receive_input_frames(decoder, decoded, rgb, to_rgb, scratch_fd, packed, request,
                                      result)) {
                goto cleanup;
            }
        } else {
            av_packet_unref(packet);
        }
    }
    if (result->frame_count != request->frames) {
        fail("invalid_media", "input decoded %u frames, expected %u", result->frame_count,
             request->frames);
        goto cleanup;
    }
    success = 1;

cleanup:
    sws_freeContext(to_rgb);
    if (rgb != NULL) {
        av_freep(&rgb->data[0]);
    }
    av_free(packed);
    av_frame_free(&rgb);
    av_frame_free(&decoded);
    av_packet_free(&packet);
    avcodec_free_context(&decoder);
    close_input_format(&format, &io);
    return success;
}

static int64_t output_pts(uint32_t boundary, const DeadpanConversionRequest *request) {
    AVRational frame_period = {(int)request->output_rate_den,
                               (int)request->output_rate_num};
    AVRational milliseconds = {OUTPUT_TIME_BASE_NUM, OUTPUT_TIME_BASE_DEN};
    return av_rescale_q_rnd((int64_t)boundary, frame_period, milliseconds,
                            AV_ROUND_NEAR_INF | AV_ROUND_PASS_MINMAX);
}

static int output_rgb_from_scratch(int scratch_fd, uint32_t output_index,
                                   const DeadpanConversionRequest *request,
                                   uint64_t frame_bytes, uint8_t *output,
                                   uint8_t *left, uint8_t *right) {
    if (!request->sample_bridge) {
        uint64_t offset;
        return checked_mul_u64(output_index, frame_bytes, &offset) &&
               exact_read_at(scratch_fd, output, frame_bytes, offset);
    }
    uint64_t denominator = (uint64_t)request->output_frames + 1;
    uint64_t numerator = ((uint64_t)output_index + 1) *
                         ((uint64_t)request->frames - 1);
    uint64_t lower = numerator / denominator;
    uint64_t remainder = numerator % denominator;
    uint64_t upper = lower + (remainder != 0);
    uint64_t left_offset;
    uint64_t right_offset;
    if (left == NULL || right == NULL || lower >= request->frames ||
        upper >= request->frames ||
        !checked_mul_u64(lower, frame_bytes, &left_offset) ||
        !exact_read_at(scratch_fd, left, frame_bytes, left_offset)) {
        return fail("invalid_request", "bridge sample coordinate is outside native scratch");
    }
    if (remainder == 0) {
        memcpy(output, left, (size_t)frame_bytes);
        return 1;
    }
    if (!checked_mul_u64(upper, frame_bytes, &right_offset) ||
        !exact_read_at(scratch_fd, right, frame_bytes, right_offset)) {
        return 0;
    }
    uint64_t left_weight = denominator - remainder;
    for (uint64_t byte = 0; byte < frame_bytes; byte++) {
        uint64_t weighted = (uint64_t)left[byte] * left_weight +
                            (uint64_t)right[byte] * remainder;
        output[byte] = (uint8_t)((weighted + denominator / 2) / denominator);
    }
    return 1;
}

static int write_encoder_packets(AVCodecContext *codec, AVFormatContext *format, AVStream *stream,
                                 AVPacket *packet) {
    for (;;) {
        int code = avcodec_receive_packet(codec, packet);
        if (code == AVERROR(EAGAIN) || code == AVERROR_EOF) {
            return 1;
        }
        if (code < 0) {
            return fail_ffmpeg("receive FFV1 packet", code);
        }
        av_packet_rescale_ts(packet, codec->time_base, stream->time_base);
        packet->stream_index = stream->index;
        code = av_interleaved_write_frame(format, packet);
        av_packet_unref(packet);
        if (code < 0) {
            return fail_ffmpeg("write bounded FFV1 packet", code);
        }
        if (!within_deadline()) {
            return 0;
        }
    }
}

static int encode_output(int output_fd, int scratch_fd, const DeadpanConversionRequest *request,
                         uint64_t frame_bytes, uint64_t *output_bytes) {
    OwnedAvio io = {0};
    AVFormatContext *format = NULL;
    AVCodecContext *encoder = NULL;
    const AVCodec *codec = NULL;
    AVStream *stream = NULL;
    AVPacket *packet = NULL;
    AVFrame *rgb = NULL;
    AVFrame *bgr0 = NULL;
    struct SwsContext *to_bgr0 = NULL;
    uint8_t *packed = NULL;
    uint8_t *sample_left = NULL;
    uint8_t *sample_right = NULL;
    int success = 0;

    int code = avformat_alloc_output_context2(&format, NULL, "matroska", NULL);
    if (code < 0 || format == NULL) {
        fail_ffmpeg("create Matroska output", code < 0 ? code : AVERROR_UNKNOWN);
        goto cleanup;
    }
    if (!owned_avio_open(&io, output_fd, 0, (int64_t)request->max_output_bytes, 1)) {
        goto cleanup;
    }
    format->pb = io.avio;
    format->flags |= AVFMT_FLAG_CUSTOM_IO;
    format->interrupt_callback.callback = deadline_interrupt;
    format->interrupt_callback.opaque = &state;
    format->io_open = deny_external_io;
    format->protocol_whitelist = av_strdup("");
    if (format->protocol_whitelist == NULL) {
        fail("resource_exhausted", "allocate FFmpeg output protocol denylist");
        goto cleanup;
    }
    codec = avcodec_find_encoder(AV_CODEC_ID_FFV1);
    if (codec == NULL) {
        fail("unsupported_media", "FFV1 encoder is unavailable");
        goto cleanup;
    }
    encoder = avcodec_alloc_context3(codec);
    stream = avformat_new_stream(format, NULL);
    if (encoder == NULL || stream == NULL) {
        fail("resource_exhausted", "allocate FFV1 encoder and stream");
        goto cleanup;
    }
    encoder->width = (int)request->width;
    encoder->height = (int)request->height;
    encoder->pix_fmt = AV_PIX_FMT_BGR0;
    encoder->time_base = (AVRational){OUTPUT_TIME_BASE_NUM, OUTPUT_TIME_BASE_DEN};
    encoder->framerate = (AVRational){(int)request->output_rate_num,
                                      (int)request->output_rate_den};
    encoder->gop_size = 1;
    encoder->max_b_frames = 0;
    encoder->thread_count = 1;
    encoder->level = 3;
    encoder->color_range = AVCOL_RANGE_JPEG;
    encoder->colorspace = AVCOL_SPC_RGB;
    encoder->color_trc = AVCOL_TRC_IEC61966_2_1;
    encoder->color_primaries = AVCOL_PRI_BT709;
    code = av_opt_set_int(encoder->priv_data, "slicecrc", 1, 0);
    if (code < 0) {
        fail_ffmpeg("enable FFV1 slice CRC", code);
        goto cleanup;
    }
    code = avcodec_open2(encoder, codec, NULL);
    if (code < 0) {
        fail_ffmpeg("open FFV1 encoder", code);
        goto cleanup;
    }
    stream->avg_frame_rate = encoder->framerate;
    stream->r_frame_rate = encoder->framerate;
    stream->time_base = encoder->time_base;
    code = avcodec_parameters_from_context(stream->codecpar, encoder);
    if (code < 0) {
        fail_ffmpeg("copy FFV1 stream parameters", code);
        goto cleanup;
    }
    av_dict_set(&stream->metadata, "COLOR_RANGE", "full", 0);
    av_dict_set(&stream->metadata, "COLOR_SPACE", "gbr", 0);
    av_dict_set(&stream->metadata, "COLOR_TRANSFER", "sRGB", 0);
    av_dict_set(&stream->metadata, "COLOR_PRIMARIES", "bt709", 0);
    code = avformat_write_header(format, NULL);
    if (code < 0) {
        fail_ffmpeg("write Matroska header", code);
        goto cleanup;
    }
    packet = av_packet_alloc();
    rgb = av_frame_alloc();
    bgr0 = av_frame_alloc();
    packed = av_malloc((size_t)frame_bytes);
    if (request->sample_bridge) {
        sample_left = av_malloc((size_t)frame_bytes);
        sample_right = av_malloc((size_t)frame_bytes);
    }
    if (packet == NULL || rgb == NULL || bgr0 == NULL || packed == NULL) {
        fail("resource_exhausted", "allocate bounded FFV1 conversion buffers");
        goto cleanup;
    }
    if (request->sample_bridge && (sample_left == NULL || sample_right == NULL)) {
        fail("resource_exhausted", "allocate bounded bridge sampling buffers");
        goto cleanup;
    }
    rgb->format = AV_PIX_FMT_RGB24;
    rgb->width = (int)request->width;
    rgb->height = (int)request->height;
    bgr0->format = AV_PIX_FMT_BGR0;
    bgr0->width = (int)request->width;
    bgr0->height = (int)request->height;
    if (av_frame_get_buffer(rgb, 32) < 0 || av_frame_get_buffer(bgr0, 32) < 0) {
        fail("resource_exhausted", "allocate FFV1 frames");
        goto cleanup;
    }
    to_bgr0 = sws_getContext((int)request->width, (int)request->height, AV_PIX_FMT_RGB24,
                             (int)request->width, (int)request->height, AV_PIX_FMT_BGR0,
                             SWS_POINT, NULL, NULL, NULL);
    if (to_bgr0 == NULL) {
        fail("resource_exhausted", "create BGR0 converter");
        goto cleanup;
    }
    for (uint32_t index = 0; index < request->output_frames; index++) {
        if (!within_deadline()) {
            goto cleanup;
        }
        if (!output_rgb_from_scratch(scratch_fd, index, request, frame_bytes,
                                     packed, sample_left, sample_right) ||
            av_frame_make_writable(rgb) < 0 || av_frame_make_writable(bgr0) < 0) {
            if (state.error->code[0] == '\0') {
                fail("resource_exhausted", "prepare writable FFV1 frame");
            }
            goto cleanup;
        }
        size_t row_bytes = (size_t)request->width * 3;
        for (uint32_t row = 0; row < request->height; row++) {
            memcpy(rgb->data[0] + (size_t)row * (size_t)rgb->linesize[0],
                   packed + (size_t)row * row_bytes, row_bytes);
        }
        if (sws_scale(to_bgr0, (const uint8_t *const *)rgb->data, rgb->linesize, 0,
                      (int)request->height, bgr0->data, bgr0->linesize) !=
            (int)request->height) {
            fail("ffmpeg_failure", "convert RGB8 frame to BGR0");
            goto cleanup;
        }
        int64_t pts = output_pts(index, request);
        int64_t next = output_pts(index + 1, request);
        if (next <= pts) {
            fail("invalid_request", "frame rate cannot produce positive Matroska durations");
            goto cleanup;
        }
        bgr0->pts = pts;
        bgr0->duration = next - pts;
        code = avcodec_send_frame(encoder, bgr0);
        if (code < 0 || !write_encoder_packets(encoder, format, stream, packet)) {
            if (code < 0) {
                fail_ffmpeg("send FFV1 frame", code);
            }
            goto cleanup;
        }
    }
    code = avcodec_send_frame(encoder, NULL);
    if (code < 0 || !write_encoder_packets(encoder, format, stream, packet)) {
        if (code < 0) {
            fail_ffmpeg("flush FFV1 encoder", code);
        }
        goto cleanup;
    }
    code = av_write_trailer(format);
    if (code < 0) {
        fail_ffmpeg("write Matroska trailer", code);
        goto cleanup;
    }
    avio_flush(format->pb);
    if (format->pb->error < 0) {
        fail_ffmpeg("flush Matroska output", format->pb->error);
        goto cleanup;
    }
    *output_bytes = (uint64_t)io.descriptor.length;
    if (*output_bytes == 0 || *output_bytes > request->max_output_bytes ||
        ftruncate(output_fd, (off_t)*output_bytes) != 0) {
        fail("io_failure", "finalize bounded Matroska output: %s", strerror(errno));
        goto cleanup;
    }
    success = 1;

cleanup:
    av_free(sample_right);
    av_free(sample_left);
    av_free(packed);
    sws_freeContext(to_bgr0);
    av_frame_free(&bgr0);
    av_frame_free(&rgb);
    av_packet_free(&packet);
    avcodec_free_context(&encoder);
    if (format != NULL) {
        format->pb = NULL;
        avformat_free_context(format);
    }
    owned_avio_close(&io);
    return success;
}

static int receive_verified_frames(AVCodecContext *decoder, AVFrame *decoded, AVFrame *rgb,
                                   struct SwsContext *to_rgb, int scratch_fd, uint8_t *expected,
                                   uint8_t *actual, uint8_t *sample_left, uint8_t *sample_right,
                                   const DeadpanConversionRequest *request,
                                   uint64_t frame_bytes, struct AVSHA *output_sha,
                                   uint32_t *frame_count, int64_t *first_pts, int64_t *last_pts) {
    for (;;) {
        int code = avcodec_receive_frame(decoder, decoded);
        if (code == AVERROR(EAGAIN) || code == AVERROR_EOF) {
            return 1;
        }
        if (code < 0) {
            return fail_ffmpeg("decode FFV1 verification frame", code);
        }
        if (!within_deadline()) {
            return 0;
        }
        uint32_t index = *frame_count;
        if (index >= request->output_frames ||
            decoded->best_effort_timestamp == AV_NOPTS_VALUE) {
            return fail("verification_failed", "FFV1 output has extra frame or missing PTS");
        }
        int64_t wanted_pts = output_pts(index, request);
        int64_t wanted_next = output_pts(index + 1, request);
        int64_t default_duration =
            ((int64_t)OUTPUT_TIME_BASE_DEN * request->output_rate_den) /
            request->output_rate_num;
        if (decoded->width != (int)request->width ||
            decoded->height != (int)request->height || decoded->format != AV_PIX_FMT_BGR0 ||
            !check_frame_tags(decoded) || decoded->best_effort_timestamp != wanted_pts ||
            wanted_next <= wanted_pts || decoded->duration != default_duration ||
            decoded->duration <= 0) {
            return fail("verification_failed",
                        "FFV1 frame %u changed pixels, tags, or nearest-ms timing", index);
        }
        if (av_frame_make_writable(rgb) < 0 ||
            sws_scale(to_rgb, (const uint8_t *const *)decoded->data, decoded->linesize, 0,
                      decoded->height, rgb->data, rgb->linesize) != decoded->height) {
            return fail("ffmpeg_failure", "convert verified FFV1 frame to RGB8");
        }
        packed_rgb(rgb, request->width, request->height, actual);
        if (!output_rgb_from_scratch(scratch_fd, index, request, frame_bytes,
                                     expected, sample_left, sample_right)) {
            return 0;
        }
        if (memcmp(expected, actual, (size_t)frame_bytes) != 0) {
            return fail("verification_failed", "FFV1 RGB bytes differ at frame %u", index);
        }
        av_sha_update(output_sha, actual, (size_t)frame_bytes);
        if (index == 0) {
            *first_pts = decoded->best_effort_timestamp;
        }
        *last_pts = decoded->best_effort_timestamp;
        (*frame_count)++;
        av_frame_unref(decoded);
    }
}

static int verify_output(int output_fd, int scratch_fd,
                         const DeadpanConversionRequest *request, uint64_t output_bytes,
                         uint64_t frame_bytes, DeadpanConversionReport *report,
                         struct AVSHA *output_sha) {
    OwnedAvio io = {0};
    AVFormatContext *format = NULL;
    AVCodecContext *decoder = NULL;
    const AVCodec *codec = NULL;
    AVPacket *packet = NULL;
    AVFrame *decoded = NULL;
    AVFrame *rgb = NULL;
    struct SwsContext *to_rgb = NULL;
    uint8_t *expected = NULL;
    uint8_t *actual = NULL;
    uint8_t *sample_left = NULL;
    uint8_t *sample_right = NULL;
    uint32_t frame_count = 0;
    int success = 0;

    state.observed_ffv1_version = -1;
    state.observed_ffv1_ec = -1;
    if (!open_input_format(&format, &io, output_fd, (int64_t)output_bytes, "matroska", "ffv1",
                           AV_CODEC_ID_FFV1, request, 0)) {
        goto cleanup;
    }
    if (format->iformat == NULL || strncmp(format->iformat->name, "matroska", 8) != 0 ||
        format->nb_streams != 1 ||
        format->streams[0]->codecpar->codec_type != AVMEDIA_TYPE_VIDEO) {
        fail("verification_failed", "output is not single-video-stream Matroska");
        goto cleanup;
    }
    AVStream *stream = format->streams[0];
    AVRational requested_rate = {(int)request->output_rate_num,
                                 (int)request->output_rate_den};
    if (stream->codecpar->codec_id != AV_CODEC_ID_FFV1 ||
        stream->codecpar->format != AV_PIX_FMT_BGR0 ||
        stream->codecpar->width != (int)request->width ||
        stream->codecpar->height != (int)request->height ||
        stream->codecpar->color_range != AVCOL_RANGE_JPEG ||
        stream->codecpar->color_space != AVCOL_SPC_RGB ||
        stream->codecpar->color_trc != AVCOL_TRC_IEC61966_2_1 ||
        stream->codecpar->color_primaries != AVCOL_PRI_BT709 || stream->time_base.num <= 0 ||
        stream->time_base.den <= 0 || stream->time_base.num != OUTPUT_TIME_BASE_NUM ||
        stream->time_base.den != OUTPUT_TIME_BASE_DEN ||
        av_cmp_q(stream->avg_frame_rate, requested_rate) != 0) {
        fail("verification_failed", "FFV1 output stream metadata changed");
        goto cleanup;
    }
    codec = avcodec_find_decoder(AV_CODEC_ID_FFV1);
    decoder = codec == NULL ? NULL : avcodec_alloc_context3(codec);
    if (decoder == NULL) {
        fail("resource_exhausted", "allocate FFV1 verification decoder");
        goto cleanup;
    }
    int code = avcodec_parameters_to_context(decoder, stream->codecpar);
    if (code < 0) {
        fail_ffmpeg("copy FFV1 verification parameters", code);
        goto cleanup;
    }
    decoder->thread_count = 1;
    decoder->err_recognition = AV_EF_EXPLODE;
    decoder->debug |= FF_DEBUG_PICT_INFO;
    decoder->max_pixels = decoder_pixel_bound(request);
    code = avcodec_open2(decoder, codec, NULL);
    if (code < 0) {
        fail_ffmpeg("open FFV1 verification decoder", code);
        goto cleanup;
    }
    if ((decoder->level != AV_LEVEL_UNKNOWN && decoder->level != 3) ||
        decoder->width != (int)request->width || decoder->height != (int)request->height ||
        decoder->pix_fmt != AV_PIX_FMT_BGR0) {
        fail("verification_failed", "FFV1 decoder did not retain version 3 BGR0 contract");
        goto cleanup;
    }
    packet = av_packet_alloc();
    decoded = av_frame_alloc();
    rgb = av_frame_alloc();
    expected = av_malloc((size_t)frame_bytes);
    actual = av_malloc((size_t)frame_bytes);
    if (request->sample_bridge) {
        sample_left = av_malloc((size_t)frame_bytes);
        sample_right = av_malloc((size_t)frame_bytes);
    }
    if (packet == NULL || decoded == NULL || rgb == NULL || expected == NULL || actual == NULL) {
        fail("resource_exhausted", "allocate bounded FFV1 verification buffers");
        goto cleanup;
    }
    if (request->sample_bridge && (sample_left == NULL || sample_right == NULL)) {
        fail("resource_exhausted", "allocate bounded bridge verification buffers");
        goto cleanup;
    }
    rgb->format = AV_PIX_FMT_RGB24;
    rgb->width = (int)request->width;
    rgb->height = (int)request->height;
    if (av_frame_get_buffer(rgb, 32) < 0) {
        fail("resource_exhausted", "allocate FFV1 verification RGB frame");
        goto cleanup;
    }
    to_rgb = sws_getContext((int)request->width, (int)request->height, AV_PIX_FMT_BGR0,
                            (int)request->width, (int)request->height, AV_PIX_FMT_RGB24,
                            SWS_POINT, NULL, NULL, NULL);
    if (to_rgb == NULL) {
        fail("resource_exhausted", "create FFV1 verification RGB converter");
        goto cleanup;
    }
    for (;;) {
        if (!within_deadline()) {
            goto cleanup;
        }
        code = av_read_frame(format, packet);
        if (code == AVERROR_EOF) {
            code = avcodec_send_packet(decoder, NULL);
            if (code < 0 && code != AVERROR_EOF) {
                fail_ffmpeg("flush FFV1 verification decoder", code);
                goto cleanup;
            }
            if (!receive_verified_frames(decoder, decoded, rgb, to_rgb, scratch_fd, expected,
                                         actual, sample_left, sample_right, request, frame_bytes,
                                         output_sha, &frame_count, &report->first_output_pts,
                                         &report->last_output_pts)) {
                goto cleanup;
            }
            break;
        }
        if (code < 0) {
            fail_ffmpeg("read FFV1 verification packet", code);
            goto cleanup;
        }
        code = avcodec_send_packet(decoder, packet);
        av_packet_unref(packet);
        if (code < 0) {
            fail_ffmpeg("send FFV1 verification packet", code);
            goto cleanup;
        }
        if (!receive_verified_frames(decoder, decoded, rgb, to_rgb, scratch_fd, expected, actual,
                                     sample_left, sample_right, request, frame_bytes, output_sha,
                                     &frame_count, &report->first_output_pts,
                                     &report->last_output_pts)) {
            goto cleanup;
        }
    }
    if (frame_count != request->output_frames || state.observed_ffv1_version != 3 ||
        state.observed_ffv1_ec != 1) {
        fail("verification_failed",
             "FFV1 output frame count, version, or slice CRC does not match the contract");
        goto cleanup;
    }
    report->output_time_base_num = (uint32_t)stream->time_base.num;
    report->output_time_base_den = (uint32_t)stream->time_base.den;
    report->ffv1_version = 3;
    report->slice_crc = 1;
    success = 1;

cleanup:
    sws_freeContext(to_rgb);
    av_free(sample_right);
    av_free(sample_left);
    av_free(actual);
    av_free(expected);
    av_frame_free(&rgb);
    av_frame_free(&decoded);
    av_packet_free(&packet);
    avcodec_free_context(&decoder);
    close_input_format(&format, &io);
    return success;
}

static void digest_hex(struct AVSHA *sha, char output[65]) {
    static const char digits[] = "0123456789abcdef";
    uint8_t digest[32];
    av_sha_final(sha, digest);
    for (size_t index = 0; index < sizeof(digest); index++) {
        output[index * 2] = digits[digest[index] >> 4];
        output[index * 2 + 1] = digits[digest[index] & 15];
    }
    output[64] = '\0';
}

static int same_file(const struct stat *left, const struct stat *right) {
    return left->st_dev == right->st_dev && left->st_ino == right->st_ino;
}

static int verify_runtime(void) {
    if (avcodec_version() != LIBAVCODEC_VERSION_INT ||
        avformat_version() != LIBAVFORMAT_VERSION_INT || avutil_version() != LIBAVUTIL_VERSION_INT ||
        swscale_version() != LIBSWSCALE_VERSION_INT) {
        return fail("runtime_mismatch", "loaded FFmpeg libraries differ from pinned 8.0.3 headers");
    }
    const char *required[] = {"--disable-gpl", "--disable-nonfree", "--disable-version3",
                              "--disable-network"};
    const char *forbidden[] = {"--enable-gpl", "--enable-nonfree", "--enable-version3",
                              "--enable-network"};
    const char *configurations[] = {avcodec_configuration(), avformat_configuration(),
                                    avutil_configuration(), swscale_configuration()};
    const char *licenses[] = {avcodec_license(), avformat_license(), avutil_license(),
                              swscale_license()};
    for (size_t library = 0; library < sizeof(configurations) / sizeof(configurations[0]);
         library++) {
        if (strcmp(licenses[library], "LGPL version 2.1 or later") != 0) {
            return fail("runtime_mismatch", "loaded FFmpeg library is not LGPL 2.1+");
        }
        for (size_t index = 0; index < sizeof(required) / sizeof(required[0]); index++) {
            if (strstr(configurations[library], required[index]) == NULL ||
                strstr(configurations[library], forbidden[index]) != NULL) {
                return fail("runtime_mismatch", "loaded FFmpeg configuration violates %s",
                            required[index]);
            }
        }
    }
    return 1;
}

static int verify_descriptors(int input_fd, int output_fd, int scratch_fd,
                              const DeadpanConversionRequest *request, uint64_t *scratch_bytes) {
    struct stat input_status;
    struct stat output_status;
    struct stat scratch_status;
    uint64_t pixels;
    uint64_t frame_bytes;
    if (fstat(input_fd, &input_status) != 0 || fstat(output_fd, &output_status) != 0 ||
        fstat(scratch_fd, &scratch_status) != 0) {
        return fail("invalid_descriptor", "inspect conversion descriptors: %s", strerror(errno));
    }
    if (!S_ISREG(input_status.st_mode) || !S_ISREG(output_status.st_mode) ||
        !S_ISREG(scratch_status.st_mode) || input_status.st_size < 0 ||
        (uint64_t)input_status.st_size != request->input_byte_length ||
        request->input_byte_length > request->max_input_bytes || output_status.st_size != 0 ||
        scratch_status.st_size != 0 || output_status.st_uid != geteuid() ||
        scratch_status.st_uid != geteuid() || (output_status.st_mode & 077) != 0 ||
        (scratch_status.st_mode & 077) != 0 || same_file(&input_status, &output_status) ||
        same_file(&input_status, &scratch_status) || same_file(&output_status, &scratch_status)) {
        return fail("invalid_descriptor",
                    "input must be exact-length regular data and output/scratch distinct empty regular files");
    }
    if (!checked_mul_u64(request->width, request->height, &pixels) ||
        !checked_mul_u64(pixels, 3, &frame_bytes) ||
        !checked_mul_u64(frame_bytes, request->frames, scratch_bytes) ||
        *scratch_bytes > request->max_scratch_bytes || *scratch_bytes > (uint64_t)INT64_MAX ||
        request->max_output_bytes > (uint64_t)INT64_MAX ||
        request->input_byte_length > (uint64_t)INT64_MAX) {
        return fail("invalid_request", "conversion byte budget is invalid or insufficient");
    }
    return 1;
}

int deadpan_convert(int input_fd, int output_fd, int scratch_fd,
                    const DeadpanConversionRequest *request,
                    DeadpanConversionReport *report,
                    DeadpanConversionError *error) {
    DecodeResult decoded = {0};
    struct AVSHA *output_sha = NULL;
    uint64_t scratch_bytes = 0;
    uint64_t output_bytes = 0;
    uint64_t start;
    uint64_t timeout_ns;
    int success = 0;

    memset(report, 0, sizeof(*report));
    memset(error, 0, sizeof(*error));
    memset(&state, 0, sizeof(state));
    state.error = error;
    state.observed_ffv1_version = -1;
    state.observed_ffv1_ec = -1;
    if (!validate_request(request)) {
        return 1;
    }
    start = monotonic_ns();
    if (start == UINT64_MAX || !checked_mul_u64(request->timeout_ms, 1000000ULL, &timeout_ns) ||
        !checked_add_u64(start, timeout_ns, &state.deadline_ns)) {
        fail("invalid_request", "conversion deadline overflow");
        return 1;
    }
    av_log_set_level(AV_LOG_DEBUG);
    av_log_set_callback(worker_log);
    av_max_alloc(128U * 1024U * 1024U);
    if (!verify_runtime() ||
        !verify_descriptors(input_fd, output_fd, scratch_fd, request, &scratch_bytes) ||
        !decode_input(input_fd, scratch_fd, request, &decoded)) {
        goto cleanup;
    }
    if ((uint64_t)decoded.frame_count * decoded.frame_bytes != scratch_bytes ||
        ftruncate(scratch_fd, (off_t)scratch_bytes) != 0) {
        fail("io_failure", "finalize RGB scratch: %s", strerror(errno));
        goto cleanup;
    }
    if (!encode_output(output_fd, scratch_fd, request, decoded.frame_bytes, &output_bytes)) {
        goto cleanup;
    }
    output_sha = av_sha_alloc();
    if (output_sha == NULL || av_sha_init(output_sha, 256) < 0) {
        fail("resource_exhausted", "allocate output RGB digest");
        goto cleanup;
    }
    report->output_bytes = output_bytes;
    report->input_time_base_num = (uint32_t)decoded.input_time_base.num;
    report->input_time_base_den = (uint32_t)decoded.input_time_base.den;
    report->discarded_audio_streams = decoded.discarded_audio_streams;
    if (!verify_output(output_fd, scratch_fd, request, output_bytes, decoded.frame_bytes, report,
                       output_sha)) {
        goto cleanup;
    }
    digest_hex(decoded.input_sha, report->input_rgb_sha256);
    digest_hex(output_sha, report->output_rgb_sha256);
    if (!request->sample_bridge &&
        strcmp(report->input_rgb_sha256, report->output_rgb_sha256) != 0) {
        fail("verification_failed", "decoded FFV1 RGB digest differs from input RGB digest");
        goto cleanup;
    }
    success = 1;

cleanup:
    av_free(output_sha);
    av_free(decoded.input_sha);
    if (!success && error->code[0] == '\0') {
        fail("internal_error", "conversion failed without a classified error");
    }
    return success ? 0 : 1;
}
