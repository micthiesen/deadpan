/* Developer-only CFR encoder experiment. No product runtime entrypoint. */
#define _POSIX_C_SOURCE 200809L
#define main original_fixture_main
#include "../media_probe.c"
#undef main

#include <fcntl.h>
#include <limits.h>
#include <sys/stat.h>
#include <unistd.h>
#include <libavutil/intreadwrite.h>
#include <libavutil/sha.h>

#define EXPORT_FRAME_LIMIT 240
#define EXPORT_SAMPLE_LIMIT INT64_C(5760000)
#define EXPORT_PACKET_LIMIT 16384
#define EXPORT_PACKET_BYTES (16 * 1024 * 1024)
#define EXPORT_INPUT_BYTES INT64_C(536870912)
#define EXPORT_AUDIO_FRAME_LIMIT 8192

typedef struct {
    int fps_num, fps_den, frames, software, b_frames, disable_edits, edges;
    int gop, movie_timescale;
    int64_t duration, samples, impulses[3];
    const char *mode, *edits, *pcm;
} ExportCase;

static const double neutral_levels[5] = {0.001, 0.01, 0.018, 0.18, 0.5};

static int argument(const char *text, int maximum) {
    require(text[0] >= '0' && text[0] <= '9', "unsigned decimal argument");
    errno = 0;
    char *end = NULL;
    unsigned long value = strtoul(text, &end, 10);
    require(errno == 0 && end && *end == 0 && value > 0 && value <= (unsigned long)maximum,
            "numeric argument within bound");
    return (int)value;
}

static int64_t gcd_positive(int64_t a, int64_t b) {
    while (b) { int64_t next = a % b; a = b; b = next; }
    return a;
}

static int64_t round_even_positive(int64_t numerator, int64_t denominator) {
    require(numerator >= 0 && denominator > 0, "nonnegative sample boundary");
    int64_t quotient = numerator / denominator, remainder = numerator % denominator;
    return quotient + (remainder > denominator - remainder ||
                       (remainder == denominator - remainder && (quotient & 1)));
}

static ExportCase parse_case(char **argv) {
    ExportCase value = {0};
    value.mode = argv[4]; value.edits = argv[5]; value.pcm = argv[9];
    require(strcmp(value.mode, "hardware-no-b") == 0 || strcmp(value.mode, "software-no-b") == 0 ||
            strcmp(value.mode, "hardware-b") == 0 || strcmp(value.mode, "software-b") == 0,
            "explicit VideoToolbox mode");
    value.software = strncmp(value.mode, "software", 8) == 0;
    value.b_frames = strstr(value.mode, "no-b") ? 0 : 2;
    require(strcmp(value.edits, "default") == 0 || strcmp(value.edits, "disabled") == 0,
            "explicit edit-list mode");
    value.disable_edits = strcmp(value.edits, "disabled") == 0;
    require(strcmp(value.pcm, "impulses") == 0 || strcmp(value.pcm, "edges") == 0,
            "explicit PCM fixture");
    value.edges = strcmp(value.pcm, "edges") == 0;
    value.fps_num = argument(argv[6], 60000000);
    value.fps_den = argument(argv[7], 1000000);
    value.frames = argument(argv[8], EXPORT_FRAME_LIMIT);
    require(gcd_positive(value.fps_num, value.fps_den) == 1, "reduced frame rate");
    require(value.fps_num >= value.fps_den && (int64_t)value.fps_num <= 60 * (int64_t)value.fps_den,
            "frame rate in 1..60");
    value.duration = (int64_t)value.frames * value.fps_den;
    require(value.duration <= 120 * (int64_t)value.fps_num, "at most 120 seconds of authored video");
    value.samples = round_even_positive(value.duration * SAMPLE_RATE, value.fps_num);
    require(value.samples >= 800 && value.samples <= EXPORT_SAMPLE_LIMIT, "bounded authored PCM duration");
    int64_t movie = value.fps_num / gcd_positive(value.fps_num, SAMPLE_RATE) * SAMPLE_RATE;
    require(movie <= INT_MAX, "movie timescale fits signed32");
    value.movie_timescale = (int)movie;
    value.gop = (value.fps_num + value.fps_den) / (2 * value.fps_den);
    if (value.gop < 1) value.gop = 1;
    value.impulses[0] = 100;
    value.impulses[1] = FFMIN(SAMPLE_RATE, value.samples / 2);
    value.impulses[2] = value.samples - 200;
    return value;
}

