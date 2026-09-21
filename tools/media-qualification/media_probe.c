/* Developer-only native FFmpeg qualification. No application dependency. */
#include <errno.h>
#include <inttypes.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/channel_layout.h>
#include <libavutil/imgutils.h>
#include <libavutil/md5.h>
#include <libavutil/opt.h>
#include <libavutil/pixdesc.h>

#define WIDTH 320
#define HEIGHT 180
#define FRAME_COUNT 120
#define SAMPLE_RATE 48000
#define MAX_FRAMES 4096

static void fail(const char *operation, int error) {
    char message[AV_ERROR_MAX_STRING_SIZE];
    av_strerror(error, message, sizeof(message));
    fprintf(stderr, "%s: %s (%d)\n", operation, message, error);
    exit(1);
}

static void check(int status, const char *operation) {
    if (status < 0) fail(operation, status);
}

static void require(int condition, const char *message) {
    if (!condition) {
        fprintf(stderr, "assertion failed: %s\n", message);
        exit(1);
    }
}

static double monotonic_ms(void) {
    struct timespec value;
    require(clock_gettime(CLOCK_MONOTONIC, &value) == 0, "monotonic clock");
    return value.tv_sec * 1000.0 + value.tv_nsec / 1000000.0;
}

/* Escape all control characters so library configuration strings remain JSON. */
static void json_string(const char *value) {
    putchar('"');
    for (const unsigned char *p = (const unsigned char *)value; *p; p++) {
        if (*p == '"' || *p == '\\') printf("\\%c", *p);
        else if (*p < 0x20) printf("\\u%04x", *p);
        else putchar(*p);
    }
    putchar('"');
}

static void inventory(void) {
    printf("{\"ffmpeg\":"); json_string(av_version_info());
    printf(",\"avcodec\":%u,\"avformat\":%u,\"avutil\":%u,\"configuration\":",
           avcodec_version(), avformat_version(), avutil_version());
    json_string(avcodec_configuration());
    printf(",\"license\":"); json_string(avcodec_license());
    printf("}\n");
}

static const unsigned char digits[10][7] = {
    {14,17,19,21,25,17,14}, {4,12,4,4,4,4,14}, {14,17,1,2,4,8,31},
    {30,1,1,14,1,1,30}, {2,6,10,18,31,2,2}, {31,16,16,30,1,1,30},
    {14,16,16,30,17,17,14}, {31,1,2,4,8,8,8}, {14,17,17,14,17,17,14},
    {14,17,17,15,1,1,14}
};

static void rectangle(AVFrame *frame, int x0, int y0, int width, int height,
                      unsigned char y, unsigned char u, unsigned char v) {
    for (int row = y0; row < y0 + height; row++)
        memset(frame->data[0] + row * frame->linesize[0] + x0, y, (size_t)width);
    for (int row = y0 / 2; row < (y0 + height) / 2; row++) {
        memset(frame->data[1] + row * frame->linesize[1] + x0 / 2, u, (size_t)width / 2);
        memset(frame->data[2] + row * frame->linesize[2] + x0 / 2, v, (size_t)width / 2);
    }
}

static void draw_frame(AVFrame *frame, int number) {
    /* Limited-range BT.709 YUV patches: neutral black/white and R/G/B. */
    static const unsigned char patches[5][3] = {
        {16,128,128}, {235,128,128}, {63,102,240}, {173,42,26}, {32,240,118}
    };
    check(av_frame_make_writable(frame), "make video writable");
    rectangle(frame, 0, 0, WIDTH, HEIGHT, number < 60 ? 40 : 90, 128, 128);
    for (int p = 0; p < 5; p++)
        rectangle(frame, p * 64, 120, 64, 60, patches[p][0], patches[p][1], patches[p][2]);
    rectangle(frame, (number * 2) % (WIDTH - 24), 88, 24, 24, 210, 70, 200);
    for (int place = 0, divisor = 100; place < 3; place++, divisor /= 10) {
        int digit = (number / divisor) % 10;
        for (int row = 0; row < 7; row++)
            for (int col = 0; col < 5; col++)
                if (digits[digit][row] & (1 << (4 - col)))
                    rectangle(frame, 96 + place * 42 + col * 6, 16 + row * 6, 6, 6, 235, 128, 128);
    }
}

/* Read the visible authored number from decoded pixels, independently of PTS
   and the decoder's own linear-frame hashes. Cell centers avoid glyph edges. */
