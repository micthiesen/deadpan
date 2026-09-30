#define _POSIX_C_SOURCE 200809L

#include "encoder.h"

#include <errno.h>
#include <fcntl.h>
#include <inttypes.h>
#include <limits.h>
#include <math.h>
#include <stdarg.h>
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
#include <libavutil/mem.h>
#include <libavutil/opt.h>
#include <libavutil/version.h>

#if LIBAVCODEC_VERSION_INT != AV_VERSION_INT(62, 11, 103)
#error "deadpan-encode requires libavcodec 62.11.103"
#endif
#if LIBAVFORMAT_VERSION_INT != AV_VERSION_INT(62, 3, 103)
#error "deadpan-encode requires libavformat 62.3.103"
#endif
#if LIBAVUTIL_VERSION_INT != AV_VERSION_INT(60, 8, 103)
#error "deadpan-encode requires libavutil 60.8.103"
#endif

#define IO_BUFFER_BYTES 32768
#define PRIVATE_URL "deadpan-private-encode-output"
#define MAX_EXTRADATA_BYTES 65536
#define MAX_PENDING_VIDEO 64ULL
#define MAX_PENDING_AUDIO 8ULL
#define MAX_CODEC_STEPS 128U

typedef struct {
    dp_encode_session *session;
    int64_t position;
    int64_t read_limit;
    int writing;
} DescriptorIo;

struct dp_encode_session {
    int fd;
    dp_encode_config config;
    dp_encode_info info;
    AVFormatContext *format;
    AVCodecContext *video;
    AVCodecContext *audio;
    AVStream *video_stream;
    AVStream *audio_stream;
    AVFrame *picture;
    AVFrame *sound;
    AVPacket *packet;
    uint8_t *video_seen;
    AVIOContext *write_io;
    AVIOContext *read_io;
    DescriptorIo write_descriptor;
    DescriptorIo read_descriptor;
    const dp_encode_control *control;
    dp_encode_error *error;
    uint64_t deadline_ns;
    uint64_t video_frames;
    uint64_t audio_samples;
    uint64_t audio_frames;
    uint64_t video_packets;
    uint64_t audio_packets;
    uint64_t packet_bytes;
    uint64_t video_duration_from_contract_packets;
    uint64_t output_length;
    int64_t video_last_dts;
    int64_t audio_last_dts;
    uint32_t faststart_read_opens;
    uint32_t faststart_read_closes;
    int video_eof;
    int audio_eof;
    int trailer_active;
    int poisoned;
    int finished;
};

static int fail(dp_encode_session *session, const char *code, const char *format, ...) {
    va_list args;
    session->poisoned = 1;
    if (session->error != NULL && session->error->code[0] == '\0') {
        (void)snprintf(session->error->code, sizeof(session->error->code), "%s", code);
        va_start(args, format);
        (void)vsnprintf(session->error->message, sizeof(session->error->message), format, args);
        va_end(args);
    }
    return 0;
}

static int ff_failure(dp_encode_session *session, const char *operation, int code) {
    char detail[AV_ERROR_MAX_STRING_SIZE];
    if (av_strerror(code, detail, sizeof(detail)) < 0)
        (void)snprintf(detail, sizeof(detail), "FFmpeg error %d", code);
    return fail(session, "encoder_failure", "%s: %s", operation, detail);
}

static uint64_t monotonic_ns(void) {
    struct timespec now;
    if (clock_gettime(CLOCK_MONOTONIC, &now) != 0 || now.tv_sec < 0 ||
        (uint64_t)now.tv_sec > (UINT64_MAX - 999999999ULL) / 1000000000ULL)
        return UINT64_MAX;
    return (uint64_t)now.tv_sec * 1000000000ULL + (uint64_t)now.tv_nsec;
}

static int check_control(dp_encode_session *session) {
    if (session->control == NULL)
        return fail(session, "internal_error", "native encoder called without active control");
    if (session->control->cancelled(session->control->opaque))
        return fail(session, "cancelled", "native encoding was cancelled");
    uint64_t now = monotonic_ns();
    if (now == UINT64_MAX)
        return fail(session, "internal_error", "read monotonic encoder clock");
    if (now >= session->deadline_ns)
        return fail(session, "deadline_exceeded", "native encoding exceeded its cooperative deadline");
    return 1;
}

static int begin_call(dp_encode_session *session, const dp_encode_control *control,
                      dp_encode_error *error) {
    if (error != NULL) memset(error, 0, sizeof(*error));
    if (session == NULL) {
        if (error != NULL) {
            (void)snprintf(error->code, sizeof(error->code), "invalid_session");
            (void)snprintf(error->message, sizeof(error->message), "native encoder session is null");
        }
        return 0;
    }
    session->error = error;
    session->control = control;
    if (session->poisoned || session->finished)
        return fail(session, "invalid_session", "native encoder session is poisoned or finished");
    if (error == NULL || control == NULL || control->cancelled == NULL ||
        control->timeout_millis == 0 || control->timeout_millis > DP_ENCODE_MAX_TIMEOUT_MILLIS)
        return fail(session, "invalid_control", "native encoder requires bounded active control");
    uint64_t now = monotonic_ns();
    uint64_t allowance = control->timeout_millis * 1000000ULL;
    if (now == UINT64_MAX || now > UINT64_MAX - allowance)
        return fail(session, "invalid_control", "native encoder deadline overflow");
    session->deadline_ns = now + allowance;
    return check_control(session);
}