static void timestamp(FILE *out, int64_t value) {
    if (value == AV_NOPTS_VALUE) fputs("null", out);
    else fprintf(out, "%"PRId64, value);
}

static void named(FILE *out, const char *value) {
    json_string_to(out, value ? value : "unknown");
}

static void hash256(FILE *out, const uint8_t *data, size_t size) {
    struct AVSHA *sha = av_sha_alloc();
    require(sha != NULL, "allocate packet hash");
    check(av_sha_init(sha, 256), "initialize packet hash");
    if (size) av_sha_update(sha, data, size);
    uint8_t digest[32];
    av_sha_final(sha, digest);
    av_free(sha);
    fputc('"', out);
    for (int i = 0; i < 32; i++) fprintf(out, "%02x", digest[i]);
    fputc('"', out);
}

static void skip_json(FILE *out, const uint8_t *data, size_t size) {
    if (!data) { require(size == 0, "absent skip data has zero length"); fputs("null", out); return; }
    require(size == 10, "skip data has exactly ten bytes");
    fprintf(out, "{\"leading\":%u,\"trailing\":%u,\"leading_reason\":%u,\"trailing_reason\":%u}",
            AV_RL32(data), AV_RL32(data + 4), (unsigned)data[8], (unsigned)data[9]);
}

static void avc_json(FILE *out, const AVPacket *packet, const AVCodecParameters *parameters) {
    if (parameters->codec_id != AV_CODEC_ID_H264) { fputs("null", out); return; }
    if (parameters->extradata_size < 5 || !parameters->extradata || parameters->extradata[0] != 1) {
        fputs("{\"available\":false,\"reason\":\"no_avcc_length_size\"}", out); return;
    }
    unsigned width = (parameters->extradata[4] & 3) + 1;
    if (width == 3) {
        fputs("{\"available\":false,\"reason\":\"reserved_avcc_length_size\"}", out); return;
    }
    unsigned types[64], count = 0;
    size_t position = 0, length = (size_t)packet->size;
    int valid = 1, idr = 0;
    while (position < length) {
        if (length - position < width || count == 64) { valid = 0; break; }
        uint32_t bytes = 0;
        for (unsigned i = 0; i < width; i++) bytes = (bytes << 8) | packet->data[position++];
        if (bytes == 0 || bytes > length - position) { valid = 0; break; }
        types[count++] = packet->data[position] & 31;
        idr |= types[count - 1] == 5;
        position += bytes;
    }
    if (!valid || count == 0) {
        fputs("{\"available\":false,\"reason\":\"invalid_or_over_limit_avc_packet\"}", out); return;
    }
    fprintf(out, "{\"available\":true,\"length_size\":%u,\"idr\":%s,\"nal_types\":[", width, idr ? "true" : "false");
    for (unsigned i = 0; i < count; i++) fprintf(out, "%s%u", i ? "," : "", types[i]);
    fputs("]}", out);
}

static void packet_json(FILE *out, const AVPacket *packet, const AVStream *stream,
                        unsigned ordinal, const AVCodecContext *encoder_context,
                        int64_t original_pts, int64_t original_dts, int64_t original_duration) {
    require(packet->size >= 0 && packet->size <= EXPORT_PACKET_BYTES, "bounded packet payload");
    require(packet->size == 0 || packet->data != NULL, "packet payload exists");
    fprintf(out, "{\"kind\":\"packet\",\"stage\":\"%s\",\"ordinal\":%u,\"stream_index\":%d,\"codec_type\":",
            encoder_context ? "before_mux" : "demux", ordinal, stream->index);
    named(out, av_get_media_type_string(stream->codecpar->codec_type));
    fprintf(out, ",\"time_base\":[%d,%d]", stream->time_base.num, stream->time_base.den);
    if (encoder_context) {
        fprintf(out, ",\"codec_time_base\":[%d,%d],\"encoder\":{\"pts\":",
                encoder_context->time_base.num, encoder_context->time_base.den);
        timestamp(out, original_pts); fputs(",\"dts\":", out); timestamp(out, original_dts);
        fprintf(out, ",\"duration\":%"PRId64"}", original_duration);
    }
    fputs(",\"pts\":", out); timestamp(out, packet->pts);
    fputs(",\"dts\":", out); timestamp(out, packet->dts);
    fprintf(out, ",\"duration\":%"PRId64",\"flags\":%d,\"keyframe\":%s,\"size\":%d,\"sha256\":",
            packet->duration, packet->flags, packet->flags & AV_PKT_FLAG_KEY ? "true" : "false", packet->size);
    hash256(out, packet->data, (size_t)packet->size);
    size_t skip_size = 0;
    const uint8_t *skip = av_packet_get_side_data(packet, AV_PKT_DATA_SKIP_SAMPLES, &skip_size);
    fputs(",\"skip\":", out); skip_json(out, skip, skip_size);
    fputs(",\"avc\":", out); avc_json(out, packet, stream->codecpar);
    fputc('}', out);
}

