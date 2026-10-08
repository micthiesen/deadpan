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
    /* Absolute descriptor offset of logical byte zero; one file may carry
       several concatenated inputs. */
    int64_t base;
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

/* A failed write's class: a full volume or exhausted quota is reported as
   `disk_full`, so the host never needs to parse the message. */
static const char *io_code(int error_number) {
    return error_number == ENOSPC || error_number == EDQUOT ? "disk_full" : "io_failure";
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
    if (request->sampling_kind > DEADPAN_SAMPLING_EXTENSION_FROM_RIGHT ||
        (request->sampling_kind == DEADPAN_SAMPLING_COPY &&
         (request->frames != request->output_frames ||
          request->rate_num != request->output_rate_num ||
          request->rate_den != request->output_rate_den)) ||
        (request->sampling_kind == DEADPAN_SAMPLING_BRIDGE_INTERIOR && request->frames < 2)) {
        return fail("invalid_request", "sampling configuration is inconsistent");
    }
    if (request->sampling_kind >= DEADPAN_SAMPLING_EXTENSION_FROM_LEFT) {
        uint32_t generated = request->generated_frames;
        /* Positive context and generated intervals exactly partition the
           native movie. Provider-specific block counts belong to the planner.
           Check before any unsigned subtraction. */
        if (generated == 0 || generated >= request->frames) {
            return fail("invalid_request", "extension counts are inconsistent");
        }
        uint32_t context = request->frames - generated;
        uint32_t expected_start =
            request->sampling_kind == DEADPAN_SAMPLING_EXTENSION_FROM_LEFT ? context : 0;
        if (request->generated_start != expected_start) {
            return fail("invalid_request", "extension interval is inconsistent");
        }
    } else if (request->generated_start != 0 || request->generated_frames != 0) {
        return fail("invalid_request", "non-extension request contains an extension interval");
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
        count = pread(io->fd, buffer, amount, (off_t)(io->base + io->position));
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
        fail("output_too_large", "output exceeds its byte budget");
        return AVERROR(ENOSPC);
    }
    while (written < buffer_size) {
        ssize_t count = pwrite(io->fd, buffer + written, (size_t)(buffer_size - written),
                               (off_t)(io->base + io->position + written));
        if (count < 0 && errno == EINTR) {
            continue;
        }
        if (count <= 0) {
            int error_number = count < 0 ? errno : EIO;
            if (error_number == ENOSPC || error_number == EDQUOT) {
                fail("disk_full", "write output: %s", strerror(error_number));
            }
            return AVERROR(error_number);
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
            fail("output_too_large", "output seek exceeds its byte budget");
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
            return fail(count < 0 ? io_code(errno) : "io_failure", "write RGB scratch: %s",
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
    if (output_index >= request->output_frames) {
        return fail("invalid_request", "sample ordinal is outside output sequence");
    }
    if (request->sampling_kind == DEADPAN_SAMPLING_COPY) {
        uint64_t offset;
        return checked_mul_u64(output_index, frame_bytes, &offset) &&
               exact_read_at(scratch_fd, output, frame_bytes, offset);
    }
    uint64_t denominator;
    uint64_t numerator;
    uint64_t interval_start = 0;
    uint64_t interval_end = request->frames;
    if (request->sampling_kind == DEADPAN_SAMPLING_BRIDGE_INTERIOR) {
        denominator = (uint64_t)request->output_frames + 1;
        numerator = ((uint64_t)output_index + 1) * ((uint64_t)request->frames - 1);
    } else {
        /* Native validation bounds every count to 10000. These products fit
           u64 independently of Rust's map implementation. Clamp before adding
           S so neither interpolation fetch can reach a context handle. */
        denominator = 2ULL * request->output_frames;
        uint64_t center = (2ULL * output_index + 1) * request->generated_frames;
        uint64_t maximum = ((uint64_t)request->generated_frames - 1) * denominator;
        numerator = center > request->output_frames ? center - request->output_frames : 0;
        if (numerator > maximum) {
            numerator = maximum;
        }
        interval_start = request->generated_start;
        interval_end = interval_start + request->generated_frames;
        numerator += interval_start * denominator;
    }
    uint64_t lower = numerator / denominator;
    uint64_t remainder = numerator % denominator;
    uint64_t upper = lower + (remainder != 0);
    uint64_t left_offset;
    uint64_t right_offset;
    if (left == NULL || right == NULL || lower < interval_start || lower >= interval_end ||
        upper >= interval_end || interval_end > request->frames ||
        !checked_mul_u64(lower, frame_bytes, &left_offset) ||
        !exact_read_at(scratch_fd, left, frame_bytes, left_offset)) {
        return fail("invalid_request", "sample coordinate is outside its native interval");
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
    if (request->sampling_kind != DEADPAN_SAMPLING_COPY) {
        sample_left = av_malloc((size_t)frame_bytes);
        sample_right = av_malloc((size_t)frame_bytes);
    }
    if (packet == NULL || rgb == NULL || bgr0 == NULL || packed == NULL) {
        fail("resource_exhausted", "allocate bounded FFV1 conversion buffers");
        goto cleanup;
    }
    if (request->sampling_kind != DEADPAN_SAMPLING_COPY && (sample_left == NULL || sample_right == NULL)) {
        fail("resource_exhausted", "allocate bounded sampling buffers");
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
        fail(io_code(errno), "finalize bounded Matroska output: %s", strerror(errno));
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
                                   uint32_t *frame_count, int64_t *first_pts, int64_t *last_pts,
                                   int64_t *last_duration) {
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
        *last_duration = decoded->duration;
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
    if (request->sampling_kind != DEADPAN_SAMPLING_COPY) {
        sample_left = av_malloc((size_t)frame_bytes);
        sample_right = av_malloc((size_t)frame_bytes);
    }
    if (packet == NULL || decoded == NULL || rgb == NULL || expected == NULL || actual == NULL) {
        fail("resource_exhausted", "allocate bounded FFV1 verification buffers");
        goto cleanup;
    }
    if (request->sampling_kind != DEADPAN_SAMPLING_COPY && (sample_left == NULL || sample_right == NULL)) {
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
                                         &report->last_output_pts,
                                         &report->last_output_duration)) {
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
                                     &report->last_output_pts,
                                     &report->last_output_duration)) {
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
        fail(io_code(errno), "finalize RGB scratch: %s", strerror(errno));
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
    if (request->sampling_kind == DEADPAN_SAMPLING_COPY &&
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

/* ---- Stream-copy assembly of one separately delivered H.264 picture stream
   and one AAC sound stream into a progressive MP4. No decoder or encoder is
   opened; packets keep their exact timestamps in the muxer's time base. ---- */

#define REMUX_MAX_PACKETS 50000000ULL

typedef struct {
    AVFormatContext *format;
    OwnedAvio io;
    AVPacket *pending;
    int has_pending;
    int finished;
    int output_index;
    uint64_t packets;
    struct AVSHA *timing;
} RemuxInput;

static void remux_close(RemuxInput *input);

/* Digest of everything that defines a stream's demuxed timing and payload:
   timestamps and durations in the stream clock, key/discard flags, bytes and
   the skip-samples side data that carries priming and padding trims. */
static void remux_digest_packet(struct AVSHA *sha, const AVPacket *packet) {
    int64_t fields[4] = {packet->pts, packet->dts, packet->duration,
                         (int64_t)(packet->flags & (AV_PKT_FLAG_KEY | AV_PKT_FLAG_DISCARD))};
    int64_t size = packet->size;
    size_t side_size = 0;
    const uint8_t *skip = av_packet_get_side_data(packet, AV_PKT_DATA_SKIP_SAMPLES, &side_size);
    av_sha_update(sha, (const uint8_t *)fields, sizeof(fields));
    av_sha_update(sha, (const uint8_t *)&size, sizeof(size));
    if (packet->size > 0) {
        av_sha_update(sha, packet->data, (size_t)packet->size);
    }
    av_sha_update(sha, (const uint8_t *)&side_size, sizeof(side_size));
    if (skip != NULL && side_size > 0) {
        av_sha_update(sha, skip, side_size);
    }
}

static void remux_digest_stream(struct AVSHA *sha, const AVStream *stream) {
    const AVCodecParameters *parameters = stream->codecpar;
    /* Start time is derived from the packets, which are digested exactly. */
    int64_t fields[6] = {stream->time_base.num, stream->time_base.den,
                         parameters->initial_padding, parameters->trailing_padding,
                         parameters->seek_preroll, parameters->extradata_size};
    av_sha_update(sha, (const uint8_t *)fields, sizeof(fields));
    if (parameters->extradata_size > 0) {
        av_sha_update(sha, parameters->extradata, (size_t)parameters->extradata_size);
    }
}

static int remux_new_digest(struct AVSHA **sha) {
    *sha = av_sha_alloc();
    if (*sha == NULL || av_sha_init(*sha, 256) < 0) {
        return fail("resource_exhausted", "allocate remux timing digest");
    }
    return 1;
}

/* Re-demux the written MP4 and require every stream to match its input. */
static int remux_verify(int output_fd, int64_t length, RemuxInput inputs[2]) {
    RemuxInput check = {0};
    struct AVSHA *digests[2] = {NULL, NULL};
    uint8_t expected[32];
    uint8_t actual[32];
    int verified = 0;
    if (!owned_avio_open(&check.io, output_fd, length, length, 0)) {
        return 0;
    }
    check.format = avformat_alloc_context();
    if (check.format == NULL) {
        fail("resource_exhausted", "allocate remux verification context");
        goto done;
    }
    check.format->pb = check.io.avio;
    check.format->flags |= AVFMT_FLAG_CUSTOM_IO;
    check.format->max_streams = MAX_STREAMS;
    check.format->interrupt_callback.callback = deadline_interrupt;
    check.format->interrupt_callback.opaque = &state;
    check.format->io_open = deny_external_io;
    check.format->protocol_whitelist = av_strdup("");
    check.format->format_whitelist = av_strdup("mov,mp4,m4a,3gp,3g2,mj2");
    if (check.format->protocol_whitelist == NULL || check.format->format_whitelist == NULL) {
        fail("resource_exhausted", "allocate FFmpeg verification allowlists");
        goto done;
    }
    int code = avformat_open_input(&check.format, NULL, av_find_input_format("mov"), NULL);
    if (code < 0) {
        fail_ffmpeg("reopen assembled MP4", code);
        goto done;
    }
    if (check.format->nb_streams != 2) {
        fail("verification_failed", "assembled MP4 does not have exactly two streams");
        goto done;
    }
    check.pending = av_packet_alloc();
    if (check.pending == NULL || !remux_new_digest(&digests[0]) ||
        !remux_new_digest(&digests[1])) {
        fail("resource_exhausted", "allocate remux verification state");
        goto done;
    }
    for (int index = 0; index < 2; index++) {
        remux_digest_stream(digests[index], check.format->streams[inputs[index].output_index]);
    }
    for (;;) {
        code = av_read_frame(check.format, check.pending);
        if (code == AVERROR_EOF) {
            break;
        }
        if (code < 0) {
            fail_ffmpeg("read assembled MP4 packet", code);
            goto done;
        }
        for (int index = 0; index < 2; index++) {
            if (check.pending->stream_index == inputs[index].output_index) {
                remux_digest_packet(digests[index], check.pending);
            }
        }
        av_packet_unref(check.pending);
        if (!within_deadline()) {
            goto done;
        }
    }
    for (int index = 0; index < 2; index++) {
        av_sha_final(inputs[index].timing, expected);
        av_sha_final(digests[index], actual);
        if (memcmp(expected, actual, sizeof(expected)) != 0) {
            fail("verification_failed",
                 "assembled %s stream differs from its input in timing, trims or bytes",
                 index == 0 ? "picture" : "sound");
            goto done;
        }
    }
    verified = 1;

done:
    av_free(digests[0]);
    av_free(digests[1]);
    remux_close(&check);
    return verified;
}

static int remux_validate(const DeadpanRemuxRequest *request) {
    uint64_t total;
    if (request->video_byte_length == 0 || request->audio_byte_length == 0 ||
        request->video_byte_length > MAX_FILE_BYTES || request->audio_byte_length > MAX_FILE_BYTES ||
        !checked_add_u64(request->video_byte_length, request->audio_byte_length, &total) ||
        request->max_output_bytes == 0 || request->max_output_bytes > 2 * MAX_FILE_BYTES ||
        request->timeout_ms == 0 || request->timeout_ms > 24ULL * 60ULL * 60ULL * 1000ULL) {
        return fail("invalid_request", "remux lengths, output budget or deadline are out of range");
    }
    return 1;
}

static int remux_open(RemuxInput *input, int fd, int64_t base, int64_t length,
                      enum AVMediaType type, enum AVCodecID codec_id, const char *codec_name) {
    const AVInputFormat *demuxer = av_find_input_format("mov");
    int result;
    if (demuxer == NULL) {
        return fail("runtime_mismatch", "required MP4 demuxer is unavailable");
    }
    if (!owned_avio_open(&input->io, fd, length, length, 0)) {
        return 0;
    }
    input->io.descriptor.base = base;
    input->format = avformat_alloc_context();
    if (input->format == NULL) {
        return fail("resource_exhausted", "allocate input format context");
    }
    input->format->pb = input->io.avio;
    input->format->flags |= AVFMT_FLAG_CUSTOM_IO;
    input->format->probesize = length < MAX_PROBE_BYTES ? length : MAX_PROBE_BYTES;
    input->format->max_streams = MAX_STREAMS;
    input->format->interrupt_callback.callback = deadline_interrupt;
    input->format->interrupt_callback.opaque = &state;
    input->format->io_open = deny_external_io;
    input->format->protocol_whitelist = av_strdup("");
    input->format->format_whitelist = av_strdup("mov,mp4,m4a,3gp,3g2,mj2");
    input->format->codec_whitelist = av_strdup(codec_name);
    if (input->format->protocol_whitelist == NULL || input->format->format_whitelist == NULL ||
        input->format->codec_whitelist == NULL) {
        return fail("resource_exhausted", "allocate FFmpeg input allowlists");
    }
    result = avformat_open_input(&input->format, NULL, demuxer, NULL);
    if (result < 0) {
        return fail_ffmpeg("open remux input descriptor", result);
    }
    if (input->format->nb_streams != 1) {
        return fail("invalid_media", "each remux input must contain exactly one stream");
    }
    const AVCodecParameters *parameters = input->format->streams[0]->codecpar;
    if (parameters->codec_type != type || parameters->codec_id != codec_id) {
        return fail("invalid_media", "remux input is not the required %s stream", codec_name);
    }
    if (type == AVMEDIA_TYPE_VIDEO &&
        (parameters->width <= 0 || parameters->height <= 0 ||
         parameters->width > 8192 || parameters->height > 8192)) {
        return fail("invalid_media", "remux picture dimensions are out of range");
    }
    if (parameters->extradata_size <= 0) {
        return fail("invalid_media", "remux %s stream lacks its decoder configuration",
                    codec_name);
    }
    input->pending = av_packet_alloc();
    if (input->pending == NULL) {
        return fail("resource_exhausted", "allocate remux packet");
    }
    if (!remux_new_digest(&input->timing)) {
        return 0;
    }
    remux_digest_stream(input->timing, input->format->streams[0]);
    return within_deadline();
}

static void remux_close(RemuxInput *input) {
    av_freep(&input->timing);
    av_packet_free(&input->pending);
    if (input->format != NULL) {
        input->format->pb = NULL;
        avformat_close_input(&input->format);
    }
    owned_avio_close(&input->io);
}

/* Keep one packet buffered per input so packets reach the muxer in
   decode-time order without unbounded interleaving queues. */
static int remux_fill(RemuxInput *input) {
    while (!input->has_pending && !input->finished) {
        int result = av_read_frame(input->format, input->pending);
        if (result == AVERROR_EOF) {
            input->finished = 1;
            break;
        }
        if (result < 0) {
            return fail_ffmpeg("read remux packet", result);
        }
        if (input->pending->stream_index != 0) {
            av_packet_unref(input->pending);
            continue;
        }
        if (input->pending->dts == AV_NOPTS_VALUE && input->pending->pts == AV_NOPTS_VALUE) {
            av_packet_unref(input->pending);
            return fail("invalid_media", "remux packet has no timestamp");
        }
        input->has_pending = 1;
    }
    return within_deadline();
}

static int64_t remux_order(const AVPacket *packet) {
    return packet->dts != AV_NOPTS_VALUE ? packet->dts : packet->pts;
}

int deadpan_remux(int input_fd, int output_fd, const DeadpanRemuxRequest *request,
                  DeadpanRemuxReport *report, DeadpanConversionError *error) {
    RemuxInput inputs[2];
    AVFormatContext *output = NULL;
    OwnedAvio output_io;
    struct stat metadata;
    uint64_t start;
    uint64_t timeout_ns;
    int header_written = 0;
    int success = 0;
    int code;

    memset(inputs, 0, sizeof(inputs));
    memset(&output_io, 0, sizeof(output_io));
    memset(report, 0, sizeof(*report));
    memset(error, 0, sizeof(*error));
    memset(&state, 0, sizeof(state));
    state.error = error;
    if (!remux_validate(request)) {
        return 1;
    }
    start = monotonic_ns();
    if (start == UINT64_MAX || !checked_mul_u64(request->timeout_ms, 1000000ULL, &timeout_ns) ||
        !checked_add_u64(start, timeout_ns, &state.deadline_ns)) {
        fail("invalid_request", "remux deadline overflow");
        return 1;
    }
    av_log_set_level(AV_LOG_ERROR);
    av_log_set_callback(worker_log);
    av_max_alloc(128U * 1024U * 1024U);
    if (!verify_runtime()) {
        goto cleanup;
    }
    if (fstat(input_fd, &metadata) != 0 || !S_ISREG(metadata.st_mode) ||
        (uint64_t)metadata.st_size != request->video_byte_length + request->audio_byte_length) {
        fail("invalid_request", "remux input descriptor does not hold exactly both inputs");
        goto cleanup;
    }
    if (fstat(output_fd, &metadata) != 0 || !S_ISREG(metadata.st_mode) || metadata.st_size != 0) {
        fail("invalid_request", "remux output must be an empty regular file");
        goto cleanup;
    }
    if (!remux_open(&inputs[0], input_fd, 0, (int64_t)request->video_byte_length,
                    AVMEDIA_TYPE_VIDEO, AV_CODEC_ID_H264, "h264") ||
        !remux_open(&inputs[1], input_fd, (int64_t)request->video_byte_length,
                    (int64_t)request->audio_byte_length, AVMEDIA_TYPE_AUDIO, AV_CODEC_ID_AAC,
                    "aac")) {
        goto cleanup;
    }
    code = avformat_alloc_output_context2(&output, NULL, "mp4", NULL);
    if (code < 0 || output == NULL) {
        fail_ffmpeg("create MP4 output", code < 0 ? code : AVERROR_UNKNOWN);
        goto cleanup;
    }
    if (!owned_avio_open(&output_io, output_fd, 0, (int64_t)request->max_output_bytes, 1)) {
        goto cleanup;
    }
    output->pb = output_io.avio;
    output->flags |= AVFMT_FLAG_CUSTOM_IO;
    output->interrupt_callback.callback = deadline_interrupt;
    output->interrupt_callback.opaque = &state;
    output->io_open = deny_external_io;
    output->protocol_whitelist = av_strdup("");
    if (output->protocol_whitelist == NULL) {
        fail("resource_exhausted", "allocate FFmpeg output protocol denylist");
        goto cleanup;
    }
    for (int index = 0; index < 2; index++) {
        const AVStream *source = inputs[index].format->streams[0];
        AVStream *stream = avformat_new_stream(output, NULL);
        if (stream == NULL) {
            fail("resource_exhausted", "allocate MP4 stream");
            goto cleanup;
        }
        code = avcodec_parameters_copy(stream->codecpar, source->codecpar);
        if (code < 0) {
            fail_ffmpeg("copy stream parameters", code);
            goto cleanup;
        }
        stream->codecpar->codec_tag = 0;
        stream->time_base = source->time_base;
        stream->avg_frame_rate = source->avg_frame_rate;
        stream->r_frame_rate = source->r_frame_rate;
        stream->sample_aspect_ratio = source->sample_aspect_ratio;
        stream->disposition = AV_DISPOSITION_DEFAULT;
        const AVDictionaryEntry *language = av_dict_get(source->metadata, "language", NULL, 0);
        if (language != NULL && av_dict_set(&stream->metadata, "language", language->value, 0) < 0) {
            fail("resource_exhausted", "copy stream language");
            goto cleanup;
        }
        inputs[index].output_index = stream->index;
    }
    {
        /* Express edit lists in the picture track's own clock. The default
           millisecond movie clock would round an initial composition delay
           (for example 1001/30000 s) and move every picture. */
        AVDictionary *options = NULL;
        int timescale = inputs[0].format->streams[0]->time_base.den;
        if (timescale <= 0 || av_dict_set_int(&options, "movie_timescale", timescale, 0) < 0) {
            av_dict_free(&options);
            fail("invalid_media", "picture time base cannot become the movie clock");
            goto cleanup;
        }
        code = avformat_write_header(output, &options);
        av_dict_free(&options);
    }
    if (code < 0) {
        fail_ffmpeg("write MP4 header", code);
        goto cleanup;
    }
    header_written = 1;
    for (;;) {
        RemuxInput *next = NULL;
        if (!remux_fill(&inputs[0]) || !remux_fill(&inputs[1])) {
            goto cleanup;
        }
        if (inputs[0].has_pending && inputs[1].has_pending) {
            next = av_compare_ts(remux_order(inputs[0].pending),
                                 inputs[0].format->streams[0]->time_base,
                                 remux_order(inputs[1].pending),
                                 inputs[1].format->streams[0]->time_base) <= 0
                       ? &inputs[0]
                       : &inputs[1];
        } else if (inputs[0].has_pending) {
            next = &inputs[0];
        } else if (inputs[1].has_pending) {
            next = &inputs[1];
        } else {
            break;
        }
        if (report->video_packets + report->audio_packets >= REMUX_MAX_PACKETS) {
            fail("invalid_media", "remux inputs exceed the packet bound");
            goto cleanup;
        }
        AVPacket *packet = next->pending;
        remux_digest_packet(next->timing, packet);
        av_packet_rescale_ts(packet, next->format->streams[0]->time_base,
                             output->streams[next->output_index]->time_base);
        packet->stream_index = next->output_index;
        packet->pos = -1;
        next->has_pending = 0;
        if (next == &inputs[0]) {
            report->video_packets++;
        } else {
            report->audio_packets++;
        }
        code = av_interleaved_write_frame(output, packet);
        av_packet_unref(packet);
        if (code < 0) {
            fail_ffmpeg("write MP4 packet", code);
            goto cleanup;
        }
    }
    if (report->video_packets == 0 || report->audio_packets == 0) {
        fail("invalid_media", "remux input stream has no packets");
        goto cleanup;
    }
    code = av_write_trailer(output);
    header_written = 0;
    if (code < 0) {
        fail_ffmpeg("write MP4 trailer", code);
        goto cleanup;
    }
    avio_flush(output_io.avio);
    if (output_io.avio->error < 0) {
        fail_ffmpeg("flush MP4 output", output_io.avio->error);
        goto cleanup;
    }
    if (fsync(output_fd) != 0) {
        fail(io_code(errno), "synchronize MP4 output: %s", strerror(errno));
        goto cleanup;
    }
    if (!remux_verify(output_fd, output_io.descriptor.length, inputs)) {
        goto cleanup;
    }
    report->output_bytes = (uint64_t)output_io.descriptor.length;
    report->width = (uint32_t)inputs[0].format->streams[0]->codecpar->width;
    report->height = (uint32_t)inputs[0].format->streams[0]->codecpar->height;
    report->sample_rate = (uint32_t)inputs[1].format->streams[0]->codecpar->sample_rate;
    report->channels = (uint32_t)inputs[1].format->streams[0]->codecpar->ch_layout.nb_channels;
    success = within_deadline();

cleanup:
    if (header_written && output != NULL) {
        (void)av_write_trailer(output);
    }
    if (output != NULL) {
        output->pb = NULL;
        avformat_free_context(output);
    }
    owned_avio_close(&output_io);
    remux_close(&inputs[0]);
    remux_close(&inputs[1]);
    if (!success && error->code[0] == '\0') {
        fail("internal_error", "remux failed without a classified error");
    }
    return success ? 0 : 1;
}

/* ---------------------------------------------------------------------------
 * Preview proxy encoding.
 *
 * The Rust worker decodes the Original through the qualified deadpan-source
 * adapter and pushes owned straight RGBA pictures in presentation order. This
 * section scales each picture to the proxy raster, converts it to limited-range
 * BT.709 4:2:0 and encodes every picture as an IDR with VideoToolbox H.264 into
 * an MP4 on the output descriptor. Each packet carries exactly the Original
 * picture's timestamp and duration in the Original's own time base; there is
 * no reordering, so DTS equals PTS. The host independently decodes the result
 * and compares every picture's timing with the Original's measured index.
 * ------------------------------------------------------------------------- */

#include <libavcodec/bsf.h>

#define PROXY_MAX_DIMENSION 4096U
#define PROXY_MAX_SOURCE_DIMENSION 8192U
#define PROXY_MAX_FRAMES 10000000ULL
#define PROXY_TIMING_QUEUE 64U

typedef struct {
    OwnedAvio io;
    AVFormatContext *format;
    AVCodecContext *encoder;
    AVStream *stream;
    AVPacket *packet;
    AVFrame *picture;
    struct SwsContext *scaler;
    /* Writes the sample aspect ratio into the H.264 VUI: VideoToolbox does
       not, and decoders compare each picture's ratio with the stream's. */
    AVBSFContext *bsf;
    DeadpanProxyRequest request;
    int header_written;
    int abandoned;
    uint64_t pushed;
    uint64_t packets;
    uint64_t keyframes;
    int64_t last_pts;
    int64_t first_pts;
    int64_t end_pts;
    int64_t timing_pts[PROXY_TIMING_QUEUE];
    int64_t timing_duration[PROXY_TIMING_QUEUE];
    uint32_t timing_head;
    uint32_t timing_count;
} ProxyEncoder;

static ProxyEncoder proxy;

static int proxy_valid_color(uint32_t transfer, uint32_t primaries) {
    return (transfer == AVCOL_TRC_BT709 || transfer == AVCOL_TRC_IEC61966_2_1 ||
            transfer == AVCOL_TRC_LINEAR) &&
           (primaries == AVCOL_PRI_BT709 || primaries == AVCOL_PRI_BT2020 ||
            primaries == AVCOL_PRI_SMPTE432);
}

static int proxy_validate(const DeadpanProxyRequest *request) {
    if (request->source_width == 0 || request->source_height == 0 ||
        request->source_width > PROXY_MAX_SOURCE_DIMENSION ||
        request->source_height > PROXY_MAX_SOURCE_DIMENSION || request->width < 2 ||
        request->height < 2 || request->width > PROXY_MAX_DIMENSION ||
        request->height > PROXY_MAX_DIMENSION || (request->width & 1) || (request->height & 1) ||
        request->width > request->source_width + 1 || request->height > request->source_height + 1) {
        return fail("invalid_request", "proxy raster is outside worker bounds");
    }
    if (request->time_base_num == 0 || request->time_base_den == 0 ||
        request->time_base_num > INT_MAX || request->time_base_den > INT_MAX ||
        request->sar_num == 0 || request->sar_den == 0 || request->sar_num > INT_MAX ||
        request->sar_den > INT_MAX || request->rotation_quarter_turns > 3 ||
        !proxy_valid_color(request->transfer, request->primaries)) {
        return fail("invalid_request", "proxy clock, aspect, rotation or color is invalid");
    }
    if (request->frames == 0 || request->frames > PROXY_MAX_FRAMES ||
        request->max_output_bytes == 0 || request->max_output_bytes > (uint64_t)INT64_MAX ||
        request->timeout_ms == 0 || request->timeout_ms > 24ULL * 60ULL * 60ULL * 1000ULL ||
        request->quality == 0 || request->quality > 100 ||
        request->output_offset > (uint64_t)INT64_MAX - request->max_output_bytes) {
        return fail("invalid_request", "proxy frame, byte, time or quality budget is invalid");
    }
    return 1;
}

/* After a failure, or once the output is complete, the worker reports and
   exits without tearing VideoToolbox down. Under heavy load a failed
   compression session has blocked indefinitely in
   VTCompressionSessionCompleteFrames or VTCompressionSessionInvalidate
   (inside avcodec_free_context), which would hide the classified error behind
   the host's stall watch. The process exit releases the session. */
/* Replace any display matrix with exactly the one the source decoder maps
   to these quarter turns (deadpan-source decoder.c, rotation()); none for
   an upright picture. */
static int proxy_set_rotation(AVCodecParameters *parameters, uint32_t quarter_turns) {
    av_packet_side_data_remove(parameters->coded_side_data, &parameters->nb_coded_side_data,
                               AV_PKT_DATA_DISPLAYMATRIX);
    if (quarter_turns == 0) {
        return 1;
    }
    AVPacketSideData *side = av_packet_side_data_new(
        &parameters->coded_side_data, &parameters->nb_coded_side_data, AV_PKT_DATA_DISPLAYMATRIX,
        sizeof(int32_t) * 9, 0);
    if (side == NULL) {
        return fail("resource_exhausted", "allocate proxy display matrix");
    }
    static const int32_t linear[4][4] = {
        {65536, 0, 0, 65536}, {0, 65536, -65536, 0}, {-65536, 0, 0, -65536}, {0, -65536, 65536, 0}};
    const int32_t *turn = linear[quarter_turns & 3U];
    int32_t matrix[9] = {turn[0], turn[1], 0, turn[2], turn[3], 0, 0, 0, 1 << 30};
    memcpy(side->data, matrix, sizeof(matrix));
    return 1;
}

/* Exact equality of two times in different rational clocks. */
static int proxy_same_time(int64_t a, AVRational a_base, int64_t b, AVRational b_base) {
    return (__int128)a * a_base.num * b_base.den == (__int128)b * b_base.num * a_base.den;
}

/* `value` in `from` units as an exact count of `to` units. */
static int proxy_exact(int64_t value, AVRational from, AVRational to, int64_t *result) {
    __int128 numerator = (__int128)value * from.num * to.den;
    __int128 denominator = (__int128)from.den * to.num;
    if (denominator == 0 || numerator % denominator != 0) {
        return 0;
    }
    __int128 quotient = numerator / denominator;
    if (quotient > INT64_MAX || quotient < INT64_MIN) {
        return 0;
    }
    *result = (int64_t)quotient;
    return 1;
}

static void proxy_digest_extradata(const AVCodecParameters *parameters, uint8_t digest[32]) {
    struct AVSHA *sha = av_sha_alloc();
    memset(digest, 0, 32);
    if (sha == NULL || av_sha_init(sha, 256) < 0) {
        av_free(sha);
        return;
    }
    if (parameters->extradata_size > 0) {
        av_sha_update(sha, parameters->extradata, (size_t)parameters->extradata_size);
    }
    av_sha_final(sha, digest);
    av_free(sha);
}

/* Open one proxy movie at [base, base + length) of `fd`: exactly one H.264
   picture stream of the planned raster with its decoder configuration. */
static int proxy_movie_open(RemuxInput *input, int fd, int64_t base, int64_t length,
                            uint32_t width, uint32_t height) {
    if (!remux_open(input, fd, base, length, AVMEDIA_TYPE_VIDEO, AV_CODEC_ID_H264, "h264")) {
        return 0;
    }
    const AVCodecParameters *parameters = input->format->streams[0]->codecpar;
    if (parameters->width != (int)width || parameters->height != (int)height) {
        return fail("invalid_media", "proxy range raster differs from the plan");
    }
    return 1;
}

/* Read every packet of an opened proxy movie. Each must be an intra picture
   with a positive duration, starting at `start` and following its
   predecessor without gap or overlap, all exactly in `clock`; the last must
   end at `end` after `frames` pictures. With an output each packet is
   written to `stream` with DTS equal to its PTS. */
static int proxy_movie_read(RemuxInput *input, AVRational clock, int64_t start, int64_t end,
                            uint64_t frames, AVFormatContext *output, AVStream *stream,
                            uint64_t *written) {
    AVRational base = input->format->streams[0]->time_base;
    int64_t expected = start;
    uint64_t count = 0;
    for (;;) {
        int code = av_read_frame(input->format, input->pending);
        if (code == AVERROR_EOF) {
            break;
        }
        if (code < 0) {
            return fail_ffmpeg("read proxy range packet", code);
        }
        AVPacket *packet = input->pending;
        int64_t duration;
        if (packet->stream_index != 0 || packet->pts == AV_NOPTS_VALUE ||
            !(packet->flags & AV_PKT_FLAG_KEY) || packet->duration <= 0 ||
            count >= frames || !proxy_same_time(packet->pts, base, expected, clock) ||
            !proxy_exact(packet->duration, base, clock, &duration) ||
            expected > INT64_MAX - duration) {
            int64_t pts = packet->pts;
            av_packet_unref(packet);
            return fail("invalid_media",
                        "proxy range picture %" PRIu64 " is not an intra picture at its time "
                        "(pts %" PRId64 ", expected %" PRId64 ")",
                        count, pts, expected);
        }
        expected += duration;
        count++;
        if (output != NULL) {
            packet->dts = packet->pts;
            av_packet_rescale_ts(packet, base, stream->time_base);
            packet->stream_index = stream->index;
            packet->pos = -1;
            code = av_interleaved_write_frame(output, packet);
            if (code < 0) {
                av_packet_unref(packet);
                return fail_ffmpeg("write assembled proxy packet", code);
            }
            (*written)++;
        }
        av_packet_unref(packet);
        if (!within_deadline()) {
            return 0;
        }
    }
    if (count != frames || expected != end) {
        return fail("invalid_media",
                    "proxy range holds %" PRIu64 " of %" PRIu64 " pictures or ends early",
                    count, frames);
    }
    return 1;
}

static void proxy_abandon(void) {
    proxy.abandoned = 1;
}

static int proxy_encoder_failure(const char *operation, int error) {
    char detail[AV_ERROR_MAX_STRING_SIZE];
    if (av_strerror(error, detail, sizeof(detail)) < 0) {
        (void)snprintf(detail, sizeof(detail), "FFmpeg error %d", error);
    }
    return fail("encoder_session_failed", "%s: %s%s%s", operation, detail,
                state.ffmpeg_diagnostic[0] ? "; " : "", state.ffmpeg_diagnostic);
}

static void proxy_release(void) {
    if (proxy.header_written && proxy.format != NULL) {
        (void)av_write_trailer(proxy.format);
    }
    av_bsf_free(&proxy.bsf);
    sws_freeContext(proxy.scaler);
    av_frame_free(&proxy.picture);
    av_packet_free(&proxy.packet);
    avcodec_free_context(&proxy.encoder);
    if (proxy.format != NULL) {
        proxy.format->pb = NULL;
        avformat_free_context(proxy.format);
    }
    owned_avio_close(&proxy.io);
    memset(&proxy, 0, sizeof(proxy));
}

static int proxy_write_packets(void) {
    for (;;) {
        int code = avcodec_receive_packet(proxy.encoder, proxy.packet);
        if (code == AVERROR(EAGAIN) || code == AVERROR_EOF) {
            return 1;
        }
        if (code < 0) {
            return proxy_encoder_failure("receive proxy packet", code);
        }
        if (proxy.timing_count == 0) {
            av_packet_unref(proxy.packet);
            return fail("invalid_packet", "proxy encoder emitted an unrequested picture");
        }
        int64_t pts = proxy.timing_pts[proxy.timing_head];
        int64_t duration = proxy.timing_duration[proxy.timing_head];
        proxy.timing_head = (proxy.timing_head + 1) % PROXY_TIMING_QUEUE;
        proxy.timing_count--;
        /* Intra-only without reordering: every packet is its own picture in
           presentation order, so its exact clock is the queued one. */
        if (proxy.packet->pts != pts ||
            (proxy.packet->dts != AV_NOPTS_VALUE && proxy.packet->dts > pts) ||
            !(proxy.packet->flags & AV_PKT_FLAG_KEY)) {
            int64_t actual_pts = proxy.packet->pts;
            int64_t actual_dts = proxy.packet->dts;
            int key = (proxy.packet->flags & AV_PKT_FLAG_KEY) != 0;
            av_packet_unref(proxy.packet);
            return fail("invalid_packet",
                        "proxy packet %" PRIu64 " is not an in-order intra picture: pts %" PRId64
                        " dts %" PRId64 " key %d, expected pts %" PRId64,
                        proxy.packets, actual_pts, actual_dts, key, pts);
        }
        proxy.packet->dts = pts;
        proxy.packet->duration = duration;
        code = av_bsf_send_packet(proxy.bsf, proxy.packet);
        av_packet_unref(proxy.packet);
        if (code < 0) {
            return fail_ffmpeg("rewrite proxy packet", code);
        }
        for (;;) {
            code = av_bsf_receive_packet(proxy.bsf, proxy.packet);
            if (code == AVERROR(EAGAIN)) {
                break;
            }
            if (code < 0) {
                return fail_ffmpeg("receive rewritten proxy packet", code);
            }
            if (proxy.packet->pts != pts || proxy.packet->duration != duration) {
                av_packet_unref(proxy.packet);
                return fail("invalid_packet", "proxy packet timing changed while rewriting");
            }
            proxy.packet->stream_index = proxy.stream->index;
            proxy.packet->pos = -1;
            av_packet_rescale_ts(proxy.packet, proxy.encoder->time_base,
                                 proxy.stream->time_base);
            proxy.packets++;
            proxy.keyframes++;
            code = av_interleaved_write_frame(proxy.format, proxy.packet);
            av_packet_unref(proxy.packet);
            if (code < 0) {
                return fail_ffmpeg("write proxy packet", code);
            }
        }
        if (!within_deadline()) {
            return 0;
        }
    }
}

int deadpan_proxy_open(int output_fd, const DeadpanProxyRequest *request,
                       DeadpanConversionError *error) {
    struct stat metadata;
    uint64_t start;
    uint64_t timeout_ns;
    const AVCodec *codec;
    AVDictionary *options = NULL;
    int code;

    proxy_release();
    memset(error, 0, sizeof(*error));
    memset(&state, 0, sizeof(state));
    state.error = error;
    if (!proxy_validate(request)) {
        return 1;
    }
    proxy.request = *request;
    start = monotonic_ns();
    if (start == UINT64_MAX || !checked_mul_u64(request->timeout_ms, 1000000ULL, &timeout_ns) ||
        !checked_add_u64(start, timeout_ns, &state.deadline_ns)) {
        fail("invalid_request", "proxy deadline overflow");
        return 1;
    }
    av_log_set_level(AV_LOG_ERROR);
    av_log_set_callback(worker_log);
    /* No process-wide av_max_alloc here: the qualified source decoder in this
       process is already open with its own bounds. */
    if (!verify_runtime()) {
        goto failed;
    }
    if (fstat(output_fd, &metadata) != 0 || !S_ISREG(metadata.st_mode) ||
        (uint64_t)metadata.st_size != request->output_offset || metadata.st_uid != geteuid() ||
        (metadata.st_mode & 077) != 0) {
        fail("invalid_descriptor",
             "proxy output must be a private regular file ending at the output offset");
        goto failed;
    }
    code = avformat_alloc_output_context2(&proxy.format, NULL, "mp4", NULL);
    if (code < 0 || proxy.format == NULL) {
        fail_ffmpeg("create proxy MP4 output", code < 0 ? code : AVERROR_UNKNOWN);
        goto failed;
    }
    if (!owned_avio_open(&proxy.io, output_fd, 0, (int64_t)request->max_output_bytes, 1)) {
        goto failed;
    }
    proxy.io.descriptor.base = (int64_t)request->output_offset;
    proxy.format->pb = proxy.io.avio;
    proxy.format->flags |= AVFMT_FLAG_CUSTOM_IO;
    proxy.format->interrupt_callback.callback = deadline_interrupt;
    proxy.format->interrupt_callback.opaque = &state;
    proxy.format->io_open = deny_external_io;
    proxy.format->protocol_whitelist = av_strdup("");
    if (proxy.format->protocol_whitelist == NULL) {
        fail("resource_exhausted", "allocate FFmpeg output protocol denylist");
        goto failed;
    }
    codec = avcodec_find_encoder_by_name("h264_videotoolbox");
    if (codec == NULL) {
        fail("video_encoder_unavailable", "VideoToolbox H.264 encoder is unavailable");
        goto failed;
    }
    proxy.encoder = avcodec_alloc_context3(codec);
    proxy.stream = avformat_new_stream(proxy.format, NULL);
    proxy.packet = av_packet_alloc();
    proxy.picture = av_frame_alloc();
    if (proxy.encoder == NULL || proxy.stream == NULL || proxy.packet == NULL ||
        proxy.picture == NULL) {
        fail("resource_exhausted", "allocate proxy encoder");
        goto failed;
    }
    proxy.encoder->width = (int)request->width;
    proxy.encoder->height = (int)request->height;
    proxy.encoder->pix_fmt = AV_PIX_FMT_NV12;
    proxy.encoder->time_base = (AVRational){(int)request->time_base_num,
                                            (int)request->time_base_den};
    proxy.encoder->sample_aspect_ratio = (AVRational){(int)request->sar_num,
                                                      (int)request->sar_den};
    proxy.encoder->gop_size = 1;
    proxy.encoder->max_b_frames = 0;
    proxy.encoder->color_range = AVCOL_RANGE_MPEG;
    proxy.encoder->colorspace = AVCOL_SPC_BT709;
    proxy.encoder->color_trc = (enum AVColorTransferCharacteristic)request->transfer;
    proxy.encoder->color_primaries = (enum AVColorPrimaries)request->primaries;
    proxy.encoder->chroma_sample_location = AVCHROMA_LOC_LEFT;
    proxy.encoder->flags |= AV_CODEC_FLAG_QSCALE;
    proxy.encoder->global_quality = (int)request->quality * FF_QP2LAMBDA;
    proxy.encoder->profile = AV_PROFILE_H264_HIGH;
    if (av_dict_set(&options, "allow_sw", "1", 0) < 0 ||
        av_dict_set(&options, "realtime", "0", 0) < 0) {
        av_dict_free(&options);
        fail("resource_exhausted", "configure proxy encoder");
        goto failed;
    }
    code = avcodec_open2(proxy.encoder, codec, &options);
    av_dict_free(&options);
    if (code < 0) {
        /* Under load VideoToolbox refuses new sessions (-12900 setting
           properties); a later attempt can succeed. */
        proxy_encoder_failure("open VideoToolbox H.264 proxy encoder", code);
        goto failed;
    }
    if (proxy.encoder->max_b_frames != 0 || proxy.encoder->has_b_frames != 0) {
        fail("encoder_unsupported", "proxy encoder enabled picture reordering");
        goto failed;
    }
    proxy.stream->time_base = proxy.encoder->time_base;
    proxy.stream->sample_aspect_ratio = proxy.encoder->sample_aspect_ratio;
    {
        const AVBitStreamFilter *filter = av_bsf_get_by_name("h264_metadata");
        char ratio[32];
        if (filter == NULL) {
            fail("encoder_unsupported", "the h264_metadata bitstream filter is unavailable");
            goto failed;
        }
        code = av_bsf_alloc(filter, &proxy.bsf);
        if (code < 0) {
            fail_ffmpeg("allocate proxy bitstream filter", code);
            goto failed;
        }
        (void)snprintf(ratio, sizeof(ratio), "%u/%u", request->sar_num, request->sar_den);
        code = avcodec_parameters_from_context(proxy.bsf->par_in, proxy.encoder);
        if (code >= 0) {
            proxy.bsf->time_base_in = proxy.encoder->time_base;
            code = av_opt_set(proxy.bsf->priv_data, "sample_aspect_ratio", ratio, 0);
        }
        if (code >= 0) {
            code = av_bsf_init(proxy.bsf);
        }
        if (code < 0) {
            fail_ffmpeg("configure proxy bitstream filter", code);
            goto failed;
        }
    }
    code = avcodec_parameters_copy(proxy.stream->codecpar, proxy.bsf->par_out);
    if (code < 0) {
        fail_ffmpeg("copy proxy stream parameters", code);
        goto failed;
    }
    proxy.stream->codecpar->sample_aspect_ratio = proxy.encoder->sample_aspect_ratio;
    if (!proxy_set_rotation(proxy.stream->codecpar, request->rotation_quarter_turns)) {
        goto failed;
    }
    {
        int timescale = (int)request->time_base_den;
        if (av_dict_set_int(&options, "video_track_timescale", timescale, 0) < 0 ||
            av_dict_set_int(&options, "movie_timescale", timescale, 0) < 0) {
            av_dict_free(&options);
            fail("resource_exhausted", "configure proxy MP4 clock");
            goto failed;
        }
        code = avformat_write_header(proxy.format, &options);
        av_dict_free(&options);
    }
    if (code < 0) {
        fail_ffmpeg("write proxy MP4 header", code);
        goto failed;
    }
    proxy.header_written = 1;
    proxy.picture->format = AV_PIX_FMT_NV12;
    proxy.picture->width = (int)request->width;
    proxy.picture->height = (int)request->height;
    proxy.picture->color_range = AVCOL_RANGE_MPEG;
    proxy.picture->colorspace = AVCOL_SPC_BT709;
    proxy.picture->color_trc = proxy.encoder->color_trc;
    proxy.picture->color_primaries = proxy.encoder->color_primaries;
    proxy.picture->chroma_location = AVCHROMA_LOC_LEFT;
    proxy.picture->sample_aspect_ratio = proxy.encoder->sample_aspect_ratio;
    if (av_frame_get_buffer(proxy.picture, 64) < 0) {
        fail("resource_exhausted", "allocate proxy picture");
        goto failed;
    }
    proxy.scaler = sws_getContext((int)request->source_width, (int)request->source_height,
                                  AV_PIX_FMT_RGBA, (int)request->width, (int)request->height,
                                  AV_PIX_FMT_NV12,
                                  SWS_AREA | SWS_ACCURATE_RND | SWS_FULL_CHR_H_INP, NULL, NULL,
                                  NULL);
    if (proxy.scaler == NULL) {
        fail("resource_exhausted", "create proxy scaler");
        goto failed;
    }
    {
        /* Input RGBA is full range; output is limited-range BT.709 Y'CbCr. */
        const int *coefficients = sws_getCoefficients(SWS_CS_ITU709);
        if (sws_setColorspaceDetails(proxy.scaler, coefficients, 1, coefficients, 0, 0,
                                     1 << 16, 1 << 16) < 0) {
            fail("ffmpeg_failure", "configure proxy color conversion");
            goto failed;
        }
    }
    proxy.last_pts = INT64_MIN;
    if (within_deadline()) {
        return 0;
    }

failed:
    proxy_abandon();
    if (error->code[0] == '\0') {
        fail("internal_error", "proxy open failed without a classified error");
    }
    return 1;
}

int deadpan_proxy_push(const uint8_t *rgba, uint64_t rgba_bytes, uint64_t stride, int64_t pts,
                       int64_t duration, DeadpanConversionError *error) {
    int code;
    state.error = error;
    if (proxy.encoder == NULL || proxy.abandoned) {
        fail("invalid_request", "proxy encoder is not open");
        return 1;
    }
    if (!within_deadline()) {
        goto failed;
    }
    if (proxy.pushed >= proxy.request.frames || duration <= 0 || pts <= proxy.last_pts ||
        pts > INT64_MAX - duration ||
        stride < (uint64_t)proxy.request.source_width * 4ULL ||
        stride > (uint64_t)INT_MAX ||
        rgba_bytes < stride * (uint64_t)proxy.request.source_height) {
        fail("input_order", "proxy picture is out of order, unbounded or has no duration");
        goto failed;
    }
    if (proxy.timing_count >= PROXY_TIMING_QUEUE) {
        fail("invalid_packet", "proxy encoder retained too many pictures");
        goto failed;
    }
    if (av_frame_make_writable(proxy.picture) < 0) {
        fail("resource_exhausted", "prepare writable proxy picture");
        goto failed;
    }
    {
        const uint8_t *source[4] = {rgba, NULL, NULL, NULL};
        const int source_stride[4] = {(int)stride, 0, 0, 0};
        if (sws_scale(proxy.scaler, source, source_stride, 0, (int)proxy.request.source_height,
                      proxy.picture->data, proxy.picture->linesize) != (int)proxy.request.height) {
            fail("ffmpeg_failure", "scale proxy picture");
            goto failed;
        }
    }
    proxy.picture->pts = pts;
    proxy.picture->duration = duration;
    proxy.picture->pict_type = AV_PICTURE_TYPE_I;
    proxy.picture->flags |= AV_FRAME_FLAG_KEY;
    uint32_t tail = (proxy.timing_head + proxy.timing_count) % PROXY_TIMING_QUEUE;
    proxy.timing_pts[tail] = pts;
    proxy.timing_duration[tail] = duration;
    proxy.timing_count++;
    if (proxy.pushed == 0) {
        proxy.first_pts = pts;
    }
    proxy.last_pts = pts;
    proxy.end_pts = pts + duration;
    proxy.pushed++;
    code = avcodec_send_frame(proxy.encoder, proxy.picture);
    if (code < 0) {
        proxy_encoder_failure("send proxy picture", code);
        goto failed;
    }
    if (!proxy_write_packets()) {
        goto failed;
    }
    return 0;

failed:
    proxy_abandon();
    return 1;
}

int deadpan_proxy_finish(int output_fd, DeadpanProxyReport *report,
                         DeadpanConversionError *error) {
    int code;
    state.error = error;
    memset(report, 0, sizeof(*report));
    if (proxy.encoder == NULL || proxy.abandoned) {
        fail("invalid_request", "proxy encoder is not open");
        return 1;
    }
    if (proxy.pushed != proxy.request.frames) {
        fail("incomplete_input", "proxy received %" PRIu64 " of %" PRIu64 " pictures",
             proxy.pushed, proxy.request.frames);
        goto failed;
    }
    code = avcodec_send_frame(proxy.encoder, NULL);
    if (code < 0) {
        proxy_encoder_failure("flush proxy encoder", code);
        goto failed;
    }
    if (!proxy_write_packets()) {
        goto failed;
    }
    /* Every packet was already drained through the one-in, one-out filter. */
    code = av_bsf_send_packet(proxy.bsf, NULL);
    if (code < 0) {
        fail_ffmpeg("flush proxy bitstream filter", code);
        goto failed;
    }
    code = av_bsf_receive_packet(proxy.bsf, proxy.packet);
    if (code != AVERROR_EOF) {
        av_packet_unref(proxy.packet);
        fail("invalid_packet", "proxy bitstream filter retained a packet");
        goto failed;
    }
    if (proxy.timing_count != 0 || proxy.packets != proxy.request.frames) {
        fail("incomplete_output", "proxy encoder emitted %" PRIu64 " of %" PRIu64 " pictures",
             proxy.packets, proxy.request.frames);
        goto failed;
    }
    code = av_write_trailer(proxy.format);
    proxy.header_written = 0;
    if (code < 0) {
        fail_ffmpeg("write proxy MP4 trailer", code);
        goto failed;
    }
    avio_flush(proxy.io.avio);
    if (proxy.io.avio->error < 0) {
        fail_ffmpeg("flush proxy MP4 output", proxy.io.avio->error);
        goto failed;
    }
    if (ftruncate(output_fd, (off_t)(proxy.io.descriptor.base + proxy.io.descriptor.length)) != 0 ||
        fsync(output_fd) != 0) {
        fail(io_code(errno), "finalize proxy output: %s", strerror(errno));
        goto failed;
    }
    {
        /* Read the finished movie back as the assembler and the verifier
           will: every picture intra at its exact time, and the decoder
           configuration a later join compares. */
        RemuxInput check;
        uint64_t unused = 0;
        AVRational clock = {(int)proxy.request.time_base_num, (int)proxy.request.time_base_den};
        memset(&check, 0, sizeof(check));
        int checked = proxy_movie_open(&check, output_fd, proxy.io.descriptor.base,
                                       proxy.io.descriptor.length, proxy.request.width,
                                       proxy.request.height) &&
                      proxy_movie_read(&check, clock, proxy.first_pts, proxy.end_pts,
                                       proxy.request.frames, NULL, NULL, &unused);
        if (checked) {
            proxy_digest_extradata(check.format->streams[0]->codecpar,
                                   report->extradata_sha256);
        }
        remux_close(&check);
        if (!checked) {
            goto failed;
        }
    }
    report->output_bytes = (uint64_t)proxy.io.descriptor.length;
    report->packets = proxy.packets;
    report->keyframes = proxy.keyframes;
    report->width = proxy.request.width;
    report->height = proxy.request.height;
    if (!within_deadline()) {
        goto failed;
    }
    /* The output is complete and synchronized; see proxy_abandon. */
    proxy_abandon();
    return 0;

failed:
    proxy_abandon();
    if (error->code[0] == '\0') {
        fail("internal_error", "proxy finish failed without a classified error");
    }
    return 1;
}

/* ---------------------------------------------------------------------------
 * Proxy assembly: join range movies at packet level without re-encoding.
 * ------------------------------------------------------------------------- */

#define PROXY_MAX_SEGMENTS 1024U

static int proxy_assemble_validate(const DeadpanProxyAssembleRequest *request) {
    if (request->width < 2 || request->height < 2 || request->width > PROXY_MAX_DIMENSION ||
        request->height > PROXY_MAX_DIMENSION || (request->width & 1) || (request->height & 1) ||
        request->time_base_num == 0 || request->time_base_den == 0 ||
        request->time_base_num > INT_MAX || request->time_base_den > INT_MAX ||
        request->sar_num == 0 || request->sar_den == 0 || request->sar_num > INT_MAX ||
        request->sar_den > INT_MAX || request->rotation_quarter_turns > 3 ||
        !proxy_valid_color(request->transfer, request->primaries)) {
        return fail("invalid_request", "proxy assembly stream is invalid");
    }
    if (request->frames == 0 || request->frames > PROXY_MAX_FRAMES || request->segments == NULL ||
        request->segment_count == 0 || request->segment_count > PROXY_MAX_SEGMENTS ||
        request->input_byte_length == 0 || request->input_byte_length > (uint64_t)INT64_MAX ||
        request->max_output_bytes == 0 || request->max_output_bytes > (uint64_t)INT64_MAX ||
        request->timeout_ms == 0 || request->timeout_ms > 24ULL * 60ULL * 60ULL * 1000ULL) {
        return fail("invalid_request", "proxy assembly budget is invalid");
    }
    uint64_t frames = 0;
    for (uint64_t index = 0; index < request->segment_count; index++) {
        const DeadpanProxySegment *segment = &request->segments[index];
        uint64_t end;
        if (segment->length == 0 || segment->frames == 0 ||
            segment->start_pts >= segment->end_pts ||
            !checked_add_u64(segment->offset, segment->length, &end) ||
            end > request->input_byte_length ||
            (index > 0 && request->segments[index - 1].end_pts != segment->start_pts) ||
            !checked_add_u64(frames, segment->frames, &frames)) {
            return fail("invalid_request", "proxy assembly range %" PRIu64 " is invalid", index);
        }
    }
    if (frames != request->frames) {
        return fail("invalid_request", "proxy assembly ranges miss pictures");
    }
    return 1;
}

int deadpan_proxy_assemble(int input_fd, int output_fd, const DeadpanProxyAssembleRequest *request,
                           DeadpanProxyAssembleReport *report, DeadpanConversionError *error) {
    RemuxInput input;
    RemuxInput check;
    AVFormatContext *output = NULL;
    AVStream *stream = NULL;
    OwnedAvio output_io;
    struct stat metadata;
    uint8_t *extradata = NULL;
    int extradata_size = 0;
    uint64_t start;
    uint64_t timeout_ns;
    uint64_t written = 0;
    int header_written = 0;
    int success = 0;
    int code;
    AVRational clock;

    memset(&input, 0, sizeof(input));
    memset(&check, 0, sizeof(check));
    memset(&output_io, 0, sizeof(output_io));
    memset(report, 0, sizeof(*report));
    memset(error, 0, sizeof(*error));
    memset(&state, 0, sizeof(state));
    state.error = error;
    if (!proxy_assemble_validate(request)) {
        return 1;
    }
    clock = (AVRational){(int)request->time_base_num, (int)request->time_base_den};
    start = monotonic_ns();
    if (start == UINT64_MAX || !checked_mul_u64(request->timeout_ms, 1000000ULL, &timeout_ns) ||
        !checked_add_u64(start, timeout_ns, &state.deadline_ns)) {
        fail("invalid_request", "proxy assembly deadline overflow");
        return 1;
    }
    av_log_set_level(AV_LOG_ERROR);
    av_log_set_callback(worker_log);
    av_max_alloc(128U * 1024U * 1024U);
    if (!verify_runtime()) {
        goto cleanup;
    }
    if (fstat(input_fd, &metadata) != 0 || !S_ISREG(metadata.st_mode) ||
        (uint64_t)metadata.st_size != request->input_byte_length) {
        fail("invalid_request", "proxy assembly input has another length");
        goto cleanup;
    }
    if (fstat(output_fd, &metadata) != 0 || !S_ISREG(metadata.st_mode) || metadata.st_size != 0 ||
        metadata.st_uid != geteuid() || (metadata.st_mode & 077) != 0) {
        fail("invalid_descriptor", "proxy output must be a private empty regular file");
        goto cleanup;
    }
    code = avformat_alloc_output_context2(&output, NULL, "mp4", NULL);
    if (code < 0 || output == NULL) {
        fail_ffmpeg("create assembled proxy MP4", code < 0 ? code : AVERROR_UNKNOWN);
        goto cleanup;
    }
    if (!owned_avio_open(&output_io, output_fd, 0, (int64_t)request->max_output_bytes, 1)) {
        goto cleanup;
    }
    output->pb = output_io.avio;
    output->flags |= AVFMT_FLAG_CUSTOM_IO;
    output->interrupt_callback.callback = deadline_interrupt;
    output->interrupt_callback.opaque = &state;
    output->io_open = deny_external_io;
    output->protocol_whitelist = av_strdup("");
    if (output->protocol_whitelist == NULL) {
        fail("resource_exhausted", "allocate FFmpeg output protocol denylist");
        goto cleanup;
    }
    for (uint64_t index = 0; index < request->segment_count; index++) {
        const DeadpanProxySegment *segment = &request->segments[index];
        if (!proxy_movie_open(&input, input_fd, (int64_t)segment->offset,
                              (int64_t)segment->length, request->width, request->height)) {
            goto cleanup;
        }
        const AVCodecParameters *parameters = input.format->streams[0]->codecpar;
        if (index == 0) {
            extradata_size = parameters->extradata_size;
            extradata = av_memdup(parameters->extradata, (size_t)extradata_size);
            stream = avformat_new_stream(output, NULL);
            if (extradata == NULL || stream == NULL) {
                fail("resource_exhausted", "allocate assembled proxy stream");
                goto cleanup;
            }
            code = avcodec_parameters_copy(stream->codecpar, parameters);
            if (code < 0) {
                fail_ffmpeg("copy proxy stream parameters", code);
                goto cleanup;
            }
            /* The recipe's interpretation, as a single-range encoding
               writes it, whatever the range movie's own boxes said. */
            stream->codecpar->codec_tag = 0;
            stream->codecpar->sample_aspect_ratio =
                (AVRational){(int)request->sar_num, (int)request->sar_den};
            stream->codecpar->color_range = AVCOL_RANGE_MPEG;
            stream->codecpar->color_space = AVCOL_SPC_BT709;
            stream->codecpar->color_trc = (enum AVColorTransferCharacteristic)request->transfer;
            stream->codecpar->color_primaries = (enum AVColorPrimaries)request->primaries;
            stream->codecpar->chroma_location = AVCHROMA_LOC_LEFT;
            stream->sample_aspect_ratio = stream->codecpar->sample_aspect_ratio;
            stream->time_base = clock;
            if (!proxy_set_rotation(stream->codecpar, request->rotation_quarter_turns)) {
                goto cleanup;
            }
            AVDictionary *options = NULL;
            int timescale = (int)request->time_base_den;
            if (av_dict_set_int(&options, "video_track_timescale", timescale, 0) < 0 ||
                av_dict_set_int(&options, "movie_timescale", timescale, 0) < 0) {
                av_dict_free(&options);
                fail("resource_exhausted", "configure assembled proxy MP4 clock");
                goto cleanup;
            }
            code = avformat_write_header(output, &options);
            av_dict_free(&options);
            if (code < 0) {
                fail_ffmpeg("write assembled proxy MP4 header", code);
                goto cleanup;
            }
            header_written = 1;
        } else if (parameters->extradata_size != extradata_size ||
                   memcmp(parameters->extradata, extradata, (size_t)extradata_size) != 0) {
            fail("segment_mismatch",
                 "proxy range %" PRIu64 " has another decoder configuration", index);
            goto cleanup;
        }
        if (!proxy_movie_read(&input, clock, segment->start_pts, segment->end_pts,
                              segment->frames, output, stream, &written)) {
            goto cleanup;
        }
        remux_close(&input);
    }
    code = av_write_trailer(output);
    header_written = 0;
    if (code < 0) {
        fail_ffmpeg("write assembled proxy MP4 trailer", code);
        goto cleanup;
    }
    avio_flush(output_io.avio);
    if (output_io.avio->error < 0) {
        fail_ffmpeg("flush assembled proxy MP4", output_io.avio->error);
        goto cleanup;
    }
    if (ftruncate(output_fd, (off_t)output_io.descriptor.length) != 0 || fsync(output_fd) != 0) {
        fail(io_code(errno), "finalize assembled proxy: %s", strerror(errno));
        goto cleanup;
    }
    /* Read the joined movie back: one configuration, every picture intra,
       contiguous from the first range's start to the last range's end. */
    {
        uint64_t unused = 0;
        if (!proxy_movie_open(&check, output_fd, 0, output_io.descriptor.length, request->width,
                              request->height) ||
            !proxy_movie_read(&check, clock, request->segments[0].start_pts,
                              request->segments[request->segment_count - 1].end_pts,
                              request->frames, NULL, NULL, &unused)) {
            goto cleanup;
        }
        const AVCodecParameters *parameters = check.format->streams[0]->codecpar;
        if (parameters->extradata_size != extradata_size ||
            memcmp(parameters->extradata, extradata, (size_t)extradata_size) != 0) {
            fail("verification_failed", "assembled proxy changed its decoder configuration");
            goto cleanup;
        }
    }
    report->output_bytes = (uint64_t)output_io.descriptor.length;
    report->packets = written;
    report->width = request->width;
    report->height = request->height;
    success = within_deadline();

cleanup:
    if (header_written && output != NULL) {
        (void)av_write_trailer(output);
    }
    if (output != NULL) {
        output->pb = NULL;
        avformat_free_context(output);
    }
    owned_avio_close(&output_io);
    remux_close(&input);
    remux_close(&check);
    av_free(extradata);
    if (!success && error->code[0] == '\0') {
        fail("internal_error", "proxy assembly failed without a classified error");
    }
    return success ? 0 : 1;
}
