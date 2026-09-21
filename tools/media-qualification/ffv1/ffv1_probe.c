/*
 * Developer-only FFV1 v3/Matroska qualification adapter.
 *
 * It decodes an input to bounded RGB8 frames, writes a private raw fixture,
 * encodes that fixture as FFV1 BGR0, then independently decodes the Matroska
 * and compares every RGB byte and frame ordinal. It deliberately does not
 * retain audio or infer missing timestamps.
 */
#include <errno.h>
#include <inttypes.h>
#include <stdint.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>

#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/avutil.h>
#include <libavutil/error.h>
#include <libavutil/imgutils.h>
#include <libavutil/opt.h>
#include <libswscale/swscale.h>

#define MAX_WIDTH 4096U
#define MAX_HEIGHT 4096U
#define MAX_FRAMES 10000U
#define MAX_FRAME_BYTES (64U * 1024U * 1024U)
#define MAX_SOURCE_BYTES (512ULL * 1024ULL * 1024ULL)
#define SOURCE_HEADER_BYTES 64U
#define SOURCE_VERSION 1U

static const uint8_t source_magic[8] = {'D', 'P', 'F', 'V', 'R', 'G', 'B', '1'};
static int observed_ffv1_version = -1;
static int observed_ffv1_ec = -1;

static void qualification_log(void *context, int level, const char *format, va_list arguments) {
    char message[1024];
    (void)context;
    vsnprintf(message, sizeof(message), format, arguments);
    int version;
    int ec;
    if (sscanf(message, "ver:%d keyframe:%*d coder:%*d ec:%d", &version, &ec) == 2) {
        observed_ffv1_version = version;
        observed_ffv1_ec = ec;
    }
    if (level <= AV_LOG_ERROR) {
        fputs(message, stderr);
    }
}

typedef struct {
    uint32_t width;
    uint32_t height;
    uint32_t frames;
    int32_t time_base_num;
    int32_t time_base_den;
    int32_t rate_num;
    int32_t rate_den;
    int64_t start_pts;
} SourceMeta;

typedef struct {
    FILE *file;
    SourceMeta meta;
    uint64_t frame_bytes;
    uint64_t expected_size;
} SourceReader;

static void print_fferror(const char *what, int error) {
    char message[AV_ERROR_MAX_STRING_SIZE];
    av_strerror(error, message, sizeof(message));
    fprintf(stderr, "%s: %s (%d)\n", what, message, error);
}

static int fail_message(const char *message) {
    fprintf(stderr, "error: %s\n", message);
    return 1;
}