static void export_write_packets(AVFormatContext *output, AVCodecContext *codec,
                                 AVStream *stream, FILE *log, unsigned *count) {
    AVPacket *packet = av_packet_alloc();
    require(packet != NULL, "allocate export packet");
    for (;;) {
        int status = avcodec_receive_packet(codec, packet);
        if (status == AVERROR(EAGAIN) || status == AVERROR_EOF) break;
        check(status, "receive export packet");
        require(*count < EXPORT_PACKET_LIMIT, "encoded packet count cap");
        int64_t pts = packet->pts, dts = packet->dts, duration = packet->duration;
        av_packet_rescale_ts(packet, codec->time_base, stream->time_base);
        packet->stream_index = stream->index;
        packet_json(log, packet, stream, (*count)++, codec, pts, dts, duration);
        fputc('\n', log);
        require(fflush(log) == 0 && !ferror(log), "persist packet observation before mux");
        check(av_interleaved_write_frame(output, packet), "mux export packet");
        av_packet_unref(packet);
    }
    av_packet_free(&packet);
}

static void configure_export_encoder(AVCodecContext *codec, int audio, void *opaque) {
    const ExportCase *value = opaque;
    if (audio) {
        codec->profile = AV_PROFILE_AAC_LOW;
    } else {
        codec->time_base = (AVRational){1, value->fps_num};
        codec->framerate = (AVRational){value->fps_num, value->fps_den};
        codec->gop_size = value->gop;
        codec->sample_aspect_ratio = (AVRational){1, 1};
        codec->field_order = AV_FIELD_PROGRESSIVE;
        codec->chroma_sample_location = AVCHROMA_LOC_LEFT;
    }
}

static void patch_centers(const AVFrame *frame, int y, int output[5][3]) {
    require(frame->format == AV_PIX_FMT_YUV420P && frame->width == WIDTH && frame->height == HEIGHT,
            "fixture planes for patch observation");
    for (int p = 0; p < 5; p++) {
        int x = p * 64 + 32;
        output[p][0] = frame->data[0][y * frame->linesize[0] + x];
        output[p][1] = frame->data[1][(y / 2) * frame->linesize[1] + x / 2];
        output[p][2] = frame->data[2][(y / 2) * frame->linesize[2] + x / 2];
    }
}

static void patch_json(const int patches[5][3]) {
    putchar('[');
    for (int p = 0; p < 5; p++) printf("%s[%d,%d,%d]", p ? "," : "", patches[p][0], patches[p][1], patches[p][2]);
    putchar(']');
}

static void draw_export_frame(AVFrame *picture, int number) {
    draw_frame(picture, number);
    for (int p = 0; p < 5; p++) {
        double linear = neutral_levels[p];
        double encoded = linear < 0.018 ? 4.5 * linear : 1.099 * pow(linear, 0.45) - 0.099;
        int y = (int)floor(16 + 219 * encoded + 0.5);
        require(y >= 16 && y <= 235, "neutral fixture limited range");
        rectangle(picture, p * 64, 64, 64, 16, (unsigned char)y, 128, 128);
    }
}

static float fixture_sample(const ExportCase *value, int64_t sample, int channel) {
    float result = 0;
    if (value->edges && (sample < 2048 || sample >= value->samples - 2048)) {
        uint32_t bits = (uint32_t)sample * UINT32_C(1664525) + UINT32_C(1013904223) + (uint32_t)channel * UINT32_C(2246822519);
        result = ((int)((bits >> 24) & 63) - 32) / 1024.0f;
        if (sample == 0) result = channel ? -0.28125f : 0.3125f;
        if (sample == value->samples - 1) result = channel ? -0.25f : 0.28125f;
    }
    for (int event = 0; event < 3; event++)
        if (sample == value->impulses[event]) result = channel ? -0.65f : 0.75f;
    return result;
}