static int end_call(dp_encode_session *session, int success) {
    if (success && session->poisoned) success = 0;
    if (success) success = check_control(session);
    if (!success) session->poisoned = 1;
    session->control = NULL;
    session->error = NULL;
    return success;
}

static int interrupt(void *opaque) {
    return !check_control(opaque);
}

static uint32_t gcd_u32(uint32_t a, uint32_t b) {
    while (b != 0) { uint32_t remainder = a % b; a = b; b = remainder; }
    return a;
}

static int runtime_valid(dp_encode_session *s) {
    if (avcodec_version() != LIBAVCODEC_VERSION_INT ||
        avformat_version() != LIBAVFORMAT_VERSION_INT || avutil_version() != LIBAVUTIL_VERSION_INT)
        return fail(s, "runtime_mismatch", "loaded FFmpeg differs from pinned 8.0.3 headers");
    const char *configurations[] = {avcodec_configuration(), avformat_configuration(), avutil_configuration()};
    const char *licenses[] = {avcodec_license(), avformat_license(), avutil_license()};
    const char *required[] = {"--disable-gpl", "--disable-nonfree", "--disable-version3", "--disable-network"};
    const char *forbidden[] = {"--enable-gpl", "--enable-nonfree", "--enable-version3", "--enable-network"};
    for (size_t library = 0; library < 3; ++library) {
        if (strcmp(licenses[library], "LGPL version 2.1 or later") != 0)
            return fail(s, "runtime_mismatch", "loaded FFmpeg license is not LGPL 2.1+");
        for (size_t option = 0; option < 4; ++option)
            if (strstr(configurations[library], required[option]) == NULL ||
                strstr(configurations[library], forbidden[option]) != NULL)
                return fail(s, "runtime_mismatch", "loaded FFmpeg violates %s", required[option]);
    }
    return 1;
}

static int validate_config(dp_encode_session *s) {
    const dp_encode_config *c = &s->config;
    if (c->abi_version != DP_ENCODE_ABI_VERSION || c->width < 2 || c->height < 2 ||
        c->width > DP_ENCODE_MAX_DIMENSION || c->height > DP_ENCODE_MAX_DIMENSION ||
        (c->width & 1) || (c->height & 1) ||
        (uint64_t)c->width * c->height > DP_ENCODE_MAX_PIXELS ||
        c->fps_num == 0 || c->fps_den == 0 || c->fps_num > INT_MAX || c->fps_den > INT_MAX ||
        gcd_u32(c->fps_num, c->fps_den) != 1 ||
        (uint64_t)c->fps_num > 60ULL * c->fps_den ||
        c->video_frames == 0 || c->video_frames > DP_ENCODE_MAX_FRAMES ||
        c->audio_samples == 0 || c->audio_samples > DP_ENCODE_MAX_AUDIO_SAMPLES ||
        c->video_bitrate == 0 || c->video_bitrate > 1000000000ULL ||
        c->gop_frames == 0 || c->gop_frames > 600 ||
        (c->b_frames != 0 && c->b_frames != 2) || c->mode > 1 ||
        c->maximum_output_bytes == 0 || c->maximum_output_bytes > DP_ENCODE_MAX_OUTPUT_BYTES ||
        c->maximum_packets == 0 || c->maximum_packets > DP_ENCODE_MAX_PACKETS ||
        c->maximum_packet_bytes == 0 || c->maximum_packet_bytes > DP_ENCODE_MAX_PACKET_BYTES)
        return fail(s, "invalid_config", "encoder contract exceeds native admission bounds");
    __int128 ticks = (__int128)c->video_frames * c->fps_den;
    if (ticks > INT64_MAX || ticks > (__int128)86400 * c->fps_num)
        return fail(s, "invalid_config", "encoder duration exceeds 24 hours or exact timestamp bounds");
    __int128 dividend = ticks * 48000;
    __int128 rounded = dividend / c->fps_num;
    __int128 remainder = dividend % c->fps_num;
    if (remainder * 2 > c->fps_num || (remainder * 2 == c->fps_num && (rounded & 1))) ++rounded;
    __int128 difference = (__int128)c->audio_samples - rounded;
    if (difference < -1 || difference > 1)
        return fail(s, "invalid_config", "audio endpoint differs from the video interval by more than one sample");
    uint64_t required_packets = c->video_frames + (c->audio_samples + 1023) / 1024 + 2;
    if (required_packets > c->maximum_packets)
        return fail(s, "invalid_config", "packet budget cannot retain the complete authored interval");
    uint64_t timescale = (uint64_t)(c->fps_num / gcd_u32(c->fps_num, 48000)) * 48000;
    if (timescale > INT_MAX)
        return fail(s, "invalid_config", "exact movie timescale exceeds signed MP4 bound");
    s->info.movie_timescale = (uint32_t)timescale;
    /* Pinned movenc uses one MOVIentry per H264/AAC packet. Its emitted tables
     * need at most stts8+ctts8+stsz4+stsc12+co64(8)+stss4+sdtp1 bytes/entry.
     * No metadata, chapters, subtitles, timecode, fragments, encryption or
     * packet extradata changes are admitted. 128 bytes/packet + 1 MiB covers
     * both tracks and bounded codec headers. get_moov_size uses a null writer;
     * ff_format_shift_data then allocates TWO buffers of that moov size.
     * The 2M packet hard cap therefore bounds those buffers below 515 MiB;
     * MOVIentry growth and temporary run tables are separately linear/bounded.
     */
    s->info.maximum_moov_bytes = c->maximum_packets * 128ULL + 1048576ULL;
    return 1;
}