static int read_frame_number(const AVFrame *frame) {
    require(frame->format == AV_PIX_FMT_YUV420P && frame->width == WIDTH && frame->height == HEIGHT,
            "frame identity fixture geometry and pixel format");
    int number = 0;
    for (int place = 0; place < 3; place++) {
        unsigned char rows[7] = {0};
        for (int row = 0; row < 7; row++) {
            for (int col = 0; col < 5; col++) {
                int x = 96 + place * 42 + col * 6 + 3, y = 16 + row * 6 + 3;
                if (frame->data[0][y * frame->linesize[0] + x] > 162)
                    rows[row] |= (unsigned char)(1 << (4 - col));
            }
        }
        int recognized = -1;
        for (int digit = 0; digit < 10; digit++)
            if (memcmp(rows, digits[digit], sizeof(rows)) == 0) recognized = digit;
        require(recognized >= 0, "decoded visible frame number is readable");
        number = number * 10 + recognized;
    }
    return number;
}

static void write_packets(AVFormatContext *format, AVCodecContext *codec, AVStream *stream) {
    AVPacket *packet = av_packet_alloc();
    require(packet != NULL, "allocate encode packet");
    int status;
    while ((status = avcodec_receive_packet(codec, packet)) >= 0) {
        av_packet_rescale_ts(packet, codec->time_base, stream->time_base);
        packet->stream_index = stream->index;
        check(av_interleaved_write_frame(format, packet), "mux packet");
        av_packet_unref(packet);
    }
    require(status == AVERROR(EAGAIN) || status == AVERROR_EOF, "encoder receive terminates normally");
    av_packet_free(&packet);
}

static AVCodecContext *encoder(AVFormatContext *output, const char *name, int audio,
                               int require_software, int b_frames, AVStream **stream) {
    const AVCodec *implementation = avcodec_find_encoder_by_name(name);
    require(implementation != NULL, "encoder exists");
    AVCodecContext *codec = avcodec_alloc_context3(implementation);
    require(codec != NULL, "allocate encoder");
    codec->thread_count = 1;
    AVDictionary *options = NULL;
    if (audio) {
        codec->sample_rate = SAMPLE_RATE;
        codec->sample_fmt = AV_SAMPLE_FMT_FLTP;
        av_channel_layout_default(&codec->ch_layout, 2);
        codec->time_base = (AVRational){1, SAMPLE_RATE};
        codec->bit_rate = 384000;
    } else {
        codec->width = WIDTH;
        codec->height = HEIGHT;
        codec->pix_fmt = AV_PIX_FMT_YUV420P;
        codec->time_base = (AVRational){1, 30000};
        codec->framerate = (AVRational){30000, 1001};
        codec->gop_size = 15;
        codec->max_b_frames = b_frames;
        codec->bit_rate = 2000000;
        codec->color_range = AVCOL_RANGE_MPEG;
        codec->color_primaries = AVCOL_PRI_BT709;
        codec->color_trc = AVCOL_TRC_BT709;
        codec->colorspace = AVCOL_SPC_BT709;
        codec->flags |= AV_CODEC_FLAG_CLOSED_GOP | AV_CODEC_FLAG_FRAME_DURATION;
        if (strcmp(name, "libx264") == 0) {
            check(av_dict_set(&options, "preset", "medium", 0), "x264 preset");
            check(av_dict_set(&options, "x264-params", "b-adapt=0:scenecut=0:keyint=15:min-keyint=15", 0), "x264 options");
        } else if (strcmp(name, "h264_videotoolbox") == 0) {
            /* No implicit fallback: separate hardware and OS software runs. */
            check(av_dict_set(&options, "allow_sw", require_software ? "1" : "0", 0), "VT fallback");
            check(av_dict_set(&options, "require_sw", require_software ? "1" : "0", 0), "VT software");
            check(av_dict_set(&options, "profile", "high", 0), "VT profile");
        }
    }
    if (output->oformat->flags & AVFMT_GLOBALHEADER) codec->flags |= AV_CODEC_FLAG_GLOBAL_HEADER;
    check(avcodec_open2(codec, implementation, &options), "open encoder");
    require(av_dict_count(options) == 0, "all encoder options consumed");
    av_dict_free(&options);
    *stream = avformat_new_stream(output, NULL);
    require(*stream != NULL, "allocate stream");
    (*stream)->time_base = codec->time_base;
    if (!audio) (*stream)->avg_frame_rate = codec->framerate;
    check(avcodec_parameters_from_context((*stream)->codecpar, codec), "encoder stream parameters");
    return codec;
}