static void color_json(const AVFrame *frame) {
    printf("\"sample_aspect_ratio\":[%d,%d],\"interlaced\":%s,\"color_range\":",
           frame->sample_aspect_ratio.num, frame->sample_aspect_ratio.den,
           frame->flags & AV_FRAME_FLAG_INTERLACED ? "true" : "false");
    named(stdout, av_color_range_name(frame->color_range));
    fputs(",\"color_space\":", stdout); named(stdout, av_color_space_name(frame->colorspace));
    fputs(",\"color_transfer\":", stdout); named(stdout, av_color_transfer_name(frame->color_trc));
    fputs(",\"color_primaries\":", stdout); named(stdout, av_color_primaries_name(frame->color_primaries));
    fputs(",\"chroma_location\":", stdout); named(stdout, av_chroma_location_name(frame->chroma_location));
}

static void export_encode(char **argv) {
    ExportCase value = parse_case(argv);
    require(strcmp(argv[2], argv[3]) != 0, "distinct output and packet-log paths");
    FILE *log = fopen(argv[3], "wx");
    require(log != NULL, "create private packet log exclusively");
    int reservation = open(argv[2], O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC, 0600);
    require(reservation >= 0, "reserve fresh private export fixture exclusively");
    require(close(reservation) == 0, "close export fixture reservation");
    AVFormatContext *output = NULL;
    check(avformat_alloc_output_context2(&output, NULL, "mp4", argv[2]), "allocate export muxer");
    require(output != NULL, "export muxer allocated");
    output->avoid_negative_ts = AVFMT_AVOID_NEG_TS_DISABLED;
    AVStream *vs = NULL, *as = NULL;
    AVCodecContext *video = configured_encoder(output, "h264_videotoolbox", 0, value.software,
        value.b_frames, &vs, configure_export_encoder, &value);
    AVCodecContext *audio = configured_encoder(output, "aac", 1, 0, 0, &as, configure_export_encoder, &value);
    require(video->width == WIDTH && video->height == HEIGHT && video->pix_fmt == AV_PIX_FMT_YUV420P &&
            av_cmp_q(video->time_base, (AVRational){1, value.fps_num}) == 0,
            "encoder retains authored picture layout and clock");
    require(audio->sample_rate == SAMPLE_RATE && audio->sample_fmt == AV_SAMPLE_FMT_FLTP &&
            audio->ch_layout.order == AV_CHANNEL_ORDER_NATIVE && audio->ch_layout.u.mask == AV_CH_LAYOUT_STEREO &&
            av_cmp_q(audio->time_base, (AVRational){1, SAMPLE_RATE}) == 0,
            "encoder retains authored stereo PCM layout and clock");
    require(audio->frame_size > 0 && audio->frame_size <= 8192, "bounded AAC input frame size");
    check(avio_open(&output->pb, argv[2], AVIO_FLAG_WRITE), "open export fixture");
    AVDictionary *options = NULL;
    check(av_dict_set(&options, "movflags", "+faststart", 0), "faststart option");
    check(av_dict_set_int(&options, "movie_timescale", value.movie_timescale, 0), "exact movie timescale");
    if (value.disable_edits) check(av_dict_set(&options, "use_editlist", "0", 0), "disable edit lists explicitly");
    check(avformat_write_header(output, &options), "write export header");
    require(av_dict_count(options) == 0, "all export mux options consumed");
    av_dict_free(&options);
    AVFrame *picture = av_frame_alloc(), *sound = av_frame_alloc();
    require(picture && sound, "allocate export input frames");
    picture->width = WIDTH; picture->height = HEIGHT; picture->format = video->pix_fmt;
    picture->sample_aspect_ratio = (AVRational){1, 1};
    picture->color_range = AVCOL_RANGE_MPEG; picture->color_primaries = AVCOL_PRI_BT709;
    picture->color_trc = AVCOL_TRC_BT709; picture->colorspace = AVCOL_SPC_BT709;
    picture->chroma_location = AVCHROMA_LOC_LEFT;
    picture->flags &= ~(AV_FRAME_FLAG_INTERLACED | AV_FRAME_FLAG_TOP_FIELD_FIRST);
    check(av_frame_get_buffer(picture, 32), "allocate export picture planes");
    sound->format = audio->sample_fmt; sound->sample_rate = SAMPLE_RATE;
    check(av_channel_layout_copy(&sound->ch_layout, &audio->ch_layout), "copy export stereo layout");
    sound->nb_samples = audio->frame_size;
    check(av_frame_get_buffer(sound, 0), "allocate export PCM block");
    int colors[5][3] = {{0}}, neutrals[5][3] = {{0}};
    int frame = 0; int64_t position = 0;
    unsigned packets = 0;
    double started = monotonic_ms();
    while (frame < value.frames || position < value.samples) {
        int64_t video_pts = (int64_t)frame * value.fps_den;
        if (frame < value.frames && (position >= value.samples ||
            av_compare_ts(video_pts, video->time_base, position, audio->time_base) <= 0)) {
            draw_export_frame(picture, frame);
            if (frame == 0) { patch_centers(picture, 150, colors); patch_centers(picture, 72, neutrals); }
            picture->pts = video_pts; picture->duration = value.fps_den;
            check(avcodec_send_frame(video, picture), "send export picture");
            export_write_packets(output, video, vs, log, &packets);
            frame++;
        } else {
            check(av_frame_make_writable(sound), "make export PCM writable");
            sound->nb_samples = (int)FFMIN(audio->frame_size, value.samples - position);
            sound->pts = position;
            for (int channel = 0; channel < 2; channel++)
                for (int n = 0; n < sound->nb_samples; n++)
                    ((float *)sound->extended_data[channel])[n] = fixture_sample(&value, position + n, channel);
            check(avcodec_send_frame(audio, sound), "send export PCM");
            export_write_packets(output, audio, as, log, &packets);
            position += sound->nb_samples;
        }
    }
    check(avcodec_send_frame(video, NULL), "drain export video");
    export_write_packets(output, video, vs, log, &packets);
    check(avcodec_send_frame(audio, NULL), "drain export audio");
    export_write_packets(output, audio, as, log, &packets);
    check(av_write_trailer(output), "finish export fixture");
    require(fclose(log) == 0, "close packet log");
    check(avio_closep(&output->pb), "close export fixture");
    printf("{\"schema_version\":1,\"kind\":\"encode\",\"encoder\":\"h264_videotoolbox\",\"mode\":"); json_string(value.mode);
    fputs(",\"edit_lists\":", stdout); json_string(value.edits);
    fputs(",\"pcm_kind\":", stdout); json_string(value.pcm);
    printf(",\"require_software\":%s,\"width\":%d,\"height\":%d,\"frame_count\":%d,\"frame_rate\":[%d,%d],\"time_base\":[1,%d],"
           "\"start_pts\":0,\"duration_ticks\":%"PRId64",\"audio_samples\":%"PRId64",\"audio_offset_samples\":0,"
           "\"impulses\":[%"PRId64",%"PRId64",%"PRId64"],\"requested_b_frames\":%d,\"requested_gop_frames\":%d,"
           "\"movie_timescale\":%d,\"audio_encoder_initial_padding\":%d,\"audio_encoder_trailing_padding\":%d,"
           "\"audio_encoder_frame_size\":%d,\"packet_count\":%u,\"encode_ms\":%.3f,\"color_yuv_patches\":",
           value.software ? "true" : "false", WIDTH, HEIGHT, value.frames, value.fps_num, value.fps_den, value.fps_num, value.duration, value.samples,
           value.impulses[0], value.impulses[1], value.impulses[2], value.b_frames, value.gop, value.movie_timescale,
           audio->initial_padding, audio->trailing_padding, audio->frame_size, packets, monotonic_ms() - started);
    patch_json(colors);
    fputs(",\"neutral_linear_levels\":[\"0.001\",\"0.01\",\"0.018\",\"0.18\",\"0.5\"],\"neutral_yuv_patches\":", stdout);
    patch_json(neutrals);
    printf(",\"mux_options\":{\"movflags\":\"+faststart\",\"movie_timescale\":%d,\"use_editlist\":%s,"
           "\"avoid_negative_ts\":\"disabled\",\"unconsumed_options\":0}",
           value.movie_timescale, value.disable_edits ? "0" : "null");
    printf(",\"encoder_video\":{\"time_base\":[%d,%d],\"frame_rate\":[%d,%d],\"sample_aspect_ratio\":[%d,%d],"
           "\"profile\":%d,\"gop_size\":%d,\"max_b_frames\":%d,\"field_order\":%d,\"color_range\":%d,"
           "\"color_space\":%d,\"color_transfer\":%d,\"color_primaries\":%d,\"chroma_location\":%d},"
           "\"encoder_audio\":{\"sample_rate\":%d,\"channels\":%d,\"profile\":%d,\"bit_rate\":%"PRId64"}}\n",
           video->time_base.num, video->time_base.den, video->framerate.num, video->framerate.den,
           video->sample_aspect_ratio.num, video->sample_aspect_ratio.den, video->profile, video->gop_size,
           video->max_b_frames, video->field_order, video->color_range, video->colorspace, video->color_trc,
           video->color_primaries, video->chroma_sample_location, audio->sample_rate, audio->ch_layout.nb_channels,
           audio->profile, audio->bit_rate);
    av_frame_free(&picture); av_frame_free(&sound);
    avcodec_free_context(&video); avcodec_free_context(&audio); avformat_free_context(output);
}