static int validate_descriptor(dp_encode_session *s) {
    struct stat metadata;
    int flags = fcntl(s->fd, F_GETFL);
    if (flags < 0 || fstat(s->fd, &metadata) != 0)
        return fail(s, "invalid_descriptor", "inspect encoder output: %s", strerror(errno));
    if (!S_ISREG(metadata.st_mode) || metadata.st_size != 0 || metadata.st_nlink != 1 ||
        metadata.st_uid != geteuid() || (metadata.st_mode & 077) != 0 ||
        (flags & O_ACCMODE) != O_RDWR || (flags & O_APPEND) != 0)
        return fail(s, "invalid_descriptor", "encoder output must be a fresh private singly linked read/write regular file");
    return 1;
}

static int descriptor_read(void *opaque, uint8_t *bytes, int size) {
    DescriptorIo *io = opaque;
    dp_encode_session *s = io->session;
    if (!check_control(s)) return AVERROR_EXIT;
    if (io->writing || size <= 0 || io->position < 0) return AVERROR(EINVAL);
    if (io->position >= io->read_limit) return AVERROR_EOF;
    size_t amount = (size_t)size;
    if ((int64_t)amount > io->read_limit - io->position)
        amount = (size_t)(io->read_limit - io->position);
    ssize_t read;
    do {
        if (!check_control(s)) return AVERROR_EXIT;
        read = pread(s->fd, bytes, amount, (off_t)io->position);
    } while (read < 0 && errno == EINTR);
    if (read < 0) {
        int reason = errno;
        fail(s, "output_io", "read fast-start bytes: %s", strerror(reason)); return AVERROR(reason);
    }
    if (read == 0) { fail(s, "output_io", "unexpected EOF while relocating MP4 data"); return AVERROR(EIO); }
    io->position += read;
    return (int)read;
}

static int descriptor_write(void *opaque, const uint8_t *bytes, int size) {
    DescriptorIo *io = opaque;
    dp_encode_session *s = io->session;
    if (!check_control(s)) return AVERROR_EXIT;
    if (!io->writing || size < 0 || io->position < 0 ||
        (uint64_t)io->position > s->config.maximum_output_bytes ||
        (uint64_t)size > s->config.maximum_output_bytes - (uint64_t)io->position) {
        fail(s, "output_too_large", "MP4 output exceeds its byte budget"); return AVERROR(ENOSPC);
    }
    int written = 0;
    while (written < size) {
        if (!check_control(s)) return AVERROR_EXIT;
        ssize_t count = pwrite(s->fd, bytes + written, (size_t)(size - written), (off_t)(io->position + written));
        if (count < 0 && errno == EINTR) continue;
        if (count <= 0) {
            int reason = count < 0 ? errno : EIO;
            fail(s, "output_io", "write encoded MP4: %s", strerror(reason)); return AVERROR(reason);
        }
        written += (int)count;
    }
    io->position += written;
    if ((uint64_t)io->position > s->output_length) s->output_length = (uint64_t)io->position;
    return written;
}

static int64_t descriptor_seek(void *opaque, int64_t offset, int whence) {
    DescriptorIo *io = opaque;
    dp_encode_session *s = io->session;
    if (!check_control(s)) return AVERROR_EXIT;
    whence &= ~AVSEEK_FORCE;
    if (whence == AVSEEK_SIZE) return io->writing ? (int64_t)s->output_length : io->read_limit;
    int64_t base;
    if (whence == SEEK_SET) base = 0;
    else if (whence == SEEK_CUR) base = io->position;
    else if (whence == SEEK_END) base = io->writing ? (int64_t)s->output_length : io->read_limit;
    else return AVERROR(EINVAL);
    __int128 position = (__int128)base + offset;
    uint64_t maximum = io->writing ? s->config.maximum_output_bytes : (uint64_t)io->read_limit;
    if (position < 0 || position > maximum) {
        fail(s, "output_seek", "MP4 seek exceeds descriptor bounds"); return AVERROR(EINVAL);
    }
    io->position = (int64_t)position;
    return io->position;
}

static AVIOContext *allocate_io(DescriptorIo *descriptor) {
    uint8_t *buffer = av_malloc(IO_BUFFER_BYTES);
    if (buffer == NULL) return NULL;
    AVIOContext *io = avio_alloc_context(buffer, IO_BUFFER_BYTES, descriptor->writing, descriptor,
        descriptor->writing ? NULL : descriptor_read,
        descriptor->writing ? descriptor_write : NULL, descriptor_seek);
    if (io == NULL) av_free(buffer);
    return io;
}

static void free_io(AVIOContext **io) {
    if (*io != NULL) av_freep(&(*io)->buffer);
    avio_context_free(io);
}

static int faststart_open(AVFormatContext *format, AVIOContext **io, const char *url,
                           int flags, AVDictionary **options) {
    dp_encode_session *s = format->opaque;
    if (!check_control(s)) return AVERROR_EXIT;
    if (!s->trailer_active || s->read_io != NULL || s->faststart_read_opens != 0 ||
        url == NULL || strcmp(url, PRIVATE_URL) != 0 || flags != AVIO_FLAG_READ ||
        (options != NULL && av_dict_count(*options) != 0)) {
        fail(s, "external_io_denied", "muxer requested an unapproved external resource"); return AVERROR(EPERM);
    }
    avio_flush(s->write_io);
    if (s->write_io->error < 0) return s->write_io->error;
    s->read_descriptor = (DescriptorIo){s, 0, (int64_t)s->output_length, 0};
    s->read_io = allocate_io(&s->read_descriptor);
    if (s->read_io == NULL) { fail(s, "allocation_failure", "allocate fast-start descriptor reader"); return AVERROR(ENOMEM); }
    s->faststart_read_opens++;
    *io = s->read_io;
    return 0;
}