static int fail_ffmpeg(const char *what, int error) {
    print_fferror(what, error);
    return 1;
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

static int valid_dimensions(uint32_t width, uint32_t height, uint64_t *frame_bytes) {
    uint64_t pixels;
    if (width == 0 || height == 0 || width > MAX_WIDTH || height > MAX_HEIGHT ||
        !checked_mul_u64(width, height, &pixels) || !checked_mul_u64(pixels, 3, frame_bytes) ||
        *frame_bytes > MAX_FRAME_BYTES) {
        return 0;
    }
    return 1;
}

static int write_exact(FILE *file, const void *data, size_t size) {
    return size == 0 || fwrite(data, 1, size, file) == size;
}

static int read_exact(FILE *file, void *data, size_t size) {
    return size == 0 || fread(data, 1, size, file) == size;
}

static void put_u32(uint8_t *bytes, uint32_t value) {
    bytes[0] = (uint8_t)value;
    bytes[1] = (uint8_t)(value >> 8);
    bytes[2] = (uint8_t)(value >> 16);
    bytes[3] = (uint8_t)(value >> 24);
}

static uint32_t get_u32(const uint8_t *bytes) {
    return (uint32_t)bytes[0] | ((uint32_t)bytes[1] << 8) |
           ((uint32_t)bytes[2] << 16) | ((uint32_t)bytes[3] << 24);
}

static void put_i32(uint8_t *bytes, int32_t value) {
    put_u32(bytes, (uint32_t)value);
}

static int32_t get_i32(const uint8_t *bytes) {
    return (int32_t)get_u32(bytes);
}

static void put_u64(uint8_t *bytes, uint64_t value) {
    for (unsigned int index = 0; index < 8; index++) {
        bytes[index] = (uint8_t)(value >> (8U * index));
    }
}

static uint64_t get_u64(const uint8_t *bytes) {
    uint64_t value = 0;
    for (unsigned int index = 0; index < 8; index++) {
        value |= (uint64_t)bytes[index] << (8U * index);
    }
    return value;
}

static void put_i64(uint8_t *bytes, int64_t value) {
    put_u64(bytes, (uint64_t)value);
}

static int64_t get_i64(const uint8_t *bytes) {
    return (int64_t)get_u64(bytes);
}

static int valid_rational(int32_t numerator, int32_t denominator) {
    return numerator > 0 && denominator > 0;
}

static int write_source_header(FILE *file, const SourceMeta *meta) {
    uint8_t bytes[SOURCE_HEADER_BYTES] = {0};
    memcpy(bytes, source_magic, sizeof(source_magic));
    put_u32(bytes + 8, SOURCE_VERSION);
    put_u32(bytes + 12, meta->width);
    put_u32(bytes + 16, meta->height);
    put_u32(bytes + 20, meta->frames);
    put_i32(bytes + 24, meta->time_base_num);
    put_i32(bytes + 28, meta->time_base_den);
    put_i32(bytes + 32, meta->rate_num);
    put_i32(bytes + 36, meta->rate_den);
    put_i64(bytes + 40, meta->start_pts);
    return write_exact(file, bytes, sizeof(bytes));
}

static int patch_source_header(FILE *file, const SourceMeta *meta) {
    if (fseek(file, 0, SEEK_SET) != 0 || !write_source_header(file, meta) ||
        fflush(file) != 0 || fseek(file, 0, SEEK_END) != 0) {
        return 0;
    }
    return 1;
}

static int append_source_frame(FILE *file, int64_t pts, int64_t duration, const uint8_t *rgb,
                               int rgb_linesize, uint32_t width, uint32_t height) {
    uint8_t timestamps[16];
    put_i64(timestamps, pts);
    put_i64(timestamps + 8, duration);
    if (!write_exact(file, timestamps, sizeof(timestamps))) {
        return 0;
    }
    for (uint32_t row = 0; row < height; row++) {
        if (!write_exact(file, rgb + (size_t)row * rgb_linesize, (size_t)width * 3)) {
            return 0;
        }
    }
    return 1;
}

static int source_file_size(FILE *file, uint64_t *size) {
    struct stat status;
    int descriptor = fileno(file);
    if (descriptor < 0 || fstat(descriptor, &status) != 0 || status.st_size < 0) {
        return 0;
    }
    *size = (uint64_t)status.st_size;
    return 1;
}

static int source_reader_open(SourceReader *reader, const char *path) {
    uint8_t bytes[SOURCE_HEADER_BYTES];
    uint64_t frame_bytes;
    uint64_t per_frame;
    uint64_t payload;
    uint64_t expected_size;
    memset(reader, 0, sizeof(*reader));
    reader->file = fopen(path, "rb");
    if (reader->file == NULL) {
        fprintf(stderr, "open %s: %s\n", path, strerror(errno));
        return 0;
    }
    if (!read_exact(reader->file, bytes, sizeof(bytes)) ||
        memcmp(bytes, source_magic, sizeof(source_magic)) != 0 ||
        get_u32(bytes + 8) != SOURCE_VERSION) {
        fprintf(stderr, "invalid RGB source header: %s\n", path);
        fclose(reader->file);
        reader->file = NULL;
        return 0;
    }
    reader->meta.width = get_u32(bytes + 12);
    reader->meta.height = get_u32(bytes + 16);
    reader->meta.frames = get_u32(bytes + 20);
    reader->meta.time_base_num = get_i32(bytes + 24);
    reader->meta.time_base_den = get_i32(bytes + 28);
    reader->meta.rate_num = get_i32(bytes + 32);
    reader->meta.rate_den = get_i32(bytes + 36);
    reader->meta.start_pts = get_i64(bytes + 40);
    if (reader->meta.frames == 0 || reader->meta.frames > MAX_FRAMES ||
        !valid_dimensions(reader->meta.width, reader->meta.height, &frame_bytes) ||
        !valid_rational(reader->meta.time_base_num, reader->meta.time_base_den) ||
        !valid_rational(reader->meta.rate_num, reader->meta.rate_den) ||
        !checked_add_u64(16, frame_bytes, &per_frame) ||
        !checked_mul_u64(reader->meta.frames, per_frame, &payload) ||
        !checked_add_u64(SOURCE_HEADER_BYTES, payload, &expected_size) ||
        expected_size > MAX_SOURCE_BYTES) {
        fprintf(stderr, "invalid or oversized RGB source metadata: %s\n", path);
        fclose(reader->file);
        reader->file = NULL;
        return 0;
    }
    if (!source_file_size(reader->file, &reader->expected_size) ||
        reader->expected_size != expected_size) {
        fprintf(stderr, "RGB source has truncated or trailing bytes: %s\n", path);
        fclose(reader->file);
        reader->file = NULL;
        return 0;
    }
    reader->frame_bytes = frame_bytes;
    return 1;
}

static void source_reader_close(SourceReader *reader) {
    if (reader->file != NULL) {
        fclose(reader->file);
    }
    memset(reader, 0, sizeof(*reader));
}

static int source_reader_next(SourceReader *reader, uint32_t index, int64_t *pts,
                              int64_t *duration, uint8_t *rgb) {
    uint8_t timestamps[16];
    if (!read_exact(reader->file, timestamps, sizeof(timestamps)) ||
        !read_exact(reader->file, rgb, (size_t)reader->frame_bytes)) {
        return 0;
    }
    *pts = get_i64(timestamps);
    *duration = get_i64(timestamps + 8);
    (void)index;
    return 1;
}

static int frame_buffer(AVFrame *frame, enum AVPixelFormat format, int width, int height) {
    frame->format = format;
    frame->width = width;
    frame->height = height;
    return av_frame_get_buffer(frame, 32);
}

static int write_decoded_source(const char *input_path, const char *source_path) {
    AVFormatContext *format = NULL;
    AVCodecContext *decoder = NULL;
    const AVCodec *decoder_codec;
    AVPacket *packet = NULL;
    AVFrame *decoded = NULL;
    AVFrame *rgb = NULL;
    struct SwsContext *to_rgb = NULL;
    FILE *source = NULL;
    SourceMeta meta = {0};
    uint64_t frame_bytes;
    uint32_t frame_count = 0;
    int video_index = -1;
    int result = 1;
    int error;

    error = avformat_open_input(&format, input_path, NULL, NULL);
    if (error < 0) {
        return fail_ffmpeg("open source", error);
    }
    error = avformat_find_stream_info(format, NULL);
    if (error < 0) {
        result = fail_ffmpeg("read source streams", error);
        goto cleanup;
    }
    for (unsigned int index = 0; index < format->nb_streams; index++) {
        if (format->streams[index]->codecpar->codec_type == AVMEDIA_TYPE_VIDEO) {
            video_index = (int)index;
            break;
        }
    }
    if (video_index < 0) {
        result = fail_message("source has no video stream");
        goto cleanup;
    }
    AVStream *stream = format->streams[video_index];
    if (!valid_rational(stream->time_base.num, stream->time_base.den)) {
        result = fail_message("source has no valid native time base");
        goto cleanup;
    }
    decoder_codec = avcodec_find_decoder(stream->codecpar->codec_id);
    if (decoder_codec == NULL) {
        result = fail_message("source codec is unavailable");
        goto cleanup;
    }
    decoder = avcodec_alloc_context3(decoder_codec);
    if (decoder == NULL) {
        result = fail_message("allocate source decoder");
        goto cleanup;
    }
    error = avcodec_parameters_to_context(decoder, stream->codecpar);
    if (error < 0) {
        result = fail_ffmpeg("copy source decoder parameters", error);
        goto cleanup;
    }
    decoder->err_recognition = AV_EF_EXPLODE;
    decoder->debug |= FF_DEBUG_PICT_INFO;
    decoder->thread_count = 1;
    error = avcodec_open2(decoder, decoder_codec, NULL);
    if (error < 0) {
        result = fail_ffmpeg("open source decoder", error);
        goto cleanup;
    }
    if (decoder->width <= 0 || decoder->height <= 0 ||
        !valid_dimensions((uint32_t)decoder->width, (uint32_t)decoder->height, &frame_bytes)) {
        result = fail_message("source dimensions exceed qualification bounds");
        goto cleanup;
    }
    meta.width = (uint32_t)decoder->width;
    meta.height = (uint32_t)decoder->height;
    meta.time_base_num = stream->time_base.num;
    meta.time_base_den = stream->time_base.den;
    AVRational rate = stream->avg_frame_rate;
    if (!valid_rational(rate.num, rate.den)) {
        rate = stream->r_frame_rate;
    }
    if (!valid_rational(rate.num, rate.den)) {
        result = fail_message("source has no valid native frame rate");
        goto cleanup;
    }
    meta.rate_num = rate.num;
    meta.rate_den = rate.den;
    meta.start_pts = stream->start_time == AV_NOPTS_VALUE ? 0 : stream->start_time;
    if (stream->codecpar->format != AV_PIX_FMT_GBRP ||
        stream->codecpar->bits_per_raw_sample != 8 ||
        stream->codecpar->color_range != AVCOL_RANGE_JPEG ||
        stream->codecpar->color_space != AVCOL_SPC_RGB ||
        stream->codecpar->color_trc != AVCOL_TRC_IEC61966_2_1 ||
        stream->codecpar->color_primaries != AVCOL_PRI_BT709) {
        result = fail_message("source is not tagged full-range 8-bit GBR sRGB/BT.709");
        goto cleanup;
    }
    source = fopen(source_path, "wb+");
    if (source == NULL) {
        fprintf(stderr, "create %s: %s\n", source_path, strerror(errno));
        goto cleanup;
    }
    if (!write_source_header(source, &meta)) {
        result = fail_message("write RGB source header");
        goto cleanup;
    }
    packet = av_packet_alloc();
    decoded = av_frame_alloc();
    rgb = av_frame_alloc();
    if (packet == NULL || decoded == NULL || rgb == NULL) {
        result = fail_message("allocate source decode frames");
        goto cleanup;
    }
    error = frame_buffer(rgb, AV_PIX_FMT_RGB24, decoder->width, decoder->height);
    if (error < 0) {
        result = fail_ffmpeg("allocate RGB frame", error);
        goto cleanup;
    }
    to_rgb = sws_getContext(decoder->width, decoder->height, decoder->pix_fmt,
                            decoder->width, decoder->height, AV_PIX_FMT_RGB24,
                            SWS_POINT, NULL, NULL, NULL);
    if (to_rgb == NULL) {
        result = fail_message("create RGB converter");
        goto cleanup;
    }

    for (;;) {
        error = av_read_frame(format, packet);
        if (error == AVERROR_EOF) {
            error = avcodec_send_packet(decoder, NULL);
        } else if (error < 0) {
            result = fail_ffmpeg("read source packet", error);
            goto cleanup;
        } else if (packet->stream_index != video_index) {
            av_packet_unref(packet);
            continue;
        } else {
            error = avcodec_send_packet(decoder, packet);
            av_packet_unref(packet);
        }
        if (error < 0 && error != AVERROR_EOF) {
            result = fail_ffmpeg("send source packet", error);
            goto cleanup;
        }
        for (;;) {
            error = avcodec_receive_frame(decoder, decoded);
            if (error == AVERROR(EAGAIN) || error == AVERROR_EOF) {
                break;
            }
            if (error < 0) {
                result = fail_ffmpeg("decode source frame", error);
                goto cleanup;
            }
            uint64_t per_frame;
            uint64_t payload;
            uint64_t total_size;
            if (frame_count >= MAX_FRAMES ||
                !valid_dimensions((uint32_t)decoded->width, (uint32_t)decoded->height, &frame_bytes) ||
                !checked_add_u64(16, frame_bytes, &per_frame) ||
                !checked_mul_u64((uint64_t)frame_count + 1, per_frame, &payload) ||
                !checked_add_u64(SOURCE_HEADER_BYTES, payload, &total_size) ||
                total_size > MAX_SOURCE_BYTES) {
                result = fail_message("source contains too many or too-large frames");
                goto cleanup;
            }
            if (decoded->width != (int)meta.width || decoded->height != (int)meta.height ||
                decoded->format != decoder->pix_fmt || decoded->best_effort_timestamp == AV_NOPTS_VALUE ||
                decoded->color_range != AVCOL_RANGE_JPEG || decoded->colorspace != AVCOL_SPC_RGB ||
                decoded->color_trc != AVCOL_TRC_IEC61966_2_1 ||
                decoded->color_primaries != AVCOL_PRI_BT709) {
                result = fail_message("source frame has changing dimensions or missing PTS");
                goto cleanup;
            }
            if (sws_scale(to_rgb, (const uint8_t *const *)decoded->data, decoded->linesize,
                          0, decoded->height, rgb->data, rgb->linesize) <= 0) {
                result = fail_message("convert source frame to RGB8");
                goto cleanup;
            }
            if (!append_source_frame(source, decoded->best_effort_timestamp, decoded->duration,
                                     rgb->data[0], rgb->linesize[0], meta.width, meta.height)) {
                result = fail_message("write decoded RGB source frame");
                goto cleanup;
            }
            if (frame_count == 0) {
                meta.start_pts = decoded->best_effort_timestamp;
            }
            frame_count++;
            av_frame_unref(decoded);
        }
        if (format->pb == NULL && error == AVERROR_EOF) {
            break;
        }
        if (error == AVERROR_EOF) {
            break;
        }
    }
    if (frame_count == 0) {
        result = fail_message("source decoded zero video frames");
        goto cleanup;
    }
    meta.frames = frame_count;
    if (!patch_source_header(source, &meta)) {
        result = fail_message("finalize RGB source header");
        goto cleanup;
    }
    result = 0;

cleanup:
    if (source != NULL) {
        fclose(source);
    }
    sws_freeContext(to_rgb);
    av_frame_free(&rgb);
    av_frame_free(&decoded);
    av_packet_free(&packet);
    avcodec_free_context(&decoder);
    avformat_close_input(&format);
    return result;
}

static int make_synthetic_source(const char *source_path, uint32_t width, uint32_t height,
                                 uint32_t frames) {
    uint64_t frame_bytes;
    SourceMeta meta = {
        .width = width,
        .height = height,
        .frames = frames,
        .time_base_num = 1,
        .time_base_den = 30000,
        .rate_num = 30000,
        .rate_den = 1001,
        .start_pts = 0,
    };
    if (frames == 0 || frames > MAX_FRAMES || !valid_dimensions(width, height, &frame_bytes)) {
        return fail_message("invalid synthetic dimensions or frame count");
    }
    FILE *source = fopen(source_path, "wb");
    if (source == NULL) {
        fprintf(stderr, "create %s: %s\n", source_path, strerror(errno));
        return 1;
    }
    uint8_t *rgb = malloc((size_t)frame_bytes);
    if (rgb == NULL) {
        fclose(source);
        return fail_message("allocate synthetic RGB frame");
    }
    int result = 1;
    if (!write_source_header(source, &meta)) {
        result = fail_message("write synthetic RGB source header");
        goto cleanup;
    }
    for (uint32_t frame = 0; frame < frames; frame++) {
        for (uint32_t y = 0; y < height; y++) {
            for (uint32_t x = 0; x < width; x++) {
                uint64_t offset = ((uint64_t)y * width + x) * 3;
                rgb[offset] = (uint8_t)((x + frame * 3) & 255U);
                rgb[offset + 1] = (uint8_t)((y * 5 + frame * 7) & 255U);
                rgb[offset + 2] = (uint8_t)((x * 11 + y * 13 + frame * 17) & 255U);
            }
        }
        if (!append_source_frame(source, (int64_t)frame * 1001, 1001, rgb,
                                 (int)(width * 3), width, height)) {
            result = fail_message("write synthetic RGB frame");
            goto cleanup;
        }
    }
    result = 0;
cleanup:
    free(rgb);
    fclose(source);
    return result;
}

typedef struct {
    AVFormatContext *format;
    AVCodecContext *codec;
    AVStream *stream;
} Encoder;

static int encoder_open(Encoder *encoder, const SourceMeta *meta, const char *output_path) {
    const AVCodec *codec = avcodec_find_encoder(AV_CODEC_ID_FFV1);
    if (codec == NULL) {
        return fail_message("FFV1 encoder is unavailable");
    }
    int error = avformat_alloc_output_context2(&encoder->format, NULL, "matroska", output_path);
    if (error < 0 || encoder->format == NULL) {
        return fail_ffmpeg("create Matroska output", error < 0 ? error : AVERROR_UNKNOWN);
    }
    encoder->codec = avcodec_alloc_context3(codec);
    if (encoder->codec == NULL) {
        return fail_message("allocate FFV1 encoder");
    }
    encoder->codec->width = (int)meta->width;
    encoder->codec->height = (int)meta->height;
    /* FFV1 8.0.3 exposes BGR0 as its lossless 8-bit RGB format. */
    encoder->codec->pix_fmt = AV_PIX_FMT_BGR0;
    encoder->codec->time_base = (AVRational){meta->time_base_num, meta->time_base_den};
    encoder->codec->framerate = (AVRational){meta->rate_num, meta->rate_den};
    encoder->codec->gop_size = 1;
    encoder->codec->max_b_frames = 0;
    encoder->codec->thread_count = 1;
    encoder->codec->level = 3;
    encoder->codec->color_range = AVCOL_RANGE_JPEG;
    encoder->codec->colorspace = AVCOL_SPC_RGB;
    encoder->codec->color_trc = AVCOL_TRC_IEC61966_2_1;
    encoder->codec->color_primaries = AVCOL_PRI_BT709;
    error = av_opt_set_int(encoder->codec->priv_data, "slicecrc", 1, 0);
    if (error < 0) {
        return fail_ffmpeg("enable FFV1 slice CRC", error);
    }
    error = avcodec_open2(encoder->codec, codec, NULL);
    if (error < 0) {
        return fail_ffmpeg("open FFV1 encoder", error);
    }
    encoder->stream = avformat_new_stream(encoder->format, NULL);
    if (encoder->stream == NULL) {
        return fail_message("create FFV1 stream");
    }
    encoder->stream->avg_frame_rate = encoder->codec->framerate;
    encoder->stream->r_frame_rate = encoder->codec->framerate;
    error = avcodec_parameters_from_context(encoder->stream->codecpar, encoder->codec);
    if (error < 0) {
        return fail_ffmpeg("copy FFV1 parameters", error);
    }
    encoder->stream->time_base = encoder->codec->time_base;
    av_dict_set(&encoder->stream->metadata, "COLOR_RANGE", "full", 0);
    av_dict_set(&encoder->stream->metadata, "COLOR_SPACE", "gbr", 0);
    av_dict_set(&encoder->stream->metadata, "COLOR_TRANSFER", "sRGB", 0);
    av_dict_set(&encoder->stream->metadata, "COLOR_PRIMARIES", "bt709", 0);
    if (!(encoder->format->oformat->flags & AVFMT_NOFILE)) {
        error = avio_open(&encoder->format->pb, output_path, AVIO_FLAG_WRITE);
        if (error < 0) {
            return fail_ffmpeg("open Matroska output", error);
        }
    }
    error = avformat_write_header(encoder->format, NULL);
    if (error < 0) {
        return fail_ffmpeg("write Matroska header", error);
    }
    return 0;
}

static void encoder_close(Encoder *encoder) {
    if (encoder->format != NULL) {
        if (encoder->format->pb != NULL && !(encoder->format->oformat->flags & AVFMT_NOFILE)) {
            avio_closep(&encoder->format->pb);
        }
        avformat_free_context(encoder->format);
    }
    avcodec_free_context(&encoder->codec);
    memset(encoder, 0, sizeof(*encoder));
}

static int encode_source(const char *source_path, const char *output_path) {
    SourceReader source;
    Encoder encoder = {0};
    AVFrame *rgb = NULL;
    AVFrame *bgr0 = NULL;
    AVPacket *packet = NULL;
    uint8_t *packed = NULL;
    struct SwsContext *to_bgr0 = NULL;
    int result = 1;
    if (!source_reader_open(&source, source_path)) {
        return 1;
    }
    if (encoder_open(&encoder, &source.meta, output_path) != 0) {
        source_reader_close(&source);
        encoder_close(&encoder);
        return 1;
    }
    rgb = av_frame_alloc();
    bgr0 = av_frame_alloc();
    packet = av_packet_alloc();
    if (rgb == NULL || bgr0 == NULL || packet == NULL ||
        frame_buffer(rgb, AV_PIX_FMT_RGB24, (int)source.meta.width, (int)source.meta.height) < 0 ||
        frame_buffer(bgr0, AV_PIX_FMT_BGR0, (int)source.meta.width, (int)source.meta.height) < 0) {
        result = fail_message("allocate FFV1 frames");
        goto cleanup;
    }
    if (rgb->data[0] == NULL || bgr0->data[0] == NULL) {
        result = fail_message("FFmpeg returned an empty RGB/BGR0 frame buffer");
        goto cleanup;
    }
    to_bgr0 = sws_getContext((int)source.meta.width, (int)source.meta.height, AV_PIX_FMT_RGB24,
                              (int)source.meta.width, (int)source.meta.height, AV_PIX_FMT_BGR0,
                              SWS_POINT, NULL, NULL, NULL);
    if (to_bgr0 == NULL) {
        result = fail_message("create BGR0 converter");
        goto cleanup;
    }
    packed = malloc((size_t)source.frame_bytes);
    if (packed == NULL) {
        result = fail_message("allocate packed RGB scratch");
        goto cleanup;
    }
    for (uint32_t index = 0; index < source.meta.frames; index++) {
        int64_t pts;
        int64_t duration;
        if (!source_reader_next(&source, index, &pts, &duration, packed)) {
            result = fail_message("read RGB source frame");
            goto cleanup;
        }
        if (av_frame_make_writable(rgb) < 0 || av_frame_make_writable(bgr0) < 0) {
            result = fail_message("make FFV1 frame writable");
            goto cleanup;
        }
        for (uint32_t row = 0; row < source.meta.height; row++) {
            memcpy(rgb->data[0] + (size_t)row * rgb->linesize[0],
                   packed + (size_t)row * source.meta.width * 3,
                   (size_t)source.meta.width * 3);
        }
        if (sws_scale(to_bgr0, (const uint8_t *const *)rgb->data, rgb->linesize, 0,
                      (int)source.meta.height, bgr0->data, bgr0->linesize) <= 0) {
            result = fail_message("convert RGB source to BGR0");
            goto cleanup;
        }
        bgr0->pts = pts;
        bgr0->duration = duration > 0 ? duration : 0;
        int error = avcodec_send_frame(encoder.codec, bgr0);
        if (error < 0) {
            result = fail_ffmpeg("send FFV1 frame", error);
            goto cleanup;
        }
        for (;;) {
            error = avcodec_receive_packet(encoder.codec, packet);
            if (error == AVERROR(EAGAIN) || error == AVERROR_EOF) {
                break;
            }
            if (error < 0) {
                result = fail_ffmpeg("receive FFV1 packet", error);
                goto cleanup;
            }
            av_packet_rescale_ts(packet, encoder.codec->time_base, encoder.stream->time_base);
            packet->stream_index = encoder.stream->index;
            error = av_interleaved_write_frame(encoder.format, packet);
            av_packet_unref(packet);
            if (error < 0) {
                result = fail_ffmpeg("write FFV1 packet", error);
                goto cleanup;
            }
        }
    }
    if (avcodec_send_frame(encoder.codec, NULL) < 0) {
        result = fail_message("flush FFV1 encoder");
        goto cleanup;
    }
    for (;;) {
        int error = avcodec_receive_packet(encoder.codec, packet);
        if (error == AVERROR_EOF || error == AVERROR(EAGAIN)) {
            break;
        }
        if (error < 0) {
            result = fail_ffmpeg("receive FFV1 flush packet", error);
            goto cleanup;
        }
        av_packet_rescale_ts(packet, encoder.codec->time_base, encoder.stream->time_base);
        packet->stream_index = encoder.stream->index;
        error = av_interleaved_write_frame(encoder.format, packet);
        av_packet_unref(packet);
        if (error < 0) {
            result = fail_ffmpeg("write FFV1 flush packet", error);
            goto cleanup;
        }
    }
    if (av_write_trailer(encoder.format) < 0) {
        result = fail_message("write Matroska trailer");
        goto cleanup;
    }
    result = 0;
cleanup:
    free(packed);
    sws_freeContext(to_bgr0);
    av_packet_free(&packet);
    av_frame_free(&bgr0);
    av_frame_free(&rgb);
    encoder_close(&encoder);
    source_reader_close(&source);
    return result;
}

static int decode_and_compare(const char *output_path, const char *source_path) {
    SourceReader source;
    AVFormatContext *format = NULL;
    AVCodecContext *decoder = NULL;
    const AVCodec *decoder_codec;
    AVPacket *packet = NULL;
    AVFrame *decoded = NULL;
    AVFrame *rgb = NULL;
    struct SwsContext *to_rgb = NULL;
    int video_index = -1;
    int result = 1;
    uint32_t frame_count = 0;
    int64_t source_pts[MAX_FRAMES];
    int64_t source_durations[MAX_FRAMES];
    int64_t output_pts[MAX_FRAMES];
    int64_t output_durations[MAX_FRAMES];
    int error;
    if (!source_reader_open(&source, source_path)) {
        return 1;
    }
    error = avformat_open_input(&format, output_path, NULL, NULL);
    if (error < 0) {
        source_reader_close(&source);
        return fail_ffmpeg("open FFV1 output", error);
    }
    error = avformat_find_stream_info(format, NULL);
    if (error < 0) {
        result = fail_ffmpeg("read FFV1 output streams", error);
        goto cleanup;
    }
    if (format->iformat == NULL || strncmp(format->iformat->name, "matroska", 8) != 0) {
        result = fail_message("output is not Matroska");
        goto cleanup;
    }
    if (format->nb_streams != 1 || format->streams[0]->codecpar->codec_type != AVMEDIA_TYPE_VIDEO) {
        result = fail_message("FFV1 output retained a non-video stream");
        goto cleanup;
    }
    video_index = 0;
    AVStream *stream = format->streams[video_index];
    if (stream->codecpar->codec_id != AV_CODEC_ID_FFV1 ||
        stream->codecpar->format != AV_PIX_FMT_BGR0 ||
        stream->codecpar->color_range != AVCOL_RANGE_JPEG ||
        stream->codecpar->color_space != AVCOL_SPC_RGB ||
        stream->codecpar->color_trc != AVCOL_TRC_IEC61966_2_1 ||
        stream->codecpar->color_primaries != AVCOL_PRI_BT709) {
        result = fail_message("FFV1 output metadata is not full-range GBR sRGB/BT.709");
        goto cleanup;
    }
    if (stream->codecpar->width != (int)source.meta.width ||
        stream->codecpar->height != (int)source.meta.height ||
        !valid_rational(stream->time_base.num, stream->time_base.den)) {
        result = fail_message("FFV1 output dimensions or time base changed");
        goto cleanup;
    }
    decoder_codec = avcodec_find_decoder(stream->codecpar->codec_id);
    if (decoder_codec == NULL) {
        result = fail_message("FFV1 decoder is unavailable");
        goto cleanup;
    }
    decoder = avcodec_alloc_context3(decoder_codec);
    if (decoder == NULL) {
        result = fail_message("allocate FFV1 decoder");
        goto cleanup;
    }
    error = avcodec_parameters_to_context(decoder, stream->codecpar);
    if (error < 0) {
        result = fail_ffmpeg("copy FFV1 decoder parameters", error);
        goto cleanup;
    }
    decoder->err_recognition = AV_EF_EXPLODE;
    decoder->debug |= FF_DEBUG_PICT_INFO;
    decoder->thread_count = 1;
    if ((error = avcodec_open2(decoder, decoder_codec, NULL)) < 0) {
        result = fail_ffmpeg("open FFV1 decoder", error);
        goto cleanup;
    }
    if (decoder->level != AV_LEVEL_UNKNOWN && decoder->level != 3) {
        result = fail_message("FFV1 output is not version 3 with slice CRC");
        goto cleanup;
    }
    if (decoder->width != (int)source.meta.width || decoder->height != (int)source.meta.height ||
        decoder->pix_fmt != AV_PIX_FMT_BGR0) {
        result = fail_message("FFV1 decoder changed dimensions or pixel format");
        goto cleanup;
    }
    packet = av_packet_alloc();
    decoded = av_frame_alloc();
    rgb = av_frame_alloc();
    if (packet == NULL || decoded == NULL || rgb == NULL ||
        frame_buffer(rgb, AV_PIX_FMT_RGB24, decoder->width, decoder->height) < 0) {
        result = fail_message("allocate FFV1 decode frames");
        goto cleanup;
    }
    to_rgb = sws_getContext(decoder->width, decoder->height, decoder->pix_fmt,
                            decoder->width, decoder->height, AV_PIX_FMT_RGB24,
                            SWS_POINT, NULL, NULL, NULL);
    if (to_rgb == NULL) {
        result = fail_message("create FFV1 RGB converter");
        goto cleanup;
    }
    for (;;) {
        error = av_read_frame(format, packet);
        if (error == AVERROR_EOF) {
            error = avcodec_send_packet(decoder, NULL);
        } else if (error < 0) {
            result = fail_ffmpeg("read FFV1 packet", error);
            goto cleanup;
        } else {
            error = avcodec_send_packet(decoder, packet);
            av_packet_unref(packet);
        }
        if (error < 0 && error != AVERROR_EOF) {
            result = fail_ffmpeg("send FFV1 packet", error);
            goto cleanup;
        }
        for (;;) {
            error = avcodec_receive_frame(decoder, decoded);
            if (error == AVERROR(EAGAIN) || error == AVERROR_EOF) {
                break;
            }
            if (error < 0) {
                result = fail_ffmpeg("decode FFV1 frame", error);
                goto cleanup;
            }
            if (frame_count >= source.meta.frames || decoded->best_effort_timestamp == AV_NOPTS_VALUE) {
                result = fail_message("FFV1 output has extra frame or missing PTS");
                goto cleanup;
            }
            if (decoded->width != (int)source.meta.width || decoded->height != (int)source.meta.height ||
                decoded->format != AV_PIX_FMT_BGR0 || decoded->color_range != AVCOL_RANGE_JPEG ||
                decoded->colorspace != AVCOL_SPC_RGB || decoded->color_trc != AVCOL_TRC_IEC61966_2_1 ||
                decoded->color_primaries != AVCOL_PRI_BT709) {
                result = fail_message("FFV1 frame metadata changed from the stream tags");
                goto cleanup;
            }
            int64_t expected_pts;
            int64_t expected_duration;
            uint8_t *expected = malloc((size_t)source.frame_bytes);
            if (expected == NULL) {
                result = fail_message("allocate expected RGB frame");
                goto cleanup;
            }
            if (!source_reader_next(&source, frame_count, &expected_pts, &expected_duration, expected)) {
                free(expected);
                result = fail_message("read expected RGB frame");
                goto cleanup;
            }
            if (av_frame_make_writable(rgb) < 0 ||
                sws_scale(to_rgb, (const uint8_t *const *)decoded->data, decoded->linesize,
                          0, decoded->height, rgb->data, rgb->linesize) <= 0) {
                free(expected);
                result = fail_message("convert FFV1 frame to RGB8");
                goto cleanup;
            }
            int mismatch = 0;
            for (uint32_t row = 0; row < source.meta.height && !mismatch; row++) {
                if (memcmp(rgb->data[0] + (size_t)row * rgb->linesize[0],
                           expected + (size_t)row * source.meta.width * 3,
                           (size_t)source.meta.width * 3) != 0) {
                    mismatch = 1;
                }
            }
            free(expected);
            if (mismatch) {
                result = fail_message("FFV1 RGB pixel mismatch");
                goto cleanup;
            }
            source_pts[frame_count] = expected_pts;
            source_durations[frame_count] = expected_duration;
            output_pts[frame_count] = decoded->best_effort_timestamp;
            output_durations[frame_count] = decoded->duration;
            frame_count++;
            av_frame_unref(decoded);
        }
        if (error == AVERROR_EOF) {
            break;
        }
    }
    if (frame_count != source.meta.frames) {
        result = fail_message("FFV1 output frame count differs from source");
        goto cleanup;
    }
    if (observed_ffv1_version != 3 || observed_ffv1_ec != 1) {
        result = fail_message("FFV1 decoded bitstream did not report version 3 with slice CRC");
        goto cleanup;
    }
    printf("{\"status\":\"passed\",\"source\":{\"width\":%u,\"height\":%u,\"frames\":%u,\"time_base\":[%d,%d],\"frame_rate\":[%d,%d],\"start_pts\":%" PRId64 ",\"native_pts\":[",
           source.meta.width, source.meta.height, source.meta.frames, source.meta.time_base_num,
           source.meta.time_base_den, source.meta.rate_num, source.meta.rate_den, source.meta.start_pts);
    for (uint32_t index = 0; index < source.meta.frames; index++) {
        if (index != 0) {
            fputs(",", stdout);
        }
        printf("%" PRId64, source_pts[index]);
    }
    fputs("],\"durations\":[", stdout);
    for (uint32_t index = 0; index < source.meta.frames; index++) {
        if (index != 0) {
            fputs(",", stdout);
        }
        printf("%" PRId64, source_durations[index]);
    }
    printf("]},\"output\":{\"codec\":\"ffv1\",\"pixel_format\":\"bgr0\",\"ffv1_version\":%d,\"slice_crc\":true,\"color_range\":\"full\",\"color_space\":\"gbr\",\"color_transfer\":\"sRGB\",\"color_primaries\":\"bt709\",\"frames\":%u,\"time_base\":[%d,%d],\"frame_rate\":[%d,%d],\"native_pts\":[", observed_ffv1_version, frame_count,
           stream->time_base.num, stream->time_base.den, stream->avg_frame_rate.num,
           stream->avg_frame_rate.den);
    for (uint32_t index = 0; index < frame_count; index++) {
        if (index != 0) {
            fputs(",", stdout);
        }
        printf("%" PRId64, output_pts[index]);
    }
    fputs("],\"durations\":[", stdout);
    for (uint32_t index = 0; index < frame_count; index++) {
        if (index != 0) {
            fputs(",", stdout);
        }
        printf("%" PRId64, output_durations[index]);
    }
    fputs("]}}\n", stdout);
    result = 0;
cleanup:
    sws_freeContext(to_rgb);
    av_frame_free(&rgb);
    av_frame_free(&decoded);
    av_packet_free(&packet);
    avcodec_free_context(&decoder);
    avformat_close_input(&format);
    source_reader_close(&source);
    return result;
}

int main(int argc, char **argv) {
    av_log_set_level(AV_LOG_DEBUG);
    av_log_set_callback(qualification_log);
    if (argc == 5 && strcmp(argv[1], "qualify") == 0) {
        if (write_decoded_source(argv[2], argv[4]) != 0 || encode_source(argv[4], argv[3]) != 0) {
            return 1;
        }
        return decode_and_compare(argv[3], argv[4]);
    }
    if (argc == 4 && strcmp(argv[1], "synth") == 0) {
        if (make_synthetic_source(argv[3], 32, 16, 30) != 0 ||
            encode_source(argv[3], argv[2]) != 0) {
            return 1;
        }
        return decode_and_compare(argv[2], argv[3]);
    }
    if (argc == 4 && strcmp(argv[1], "verify") == 0) {
        return decode_and_compare(argv[2], argv[3]);
    }
    fprintf(stderr, "usage: %s qualify INPUT OUTPUT SOURCE\n", argv[0]);
    fprintf(stderr, "       %s synth OUTPUT SOURCE\n", argv[0]);
    fprintf(stderr, "       %s verify OUTPUT SOURCE\n", argv[0]);
    return 2;
}