static void admit_fixture(const char *path) {
    int descriptor = open(path, O_RDONLY | O_NONBLOCK | O_CLOEXEC);
    require(descriptor >= 0, "open fixture for bounded regular-file admission");
    struct stat info;
    int valid = fstat(descriptor, &info) == 0 && S_ISREG(info.st_mode) && info.st_size > 0 && info.st_size <= EXPORT_INPUT_BYTES;
    require(close(descriptor) == 0, "close fixture admission descriptor");
    require(valid, "fixture is a nonempty regular file within 512MiB");
}

static void decoder_options(AVCodecContext *codec, AVStream *stream) {
    require(stream->time_base.num > 0 && stream->time_base.den > 0, "positive decoded stream time base");
    codec->pkt_timebase = stream->time_base;
    codec->max_pixels = WIDTH * HEIGHT;
    if (codec->codec_type == AVMEDIA_TYPE_VIDEO)
        require(codec->width == WIDTH && codec->height == HEIGHT && codec->codec_id == AV_CODEC_ID_H264,
                "bounded fixture H264 geometry");
    else
        require(codec->sample_rate == SAMPLE_RATE && codec->ch_layout.nb_channels == 2 && codec->codec_id == AV_CODEC_ID_AAC &&
                codec->ch_layout.order == AV_CHANNEL_ORDER_NATIVE && codec->ch_layout.u.mask == AV_CH_LAYOUT_STEREO,
                "fixture AAC rate and channels");
}