static int faststart_close(AVFormatContext *format, AVIOContext *io) {
    dp_encode_session *s = format->opaque;
    if (io == NULL || io != s->read_io) {
        fail(s, "external_io_denied", "muxer closed an unowned descriptor reader"); return AVERROR(EPERM);
    }
    int error = io->error;
    free_io(&s->read_io);
    s->faststart_read_closes++;
    if (error < 0 && error != AVERROR_EOF) return error;
    return check_control(s) ? 0 : AVERROR_EXIT;
}

static int dict_option(dp_encode_session *s, AVDictionary **options, const char *name, const char *value) {
    int error = av_dict_set(options, name, value, 0);
    return error < 0 ? ff_failure(s, "set encoder option", error) : 1;
}

static int dict_number(dp_encode_session *s, AVDictionary **options, const char *name, int64_t value) {
    int error = av_dict_set_int(options, name, value, 0);
    return error < 0 ? ff_failure(s, "set numeric encoder option", error) : 1;
}

static int open_codec(dp_encode_session *s, int audio) {
    const AVCodec *implementation = avcodec_find_encoder_by_name(audio ? "aac" : "h264_videotoolbox");
    if (implementation == NULL)
        return fail(s, audio ? "audio_encoder_unavailable" : "video_encoder_unavailable",
            "required %s encoder is unavailable", audio ? "native AAC" : "VideoToolbox H264");
    AVCodecContext *codec = avcodec_alloc_context3(implementation);
    if (codec == NULL) return fail(s, "allocation_failure", "allocate encoder context");
    if (audio) s->audio = codec; else s->video = codec;
    codec->thread_count = 1;
    codec->flags |= AV_CODEC_FLAG_GLOBAL_HEADER;
    AVDictionary *options = NULL;
    int success = 0;
    if (audio) {
        codec->sample_rate = 48000;
        codec->sample_fmt = AV_SAMPLE_FMT_FLTP;
        av_channel_layout_default(&codec->ch_layout, 2);
        codec->time_base = (AVRational){1, 48000};
        codec->bit_rate = 384000;
        codec->profile = AV_PROFILE_AAC_LOW;
        if (!(implementation->capabilities & AV_CODEC_CAP_SMALL_LAST_FRAME)) {
            fail(s, "encoder_unsupported", "AAC encoder cannot accept an exact short final frame"); goto done;
        }
    } else {
        codec->width = (int)s->config.width; codec->height = (int)s->config.height;
        codec->pix_fmt = AV_PIX_FMT_YUV420P;
        /* VT consumes its private profile option but does not write this
         * generic requested field back. Keep both declarations coherent;
         * only independent emitted-bitstream inspection proves High profile. */
        codec->profile = AV_PROFILE_H264_HIGH;
        codec->time_base = (AVRational){1, (int)s->config.fps_num};
        codec->framerate = (AVRational){(int)s->config.fps_num, (int)s->config.fps_den};
        codec->gop_size = (int)s->config.gop_frames;
        codec->max_b_frames = (int)s->config.b_frames;
        codec->bit_rate = (int64_t)s->config.video_bitrate;
        codec->sample_aspect_ratio = (AVRational){1, 1};
        codec->field_order = AV_FIELD_PROGRESSIVE;
        codec->color_range = AVCOL_RANGE_MPEG;
        codec->color_primaries = AVCOL_PRI_BT709;
        codec->color_trc = AVCOL_TRC_BT709;
        codec->colorspace = AVCOL_SPC_BT709;
        codec->chroma_sample_location = AVCHROMA_LOC_LEFT;
        codec->flags |= AV_CODEC_FLAG_CLOSED_GOP | AV_CODEC_FLAG_FRAME_DURATION;
        if (!dict_option(s, &options, "allow_sw", s->config.mode ? "1" : "0") ||
            !dict_option(s, &options, "require_sw", s->config.mode ? "1" : "0") ||
            !dict_option(s, &options, "profile", "high")) goto done;
    }
    if (!check_control(s)) goto done;
    int error = avcodec_open2(codec, implementation, &options);
    if (error < 0) {
        /* Only this exact error establishes that the selected video encoder is
         * absent. EINVAL, ENOSYS, external/driver and generic open failures must
         * retain their ordinary failure category; their prose is not policy. */
        if (!audio && error == AVERROR_ENCODER_NOT_FOUND)
            fail(s, "video_encoder_unavailable",
                "open selected VideoToolbox H264 encoder: encoder not found (FFmpeg %d)", error);
        else
            ff_failure(s, "open selected encoder", error);
        goto done;
    }
    if (!check_control(s)) goto done;
    if (av_dict_count(options) != 0) { fail(s, "encoder_unsupported", "encoder did not consume all requested options"); goto done; }
    if (codec->extradata_size < 0 || codec->extradata_size > MAX_EXTRADATA_BYTES) {
        fail(s, "encoder_unsupported", "codec header exceeds admitted size"); goto done;
    }
    if (audio) {
        if (codec->sample_rate != 48000 || codec->sample_fmt != AV_SAMPLE_FMT_FLTP ||
            codec->ch_layout.order != AV_CHANNEL_ORDER_NATIVE || codec->ch_layout.u.mask != AV_CH_LAYOUT_STEREO ||
            codec->ch_layout.nb_channels != 2 || codec->frame_size != (int)DP_ENCODE_AUDIO_BLOCK ||
            av_cmp_q(codec->time_base, (AVRational){1, 48000}) != 0) {
            fail(s, "encoder_unsupported", "AAC encoder changed the exact PCM contract"); goto done;
        }
    } else if (codec->width != (int)s->config.width || codec->height != (int)s->config.height ||
               codec->pix_fmt != AV_PIX_FMT_YUV420P ||
               av_cmp_q(codec->time_base, (AVRational){1, (int)s->config.fps_num}) != 0) {
        fail(s, "encoder_unsupported", "H264 encoder changed the picture layout or clock"); goto done;
    }
    AVStream *stream = avformat_new_stream(s->format, NULL);
    if (stream == NULL) { fail(s, "allocation_failure", "allocate MP4 stream"); goto done; }
    if (audio) s->audio_stream = stream; else s->video_stream = stream;
    stream->time_base = codec->time_base;
    if (!audio) { stream->avg_frame_rate = codec->framerate; stream->sample_aspect_ratio = codec->sample_aspect_ratio; }
    error = avcodec_parameters_from_context(stream->codecpar, codec);
    if (error < 0) { ff_failure(s, "copy encoder stream parameters", error); goto done; }
    success = 1;
done:
    av_dict_free(&options);
    return success;
}