static void encode(const char *path, const char *name, const char *cadence, const char *mode, int swap_content) {
    require(strcmp(mode, "hardware") == 0 || strcmp(mode, "software") == 0 ||
            strcmp(mode, "hardware-no-b") == 0 || strcmp(mode, "software-no-b") == 0, "known encoder mode");
    int require_software = strstr(mode, "software") != NULL;
    int b_frames = strstr(mode, "no-b") ? 0 : 2;
    int vfr = strcmp(cadence, "vfr") == 0;
    int64_t offset = strcmp(cadence, "offset") == 0 ? 60060 : 0;
    require(vfr || offset || strcmp(cadence, "cfr") == 0, "known cadence");
    int64_t durations[FRAME_COUNT], total_ticks = 0;
    for (int n = 0; n < FRAME_COUNT; n++) {
        durations[n] = 1001 * (vfr ? 1 + (n % 3) : 1);
        total_ticks += durations[n];
    }
    AVFormatContext *output = NULL;
    check(avformat_alloc_output_context2(&output, NULL, "mp4", path), "allocate muxer");
    require(output != NULL, "muxer allocated");
    output->avoid_negative_ts = AVFMT_AVOID_NEG_TS_DISABLED;
    AVStream *video_stream = NULL, *audio_stream = NULL;
    AVCodecContext *video = encoder(output, name, 0, require_software, b_frames, &video_stream);
    AVCodecContext *audio = encoder(output, "aac", 1, 0, 0, &audio_stream);
    check(avio_open(&output->pb, path, AVIO_FLAG_WRITE), "open output");
    AVDictionary *mux_options = NULL;
    check(av_dict_set(&mux_options, "movflags", "+faststart", 0), "MP4 faststart");
    check(avformat_write_header(output, &mux_options), "write header");
    require(av_dict_count(mux_options) == 0, "all mux options consumed");
    av_dict_free(&mux_options);
    AVFrame *picture = av_frame_alloc(), *sound = av_frame_alloc();
    require(picture && sound, "allocate encoding frames");
    picture->width = WIDTH;
    picture->height = HEIGHT;
    picture->format = video->pix_fmt;
    picture->color_range = video->color_range;
    picture->color_primaries = video->color_primaries;
    picture->color_trc = video->color_trc;
    picture->colorspace = video->colorspace;
    check(av_frame_get_buffer(picture, 32), "allocate picture planes");
    sound->format = audio->sample_fmt;
    sound->sample_rate = SAMPLE_RATE;
    check(av_channel_layout_copy(&sound->ch_layout, &audio->ch_layout), "copy audio layout");
    sound->nb_samples = audio->frame_size;
    check(av_frame_get_buffer(sound, 0), "allocate audio samples");
    int64_t audio_count = av_rescale_q(total_ticks, video->time_base, audio->time_base);
    int64_t audio_offset = av_rescale_q(offset, video->time_base, audio->time_base);
    int64_t impulses[] = {100, SAMPLE_RATE, audio_count - 200};
    int64_t video_position = 0, audio_position = 0;
    int frame_number = 0;
    double started = monotonic_ms();
    while (frame_number < FRAME_COUNT || audio_position < audio_count) {
        if (frame_number < FRAME_COUNT &&
            (audio_position >= audio_count || av_compare_ts(video_position, video->time_base,
                                                            audio_position, audio->time_base) <= 0)) {
            int content_number = swap_content && frame_number == 10 ? 11 :
                                 swap_content && frame_number == 11 ? 10 : frame_number;
            draw_frame(picture, content_number);
            picture->pts = offset + video_position;
            picture->duration = durations[frame_number];
            check(avcodec_send_frame(video, picture), "send video");
            write_packets(output, video, video_stream);
            video_position += durations[frame_number++];
        } else {
            check(av_frame_make_writable(sound), "make audio writable");
            sound->nb_samples = (int)FFMIN(audio->frame_size, audio_count - audio_position);
            sound->pts = audio_offset + audio_position;
            for (int channel = 0; channel < 2; channel++) {
                float *samples = (float *)sound->extended_data[channel];
                for (int n = 0; n < sound->nb_samples; n++) {
                    samples[n] = 0;
                    for (int event = 0; event < 3; event++)
                        if (audio_position + n == impulses[event]) samples[n] = channel ? -0.65f : 0.75f;
                }
            }
            check(avcodec_send_frame(audio, sound), "send audio");
            write_packets(output, audio, audio_stream);
            audio_position += sound->nb_samples;
        }
    }
    check(avcodec_send_frame(video, NULL), "drain video");
    write_packets(output, video, video_stream);
    check(avcodec_send_frame(audio, NULL), "drain audio");
    write_packets(output, audio, audio_stream);
    check(av_write_trailer(output), "write trailer");
    printf("{\"encoder\":"); json_string(name);
    printf(",\"require_software\":%s,\"cadence\":", require_software ? "true" : "false"); json_string(cadence);
    printf(",\"frame_count\":%d,\"time_base\":[1,30000],\"start_pts\":%"PRId64
           ",\"duration_ticks\":%"PRId64",\"audio_samples\":%"PRId64
           ",\"audio_offset_samples\":%"PRId64",\"impulses\":[%"PRId64",%"PRId64",%"PRId64"]"
           ",\"audio_encoder_initial_padding\":%d,\"requested_b_frames\":%d,\"encode_ms\":%.3f}\n", FRAME_COUNT, offset, total_ticks, audio_count,
           audio_offset, impulses[0], impulses[1], impulses[2], audio->initial_padding, b_frames, monotonic_ms() - started);
    av_frame_free(&picture);
    av_frame_free(&sound);
    avcodec_free_context(&video);
    avcodec_free_context(&audio);
    check(avio_closep(&output->pb), "close output");
    avformat_free_context(output);
}