static void manual_decoder_options(AVCodecContext *codec, AVStream *stream) {
    decoder_options(codec, stream);
    codec->flags2 |= AV_CODEC_FLAG2_SKIP_MANUAL;
}

static void stream_header(const char *kind, const AVStream *stream) {
    printf("{\"schema_version\":1,\"kind\":"); json_string(kind);
    printf(",\"stream_index\":%d,\"time_base\":[%d,%d],\"stream_start_pts\":",
           stream->index, stream->time_base.num, stream->time_base.den);
    timestamp(stdout, stream->start_time);
    fputs(",\"stream_duration\":", stdout); timestamp(stdout, stream->duration);
}

static void export_video(const char *path) {
    admit_fixture(path);
    Decoder decoder = configured_decoder(path, AVMEDIA_TYPE_VIDEO, decoder_options);
    require(decoder.format->nb_streams <= 8, "bounded stream inventory");
    AVStream *stream = decoder.format->streams[decoder.stream_index];
    stream_header("video", stream);
    printf(",\"stream_sample_aspect_ratio\":[%d,%d],\"codec_parameters_sample_aspect_ratio\":[%d,%d]",
           stream->sample_aspect_ratio.num, stream->sample_aspect_ratio.den,
           stream->codecpar->sample_aspect_ratio.num, stream->codecpar->sample_aspect_ratio.den);
    fputs(",\"frames\":[", stdout);
    unsigned count = 0; uint32_t budget = EXPORT_PACKET_LIMIT;
    int colors[5][3] = {{0}}, neutrals[5][3] = {{0}};
    while (next_frame_bounded(&decoder, &budget)) {
        require(count < EXPORT_FRAME_LIMIT, "decoded fixture frame cap");
        AVFrame *frame = decoder.frame;
        int identity = read_frame_number(frame);
        unsigned char hash[16]; frame_hash(frame, hash);
        if (count == 0) { patch_centers(frame, 150, colors); patch_centers(frame, 72, neutrals); }
        if (count++) putchar(',');
        fputs("{\"pts\":", stdout); timestamp(stdout, frame->pts);
        fputs(",\"best_effort_pts\":", stdout); timestamp(stdout, frame->best_effort_timestamp);
        printf(",\"duration\":%"PRId64",\"authored_identity\":%d,\"keyframe\":%s,\"type\":\"%c\",\"md5\":\"",
               frame->duration, identity, frame->flags & AV_FRAME_FLAG_KEY ? "true" : "false",
               av_get_picture_type_char(frame->pict_type));
        for (int i = 0; i < 16; i++) printf("%02x", hash[i]);
        printf("\",\"decode_error_flags\":%d,\"flags\":%d,", frame->decode_error_flags, frame->flags);
        color_json(frame); putchar('}');
        av_frame_unref(frame);
    }
    require(count > 0, "decoded video has frames");
    printf("],\"frame_count\":%u,\"first_frame_yuv_patches\":", count); patch_json(colors);
    fputs(",\"first_frame_neutral_patches\":", stdout); patch_json(neutrals);
    fputs(",\"gop_evidence\":{\"fresh_decoder_tested\":false,\"reason\":\"first_matrix_reports_packet_and_frame_observations_only\"}}\n", stdout);
    close_decoder(&decoder);
}