static int allocate_inputs(dp_encode_session *s) {
    s->picture = av_frame_alloc(); s->sound = av_frame_alloc(); s->packet = av_packet_alloc();
    s->video_seen = av_mallocz((size_t)((s->config.video_frames + 7) / 8));
    if (s->picture == NULL || s->sound == NULL || s->packet == NULL || s->video_seen == NULL)
        return fail(s, "allocation_failure", "allocate bounded native input and packet");
    s->picture->width = s->video->width; s->picture->height = s->video->height;
    s->picture->format = s->video->pix_fmt;
    s->picture->sample_aspect_ratio = (AVRational){1, 1};
    s->picture->color_range = AVCOL_RANGE_MPEG;
    s->picture->color_primaries = AVCOL_PRI_BT709;
    s->picture->color_trc = AVCOL_TRC_BT709;
    s->picture->colorspace = AVCOL_SPC_BT709;
    s->picture->chroma_location = AVCHROMA_LOC_LEFT;
    int error = av_frame_get_buffer(s->picture, 32);
    if (error < 0) return ff_failure(s, "allocate aligned picture", error);
    s->sound->format = AV_SAMPLE_FMT_FLTP; s->sound->sample_rate = 48000;
    s->sound->nb_samples = DP_ENCODE_AUDIO_BLOCK;
    error = av_channel_layout_copy(&s->sound->ch_layout, &s->audio->ch_layout);
    if (error < 0) return ff_failure(s, "copy stereo layout", error);
    error = av_frame_get_buffer(s->sound, 0);
    return error < 0 ? ff_failure(s, "allocate bounded PCM frame", error) : 1;
}

static void observe_info(dp_encode_session *s) {
    s->info.abi_version = DP_ENCODE_ABI_VERSION;
    s->info.avcodec_version = avcodec_version(); s->info.avformat_version = avformat_version();
    s->info.avutil_version = avutil_version();
    s->info.video_time_base_num = (uint32_t)s->video_stream->time_base.num;
    s->info.video_time_base_den = (uint32_t)s->video_stream->time_base.den;
    s->info.audio_time_base_num = (uint32_t)s->audio_stream->time_base.num;
    s->info.audio_time_base_den = (uint32_t)s->audio_stream->time_base.den;
    s->info.audio_frame_size = (uint32_t)s->audio->frame_size;
    s->info.video_profile = s->video->profile;
    s->info.video_has_b_frames = s->video->has_b_frames;
    s->info.video_max_b_frames = s->video->max_b_frames;
    s->info.video_gop_size = s->video->gop_size;
    s->info.audio_profile = s->audio->profile;
    s->info.audio_initial_padding = s->audio->initial_padding;
    s->info.audio_trailing_padding = s->audio->trailing_padding;
    s->info.requested_mode = s->config.mode;
    s->info.video_bitrate = s->video->bit_rate < 0 ? 0 : (uint64_t)s->video->bit_rate;
    s->info.audio_bitrate = s->audio->bit_rate < 0 ? 0 : (uint64_t)s->audio->bit_rate;
}