typedef struct {
    AVFormatContext *format;
    AVCodecContext *codec;
    AVPacket *packet;
    AVFrame *frame;
    int stream_index;
    int draining;
} Decoder;

static Decoder open_decoder(const char *path, enum AVMediaType type) {
    Decoder decoder = {0};
    check(avformat_open_input(&decoder.format, path, NULL, NULL), "open input");
    check(avformat_find_stream_info(decoder.format, NULL), "probe streams");
    decoder.stream_index = av_find_best_stream(decoder.format, type, -1, -1, NULL, 0);
    check(decoder.stream_index, "find stream");
    const AVCodecParameters *parameters = decoder.format->streams[decoder.stream_index]->codecpar;
    const AVCodec *codec = avcodec_find_decoder(parameters->codec_id);
    require(codec != NULL, "decoder exists");
    decoder.codec = avcodec_alloc_context3(codec);
    require(decoder.codec != NULL, "allocate decoder");
    check(avcodec_parameters_to_context(decoder.codec, parameters), "copy decoder parameters");
    decoder.codec->thread_count = 1;
    check(avcodec_open2(decoder.codec, codec, NULL), "open decoder");
    decoder.packet = av_packet_alloc();
    decoder.frame = av_frame_alloc();
    require(decoder.packet && decoder.frame, "allocate decoder buffers");
    return decoder;
}

static int next_frame(Decoder *decoder) {
    for (;;) {
        int status = avcodec_receive_frame(decoder->codec, decoder->frame);
        if (status >= 0) return 1;
        if (status == AVERROR_EOF) return 0;
        require(status == AVERROR(EAGAIN), "decoder receive status");
        require(!decoder->draining, "drained decoder does not request input");
        do {
            status = av_read_frame(decoder->format, decoder->packet);
            if (status == AVERROR_EOF) {
                check(avcodec_send_packet(decoder->codec, NULL), "drain decoder");
                decoder->draining = 1;
                break;
            }
            check(status, "read packet");
            if (decoder->packet->stream_index == decoder->stream_index) {
                check(avcodec_send_packet(decoder->codec, decoder->packet), "send packet");
                av_packet_unref(decoder->packet);
                break;
            }
            av_packet_unref(decoder->packet);
        } while (1);
    }
}

static void close_decoder(Decoder *decoder) {
    av_packet_free(&decoder->packet);
    av_frame_free(&decoder->frame);
    avcodec_free_context(&decoder->codec);
    avformat_close_input(&decoder->format);
}

static void frame_hash(const AVFrame *frame, unsigned char hash[16]) {
    int size = av_image_get_buffer_size(frame->format, frame->width, frame->height, 1);
    check(size, "image buffer size");
    unsigned char *buffer = av_malloc((size_t)size);
    require(buffer != NULL, "allocate packed frame");
    int copied = av_image_copy_to_buffer(buffer, size, (const uint8_t *const *)frame->data,
                                         frame->linesize, frame->format, frame->width, frame->height, 1);
    check(copied, "pack image planes");
    require(copied == size, "complete image copied");
    av_md5_sum(hash, buffer, (size_t)size);
    av_free(buffer);
}