static void export_audio(const char *path, const char *pcm_path, const char *mode) {
    require(strcmp(mode, "ordinary") == 0 || strcmp(mode, "manual") == 0, "explicit audio decode mode");
    require(strcmp(path, pcm_path) != 0, "distinct input and PCM paths");
    admit_fixture(path);
    Decoder decoder = configured_decoder(path, AVMEDIA_TYPE_AUDIO,
        strcmp(mode, "manual") == 0 ? manual_decoder_options : decoder_options);
    require(decoder.format->nb_streams <= 8, "bounded stream inventory");
    AVStream *stream = decoder.format->streams[decoder.stream_index];
    FILE *pcm = fopen(pcm_path, "wx");
    require(pcm != NULL, "create private PCM output exclusively");
    stream_header("audio", stream);
    fputs(",\"mode\":", stdout); json_string(mode);
    printf(",\"initial_padding\":%d,\"trailing_padding\":%d,\"seek_preroll\":%d,\"frames\":[",
           stream->codecpar->initial_padding, stream->codecpar->trailing_padding, stream->codecpar->seek_preroll);
    int64_t count = 0, first = AV_NOPTS_VALUE;
    unsigned frames = 0; uint32_t budget = EXPORT_PACKET_LIMIT;
    while (next_frame_bounded(&decoder, &budget)) {
        AVFrame *frame = decoder.frame;
        require(frames < EXPORT_AUDIO_FRAME_LIMIT && frame->nb_samples > 0 && frame->nb_samples <= 8192,
                "bounded decoded audio frames");
        require(count <= EXPORT_SAMPLE_LIMIT + 8192 - frame->nb_samples, "decoded audio sample cap");
        require(frame->format == AV_SAMPLE_FMT_FLTP && frame->sample_rate == SAMPLE_RATE && frame->ch_layout.nb_channels == 2 &&
                frame->ch_layout.order == AV_CHANNEL_ORDER_NATIVE && frame->ch_layout.u.mask == AV_CH_LAYOUT_STEREO,
                "decoded planar-float 48k stereo");
        if (frames == 0 && frame->pts != AV_NOPTS_VALUE)
            first = av_rescale_q(frame->pts, stream->time_base, (AVRational){1, SAMPLE_RATE});
        if (frames++) putchar(',');
        fputs("{\"pts\":", stdout); timestamp(stdout, frame->pts);
        fputs(",\"best_effort_pts\":", stdout); timestamp(stdout, frame->best_effort_timestamp);
        fputs(",\"pkt_dts\":", stdout); timestamp(stdout, frame->pkt_dts);
        printf(",\"duration\":%"PRId64",\"nb_samples\":%d,\"sample_offset\":%"PRId64",\"sample_rate\":%d,"
               "\"channels\":%d,\"channel_layout\":", frame->duration, frame->nb_samples, count,
               frame->sample_rate, frame->ch_layout.nb_channels);
        char layout[128]; check(av_channel_layout_describe(&frame->ch_layout, layout, sizeof(layout)), "describe stereo layout");
        json_string(layout);
        AVFrameSideData *skip = av_frame_get_side_data(frame, AV_FRAME_DATA_SKIP_SAMPLES);
        fputs(",\"skip\":", stdout); skip_json(stdout, skip ? skip->data : NULL, skip ? skip->size : 0);
        printf(",\"discard\":%s,\"decode_error_flags\":%d,\"flags\":%d}",
               frame->flags & AV_FRAME_FLAG_DISCARD ? "true" : "false", frame->decode_error_flags, frame->flags);
        for (int n = 0; n < frame->nb_samples; n++) {
            float pair[2] = {((float *)frame->extended_data[0])[n], ((float *)frame->extended_data[1])[n]};
            require(isfinite(pair[0]) && isfinite(pair[1]), "finite decoded fixture PCM");
            require(fwrite(pair, sizeof(float), 2, pcm) == 2, "write decoded fixture PCM");
        }
        count += frame->nb_samples;
        av_frame_unref(frame);
    }
    require(frames > 0, "decoded audio has frames");
    require(fclose(pcm) == 0, "close decoded fixture PCM");
    fputs("],\"first_sample_pts\":", stdout); timestamp(stdout, first);
    printf(",\"decoded_samples\":%"PRId64",\"decoded_frames\":%u,\"sample_rate\":%d,\"channels\":2,\"channel_layout\":\"stereo\"}\n",
           count, frames, SAMPLE_RATE);
    close_decoder(&decoder);
}