int dp_encode_open(int fd, const dp_encode_config *config, const dp_encode_control *control,
                    dp_encode_session **session, dp_encode_info *info, dp_encode_error *error) {
    if (session != NULL) *session = NULL;
    if (info != NULL) memset(info, 0, sizeof(*info));
    dp_encode_session *s = calloc(1, sizeof(*s));
    if (s == NULL) {
        if (error != NULL) { memset(error, 0, sizeof(*error));
            (void)snprintf(error->code, sizeof(error->code), "allocation_failure");
            (void)snprintf(error->message, sizeof(error->message), "allocate encoder session"); }
        return 0;
    }
    s->fd = fd; s->video_last_dts = AV_NOPTS_VALUE; s->audio_last_dts = AV_NOPTS_VALUE;
    AVDictionary *options = NULL;
    int success = 0;
    if (!begin_call(s, control, error)) goto done;
    if (config == NULL || session == NULL || info == NULL) { fail(s, "invalid_config", "missing encoder configuration or result"); goto done; }
    s->config = *config;
    if (!validate_config(s) || !validate_descriptor(s) || !runtime_valid(s)) goto done;
    int code = avformat_alloc_output_context2(&s->format, NULL, "mp4", PRIVATE_URL);
    if (code < 0 || s->format == NULL) { ff_failure(s, "allocate MP4 muxer", code < 0 ? code : AVERROR(ENOMEM)); goto done; }
    s->format->opaque = s;
    s->format->io_open = faststart_open;
    s->format->io_close2 = faststart_close;
    s->format->interrupt_callback = (AVIOInterruptCB){interrupt, s};
    s->format->flags |= AVFMT_FLAG_CUSTOM_IO;
    s->format->avoid_negative_ts = AVFMT_AVOID_NEG_TS_DISABLED;
    s->format->max_interleave_delta = 1000000;
    if (!open_codec(s, 0) || !open_codec(s, 1) || !allocate_inputs(s)) goto done;
    s->write_descriptor = (DescriptorIo){s, 0, 0, 1};
    s->write_io = allocate_io(&s->write_descriptor);
    if (s->write_io == NULL) { fail(s, "allocation_failure", "allocate output descriptor writer"); goto done; }
    s->format->pb = s->write_io;
    if (!dict_option(s, &options, "movflags", "+faststart") ||
        !dict_option(s, &options, "use_editlist", "1") ||
        !dict_option(s, &options, "write_tmcd", "0") ||
        !dict_option(s, &options, "write_btrt", "0") ||
        !dict_number(s, &options, "movie_timescale", s->info.movie_timescale) ||
        !dict_number(s, &options, "video_track_timescale", s->config.fps_num)) goto done;
    code = avformat_write_header(s->format, &options);
    if (code < 0) { ff_failure(s, "write MP4 header", code); goto done; }
    if (av_dict_count(options) != 0) { fail(s, "encoder_unsupported", "muxer did not consume all requested options"); goto done; }
    if (av_cmp_q(s->video_stream->time_base, (AVRational){1, (int)s->config.fps_num}) != 0 ||
        av_cmp_q(s->audio_stream->time_base, (AVRational){1, 48000}) != 0) {
        fail(s, "encoder_unsupported", "MP4 muxer changed the exact output clocks"); goto done;
    }
    avio_flush(s->write_io);
    if (s->write_io->error < 0) { ff_failure(s, "flush MP4 header", s->write_io->error); goto done; }
    observe_info(s);
    *info = s->info;
    success = 1;
done:
    av_dict_free(&options);
    success = end_call(s, success);
    if (!success) dp_encode_close(s);
    else *session = s;
    return success;
}

static int packet_valid(dp_encode_session *s, int audio) {
    AVPacket *p = s->packet;
    AVCodecContext *codec = audio ? s->audio : s->video;
    uint64_t total = s->video_packets + s->audio_packets;
    if (p->size <= 0 || (uint64_t)p->size > s->config.maximum_packet_bytes ||
        total >= s->config.maximum_packets || (uint64_t)p->size > s->config.maximum_output_bytes - s->packet_bytes)
        return fail(s, "packet_limit", "encoder packet exceeds count, byte or total budget");
    /* Pinned videotoolboxenc.c returns PTS/DTS but no packet duration. The
     * caller already authored every CFR duration. Preserve that known value,
     * record the omission, and never infer timing from encoded content. */
    if (!audio && p->duration == 0) {
        p->duration = s->config.fps_den;
        s->video_duration_from_contract_packets++;
    }
    if (p->pts == AV_NOPTS_VALUE || p->dts == AV_NOPTS_VALUE || p->duration <= 0 ||
        p->side_data_elems < 0 || p->side_data_elems > 8)
        return fail(s, "invalid_packet", "encoder packet lacks bounded exact timestamps");
    int64_t *last = audio ? &s->audio_last_dts : &s->video_last_dts;
    if (*last != AV_NOPTS_VALUE && p->dts <= *last)
        return fail(s, "invalid_packet", "encoder DTS is not strictly increasing");
    __int128 end = audio ? (__int128)s->config.audio_samples + DP_ENCODE_AUDIO_BLOCK :
        (__int128)s->config.video_frames * s->config.fps_den;
    int64_t priming = audio ? (int64_t)DP_ENCODE_AUDIO_BLOCK * 8 : (int64_t)s->config.fps_den * 64;
    if (p->pts < -priming || p->dts < -priming || (__int128)p->pts >= end || (__int128)p->dts >= end ||
        (__int128)p->pts + p->duration > end + priming)
        return fail(s, "invalid_packet", "encoder packet timing exceeds bounded delay or interval");
    if (!audio && (p->pts < 0 || p->pts % s->config.fps_den != 0 || p->duration != s->config.fps_den))
        return fail(s, "invalid_packet", "H264 packet changed the authored frame clock");
    /* Retain the actual rejected encoder timestamps. This is an observation
     * about the selected path, not permission to rewrite timing or switch it.
     * Check before admitting this packet or handing it to the MP4 muxer. */
    if (!audio && p->pts < p->dts)
        return fail(s, "video_timestamp_order",
            "H264 packet PTS (%" PRId64 ") precedes DTS (%" PRId64 ")", p->pts, p->dts);
    if (!audio) {
        uint64_t ordinal = (uint64_t)p->pts / s->config.fps_den;
        uint8_t bit = (uint8_t)(1U << (ordinal % 8));
        if (s->video_seen[ordinal / 8] & bit)
            return fail(s, "invalid_packet", "H264 encoder emitted a duplicate presentation ordinal");
        s->video_seen[ordinal / 8] |= bit;
    }
    for (int index = 0; index < p->side_data_elems; ++index) {
        const AVPacketSideData *side = &p->side_data[index];
        if (side->size > 1024 ||
            (side->type != AV_PKT_DATA_QUALITY_STATS && !(audio && side->type == AV_PKT_DATA_SKIP_SAMPLES)))
            return fail(s, "invalid_packet", "encoder emitted unadmitted packet side data");
    }
    if (codec->extradata_size < 0 || codec->extradata_size > MAX_EXTRADATA_BYTES)
        return fail(s, "invalid_packet", "encoder changed its bounded codec header");
    *last = p->dts;
    s->packet_bytes += (uint64_t)p->size;
    if (audio) s->audio_packets++; else s->video_packets++;
    return 1;
}