typedef struct { int64_t pts, duration; unsigned char hash[16]; } FrameRecord;

static void probe_video(const char *path) {
    Decoder decoder = open_decoder(path, AVMEDIA_TYPE_VIDEO);
    AVStream *stream = decoder.format->streams[decoder.stream_index];
    FrameRecord records[MAX_FRAMES];
    AVFrame *retained = NULL;
    int patches[5][3] = {{0}};
    int count = 0, b_frames = 0, keys = 0;
    double started = monotonic_ms();
    printf("{\"time_base\":[%d,%d],\"stream_start_pts\":%"PRId64
           ",\"stream_duration\":%"PRId64",\"frames\":[",
           stream->time_base.num, stream->time_base.den, stream->start_time, stream->duration);
    while (next_frame(&decoder)) {
        require(count < MAX_FRAMES, "bounded qualification frame count");
        FrameRecord *record = &records[count];
        record->pts = decoder.frame->best_effort_timestamp;
        record->duration = decoder.frame->duration;
        require(record->pts != AV_NOPTS_VALUE, "decoded PTS exists");
        require(decoder.frame->decode_error_flags == 0, "no concealed decode errors");
        require(count == 0 || record->pts > records[count - 1].pts, "strict presentation ordering");
        int authored_identity = read_frame_number(decoder.frame);
        if (authored_identity != count)
            fprintf(stderr, "frame identity mismatch: decoded=%d expected=%d pts=%"PRId64"\n",
                    authored_identity, count, record->pts);
        require(authored_identity == count, "decoded frame identity matches authored index");
        frame_hash(decoder.frame, record->hash);
        if (count == 0) {
            retained = av_frame_clone(decoder.frame);
            require(retained != NULL, "retain decoded frame");
            require(decoder.frame->format == AV_PIX_FMT_YUV420P, "fixture decoded pixel format");
            for (int patch = 0; patch < 5; patch++) {
                int x = patch * 64 + 32, y = 150;
                patches[patch][0] = decoder.frame->data[0][y * decoder.frame->linesize[0] + x];
                patches[patch][1] = decoder.frame->data[1][(y / 2) * decoder.frame->linesize[1] + x / 2];
                patches[patch][2] = decoder.frame->data[2][(y / 2) * decoder.frame->linesize[2] + x / 2];
            }
        }
        if (count) putchar(',');
        printf("{\"pts\":%"PRId64",\"duration\":%"PRId64",\"authored_identity\":%d,\"keyframe\":%s,\"type\":\"%c\",\"md5\":\"",
               record->pts, record->duration, authored_identity, (decoder.frame->flags & AV_FRAME_FLAG_KEY) ? "true" : "false",
               av_get_picture_type_char(decoder.frame->pict_type));
        for (int byte = 0; byte < 16; byte++) printf("%02x", record->hash[byte]);
        printf("\"}");
        b_frames += decoder.frame->pict_type == AV_PICTURE_TYPE_B;
        keys += !!(decoder.frame->flags & AV_FRAME_FLAG_KEY);
        count++;
        av_frame_unref(decoder.frame);
    }
    require(count > 0, "video contains frames");
    double linear_ms = monotonic_ms() - started;
    /* Deterministic nonmonotonic requests on the SAME demuxer and decoder.
       Decode through earlier B-frames until the exact requested PTS is found. */
    int requests[] = {0, count - 1, 1, count / 2, 14, 16, 29, 30, 61, 5, count - 2, 0};
    printf("],\"frame_count\":%d,\"b_frames\":%d,\"keyframes\":%d,\"linear_decode_ms\":%.3f,\"seeks\":[",
           count, b_frames, keys, linear_ms);
    size_t prefix_count = sizeof(requests) / sizeof(requests[0]);
    for (size_t n = 0; n < prefix_count + (size_t)count; n++) {
        int target = n < prefix_count ? requests[n] % count : count - 1 - (int)(n - prefix_count);
        int64_t pts = records[target].pts;
        started = monotonic_ms();
        check(avformat_seek_file(decoder.format, decoder.stream_index, INT64_MIN, pts, pts,
                                 AVSEEK_FLAG_BACKWARD), "seek backward");
        avcodec_flush_buffers(decoder.codec);
        av_frame_unref(decoder.frame);
        av_packet_unref(decoder.packet);
        decoder.draining = 0;
        int found = 0, decoded = 0;
        while (next_frame(&decoder)) {
            decoded++;
            int64_t actual = decoder.frame->best_effort_timestamp;
            require(actual <= pts, "seek did not skip target");
            if (actual == pts) {
                unsigned char hash[16];
                frame_hash(decoder.frame, hash);
                require(memcmp(hash, records[target].hash, 16) == 0, "seek frame equals linear decode");
                found = 1;
                av_frame_unref(decoder.frame);
                break;
            }
            av_frame_unref(decoder.frame);
        }
        require(found, "seek found exact requested PTS");
        if (n) putchar(',');
        printf("{\"frame\":%d,\"pts\":%"PRId64",\"decoded_preroll_frames\":%d,\"ms\":%.3f}",
               target, pts, decoded, monotonic_ms() - started);
    }
    unsigned char retained_hash[16];
    frame_hash(retained, retained_hash);
    require(memcmp(retained_hash, records[0].hash, 16) == 0, "retained frame survives decode and seek flush");
    printf("],\"retained_frame_survived_flush\":true,\"first_frame_yuv_patches\":[");
    for (int p = 0; p < 5; p++) {
        if (p) putchar(',');
        printf("[%d,%d,%d]", patches[p][0], patches[p][1], patches[p][2]);
    }
    printf("]}\n");
    av_frame_free(&retained);
    close_decoder(&decoder);
}