static void export_packets(const char *path) {
    admit_fixture(path);
    AVFormatContext *input = NULL;
    check(avformat_open_input(&input, path, NULL, NULL), "open packet observations");
    check(avformat_find_stream_info(input, NULL), "probe packet streams");
    require(input->nb_streams > 0 && input->nb_streams <= 8, "bounded packet stream inventory");
    fputs("{\"schema_version\":1,\"kind\":\"packets\",\"streams\":[", stdout);
    for (unsigned i = 0; i < input->nb_streams; i++) {
        AVStream *stream = input->streams[i];
        require(stream->time_base.num > 0 && stream->time_base.den > 0, "positive packet time base");
        printf("%s{\"stream_index\":%u,\"codec_type\":", i ? "," : "", i);
        named(stdout, av_get_media_type_string(stream->codecpar->codec_type));
        fputs(",\"codec_name\":", stdout); named(stdout, avcodec_get_name(stream->codecpar->codec_id));
        printf(",\"time_base\":[%d,%d]}", stream->time_base.num, stream->time_base.den);
    }
    fputs("],\"packets\":[", stdout);
    AVPacket *packet = av_packet_alloc(); require(packet != NULL, "allocate observed packet");
    unsigned count = 0;
    for (;;) {
        int status = av_read_frame(input, packet);
        if (status == AVERROR_EOF) break;
        check(status, "read observed packet");
        require(count < EXPORT_PACKET_LIMIT, "observed packet count cap");
        require(packet->stream_index >= 0 && (unsigned)packet->stream_index < input->nb_streams,
                "packet belongs to observed stream");
        if (count) putchar(',');
        packet_json(stdout, packet, input->streams[packet->stream_index], count++, NULL, 0, 0, 0);
        av_packet_unref(packet);
    }
    printf("],\"packet_count\":%u}\n", count);
    av_packet_free(&packet); avformat_close_input(&input);
}

int main(int argc, char **argv) {
    av_log_set_level(AV_LOG_WARNING);
    if (argc == 2 && strcmp(argv[1], "inventory") == 0) inventory();
    else if (argc == 10 && strcmp(argv[1], "encode") == 0) export_encode(argv);
    else if (argc == 3 && strcmp(argv[1], "video") == 0) export_video(argv[2]);
    else if (argc == 5 && strcmp(argv[1], "audio") == 0) export_audio(argv[2], argv[3], argv[4]);
    else if (argc == 3 && strcmp(argv[1], "packets") == 0) export_packets(argv[2]);
    else {
        fputs("usage: export_probe inventory | encode OUTPUT PACKETS_JSONL hardware-no-b|software-no-b|hardware-b|software-b default|disabled FPS_NUM FPS_DEN FRAMES impulses|edges | video INPUT | audio INPUT PCM ordinary|manual | packets INPUT\n", stderr);
        return 2;
    }
    require(!ferror(stdout) && fflush(stdout) == 0, "write complete observation report");
    return 0;
}