static int receive_packets(dp_encode_session *s, int audio, int draining, int *received) {
    AVCodecContext *codec = audio ? s->audio : s->video;
    AVStream *stream = audio ? s->audio_stream : s->video_stream;
    *received = 0;
    for (unsigned step = 0; step < MAX_CODEC_STEPS; ++step) {
        if (!check_control(s)) return 0;
        int code = avcodec_receive_packet(codec, s->packet);
        if (code == AVERROR_EOF) { if (audio) s->audio_eof = 1; else s->video_eof = 1; return 1; }
        if (code == AVERROR(EAGAIN)) {
            if (draining) return fail(s, "encoder_stalled", "encoder requested new input after accepted drain");
            return 1;
        }
        if (code < 0) return ff_failure(s, "receive encoded packet", code);
        if (!packet_valid(s, audio)) { av_packet_unref(s->packet); return 0; }
        av_packet_rescale_ts(s->packet, codec->time_base, stream->time_base);
        s->packet->stream_index = stream->index;
        code = av_interleaved_write_frame(s->format, s->packet);
        av_packet_unref(s->packet);
        if (code < 0) return ff_failure(s, "mux encoded packet", code);
        if (s->write_io->error < 0) return ff_failure(s, "write encoded packet", s->write_io->error);
        ++*received;
    }
    return fail(s, "encoder_stalled", "encoder exceeded bounded packet drain work");
}

static int send_frame(dp_encode_session *s, int audio, AVFrame *frame) {
    AVCodecContext *codec = audio ? s->audio : s->video;
    if (!check_control(s)) return 0;
    int code = avcodec_send_frame(codec, frame);
    if (code == AVERROR(EAGAIN)) {
        int received;
        if (!receive_packets(s, audio, 0, &received)) return 0;
        if (received == 0) return fail(s, "encoder_stalled", "encoder send and receive both require progress");
        if (!check_control(s)) return 0;
        code = avcodec_send_frame(codec, frame);
    }
    if (code < 0) return ff_failure(s, "send encoder input", code);
    int received;
    return receive_packets(s, audio, frame == NULL, &received);
}

static int video_is_next(dp_encode_session *s) {
    if (s->video_frames >= s->config.video_frames) return 0;
    if (s->audio_samples >= s->config.audio_samples) return 1;
    return av_compare_ts((int64_t)(s->video_frames * s->config.fps_den), s->video->time_base,
                         (int64_t)s->audio_samples, s->audio->time_base) <= 0;
}

int dp_encode_push_picture(dp_encode_session *s, uint64_t ordinal, int64_t pts, int64_t duration,
                           const uint8_t *bytes, uint64_t length, const dp_encode_control *control,
                           dp_encode_error *error) {
    if (!begin_call(s, control, error)) return s == NULL ? 0 : end_call(s, 0);
    int success = 0;
    uint64_t expected_length = (uint64_t)s->config.width * s->config.height * 3 / 2;
    if (bytes == NULL || length != expected_length || ordinal != s->video_frames ||
        ordinal >= s->config.video_frames || pts != (int64_t)(ordinal * s->config.fps_den) ||
        duration != s->config.fps_den || !video_is_next(s)) {
        fail(s, "input_order", "picture identity, exact timing, planes or chronological input order disagree"); goto done;
    }
    if (s->video_frames >= s->video_packets + MAX_PENDING_VIDEO) {
        fail(s, "encoder_stalled", "VideoToolbox retained too many undelivered frames"); goto done;
    }
    int code = av_frame_make_writable(s->picture);
    if (code < 0) { ff_failure(s, "make encoder picture writable", code); goto done; }
    uint64_t offset = 0;
    for (int plane = 0; plane < 3; ++plane) {
        uint32_t width = plane == 0 ? s->config.width : s->config.width / 2;
        uint32_t height = plane == 0 ? s->config.height : s->config.height / 2;
        uint8_t maximum = plane == 0 ? 235 : 240;
        for (uint32_t row = 0; row < height; ++row) {
            if (!check_control(s)) goto done;
            for (uint32_t x = 0; x < width; ++x)
                if (bytes[offset + x] < 16 || bytes[offset + x] > maximum) {
                    fail(s, "invalid_pixels", "I420 input contains an out-of-range code"); goto done;
                }
            memcpy(s->picture->data[plane] + (size_t)row * (size_t)s->picture->linesize[plane], bytes + offset, width);
            offset += width;
        }
    }
    s->picture->pts = pts; s->picture->duration = duration;
    s->picture->flags &= ~(AV_FRAME_FLAG_INTERLACED | AV_FRAME_FLAG_TOP_FIELD_FIRST);
    if (!send_frame(s, 0, s->picture)) goto done;
    s->video_frames++;
    success = 1;
done:
    return end_call(s, success);
}