static void probe_audio(const char *path, const char *pcm_path) {
    Decoder decoder = open_decoder(path, AVMEDIA_TYPE_AUDIO);
    AVStream *stream = decoder.format->streams[decoder.stream_index];
    FILE *pcm = fopen(pcm_path, "wb");
    require(pcm != NULL, "open PCM output");
    int64_t count = 0, first_pts = AV_NOPTS_VALUE, next_sample = AV_NOPTS_VALUE;
    int frames = 0;
    while (next_frame(&decoder)) {
        AVFrame *frame = decoder.frame;
        require(frame->format == AV_SAMPLE_FMT_FLTP, "fixture audio decodes as planar float");
        require(frame->sample_rate == SAMPLE_RATE && frame->ch_layout.nb_channels == 2, "48 kHz stereo decode");
        require(frame->best_effort_timestamp != AV_NOPTS_VALUE, "audio PTS exists");
        int64_t sample = av_rescale_q(frame->best_effort_timestamp, stream->time_base, (AVRational){1, SAMPLE_RATE});
        if (frames == 0) first_pts = sample;
        else require(sample == next_sample, "audio frames contiguous in exact sample coordinates");
        for (int n = 0; n < frame->nb_samples; n++) {
            float pair[2] = {((float *)frame->extended_data[0])[n], ((float *)frame->extended_data[1])[n]};
            require(isfinite(pair[0]) && isfinite(pair[1]), "finite decoded PCM");
            require(fwrite(pair, sizeof(float), 2, pcm) == 2, "write PCM");
        }
        next_sample = sample + frame->nb_samples;
        count += frame->nb_samples;
        frames++;
        av_frame_unref(frame);
    }
    require(frames > 0, "audio contains frames");
    require(fclose(pcm) == 0, "close PCM");
    printf("{\"time_base\":[%d,%d],\"stream_start_pts\":%"PRId64
           ",\"stream_duration\":%"PRId64",\"first_sample_pts\":%"PRId64
           ",\"decoded_samples\":%"PRId64",\"decoded_frames\":%d,\"sample_rate\":%d}\n",
           stream->time_base.num, stream->time_base.den, stream->start_time, stream->duration,
           first_pts, count, frames, SAMPLE_RATE);
    close_decoder(&decoder);
}

int main(int argc, char **argv) {
    av_log_set_level(AV_LOG_WARNING);
    if (argc == 2 && strcmp(argv[1], "inventory") == 0) inventory();
    else if (argc == 6 && strcmp(argv[1], "encode") == 0)
        encode(argv[2], argv[3], argv[4], argv[5], 0);
    else if (argc == 6 && strcmp(argv[1], "encode-swap") == 0)
        encode(argv[2], argv[3], argv[4], argv[5], 1);
    else if (argc == 3 && strcmp(argv[1], "video") == 0) probe_video(argv[2]);
    else if (argc == 4 && strcmp(argv[1], "audio") == 0) probe_audio(argv[2], argv[3]);
    else {
        fprintf(stderr, "usage: media_probe inventory | encode|encode-swap OUTPUT ENCODER cfr|vfr|offset hardware|software|hardware-no-b|software-no-b | video INPUT | audio INPUT PCM_OUTPUT\n");
        return 2;
    }
    return 0;
}