int dp_encode_push_audio(dp_encode_session *s, uint64_t first_sample, const float *left,
                         const float *right, uint32_t count, const dp_encode_control *control,
                         dp_encode_error *error) {
    if (!begin_call(s, control, error)) return s == NULL ? 0 : end_call(s, 0);
    int success = 0;
    if (left == NULL || right == NULL || count == 0 || count > DP_ENCODE_AUDIO_BLOCK ||
        first_sample != s->audio_samples || count > s->config.audio_samples - s->audio_samples ||
        (count < DP_ENCODE_AUDIO_BLOCK && count != s->config.audio_samples - s->audio_samples) ||
        video_is_next(s)) {
        fail(s, "input_order", "PCM sample identity, final block size or chronological input order disagree"); goto done;
    }
    if (s->audio_frames >= s->audio_packets + MAX_PENDING_AUDIO) {
        fail(s, "encoder_stalled", "AAC retained too many undelivered blocks"); goto done;
    }
    for (uint32_t sample = 0; sample < count; ++sample)
        if (!isfinite(left[sample]) || !isfinite(right[sample])) {
            fail(s, "invalid_pcm", "PCM input contains NaN or infinity"); goto done;
        }
    s->sound->nb_samples = DP_ENCODE_AUDIO_BLOCK;
    int code = av_frame_make_writable(s->sound);
    if (code < 0) { ff_failure(s, "make PCM buffer writable", code); goto done; }
    memcpy(s->sound->extended_data[0], left, count * sizeof(float));
    memcpy(s->sound->extended_data[1], right, count * sizeof(float));
    s->sound->nb_samples = (int)count;
    s->sound->pts = (int64_t)first_sample;
    s->sound->duration = count;
    if (!send_frame(s, 1, s->sound)) goto done;
    s->audio_samples += count; s->audio_frames++;
    success = 1;
done:
    return end_call(s, success);
}

int dp_encode_finish(dp_encode_session *s, const dp_encode_control *control,
                     dp_encode_report *report, dp_encode_error *error) {
    if (report != NULL) memset(report, 0, sizeof(*report));
    if (!begin_call(s, control, error)) return s == NULL ? 0 : end_call(s, 0);
    int success = 0;
    if (report == NULL || s->video_frames != s->config.video_frames || s->audio_samples != s->config.audio_samples) {
        fail(s, "incomplete_input", "finish requires every authored frame and sample exactly once"); goto done;
    }
    if (!send_frame(s, 0, NULL) || !send_frame(s, 1, NULL)) goto done;
    if (!s->video_eof || !s->audio_eof || s->video_packets != s->config.video_frames ||
        s->audio_packets != s->audio_frames + 1) {
        fail(s, "incomplete_output", "encoders did not emit their complete bounded streams and EOF"); goto done;
    }
    int code = av_interleaved_write_frame(s->format, NULL);
    if (code < 0) { ff_failure(s, "flush interleaved packets", code); goto done; }
    if (!check_control(s)) goto done;
    s->trailer_active = 1;
    code = av_write_trailer(s->format);
    s->trailer_active = 0;
    if (code < 0) { ff_failure(s, "finish fast-start MP4", code); goto done; }
    avio_flush(s->write_io);
    if (s->write_io->error < 0) { ff_failure(s, "flush MP4 trailer", s->write_io->error); goto done; }
    if (s->poisoned || s->faststart_read_opens != 1 || s->faststart_read_closes != 1 || s->read_io != NULL) {
        fail(s, "faststart_failure", "muxer did not finish one owned descriptor relocation"); goto done;
    }
    if (!check_control(s)) goto done;
    if (fsync(s->fd) != 0) { fail(s, "output_io", "synchronize MP4: %s", strerror(errno)); goto done; }
    struct stat metadata;
    if (fstat(s->fd, &metadata) != 0 || metadata.st_size < 0 || (uint64_t)metadata.st_size != s->output_length ||
        s->output_length == 0 || s->output_length > s->config.maximum_output_bytes) {
        fail(s, "output_io", "finished MP4 length differs from descriptor writes"); goto done;
    }
    if (!check_control(s)) goto done;
    observe_info(s);
    *report = (dp_encode_report){s->info, s->video_frames, s->audio_samples,
        s->video_packets, s->audio_packets, s->output_length, s->packet_bytes,
        s->video_duration_from_contract_packets,
        s->faststart_read_opens, s->faststart_read_closes, 1, 1};
    success = 1;
done:
    success = end_call(s, success);
    if (success) s->finished = 1;
    return success;
}

void dp_encode_close(dp_encode_session *s) {
    if (s == NULL) return;
    /* No trailer is attempted here: a failed/dropped session cannot become a
     * success. Free native state without callbacks borrowing expired Rust data. */
    av_packet_free(&s->packet);
    av_freep(&s->video_seen);
    av_frame_free(&s->picture); av_frame_free(&s->sound);
    avcodec_free_context(&s->video); avcodec_free_context(&s->audio);
    if (s->format != NULL) s->format->pb = NULL;
    avformat_free_context(s->format);
    free_io(&s->read_io); free_io(&s->write_io);
    free(s);
}
