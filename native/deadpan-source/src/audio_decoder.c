#define _POSIX_C_SOURCE 200809L
#include "audio_decoder.h"
#include <errno.h>
#include <inttypes.h>
#include <limits.h>
#include <math.h>
#include <stdarg.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>
#include <libavcodec/avcodec.h>
#include <libavcodec/version.h>
#include <libavformat/avformat.h>
#include <libavformat/version.h>
#include <libavutil/avutil.h>
#include <libavutil/channel_layout.h>
#include <libavutil/error.h>
#include <libavutil/intreadwrite.h>
#include <libavutil/mem.h>
#include <libavutil/opt.h>
#include <libavutil/samplefmt.h>
#include <libavutil/version.h>
#if LIBAVCODEC_VERSION_INT != AV_VERSION_INT(62, 11, 103) || LIBAVFORMAT_VERSION_INT != AV_VERSION_INT(62, 3, 103) || LIBAVUTIL_VERSION_INT != AV_VERSION_INT(60, 8, 103)
#error "deadpan-source audio requires exactly FFmpeg 8.0.3 headers"
#endif
#define IO_BUFFER_BYTES 32768
#define MAX_PROBE_BYTES (1024 * 1024)
#define MAX_STREAMS 33
#define DEMUXERS "mov,wav"
#define CODECS "aac,pcm_s16le"
struct DeadpanAudio {
    int fd;
    int64_t length, position;
    DeadpanAudioLimits limits;
    DeadpanAudioInfo info;
    DeadpanAudioLayout header_layout;
    DeadpanAudioFrame current;
    AVIOContext *io;
    AVFormatContext *format;
    AVCodecContext *decoder;
    AVPacket *packet;
    AVFrame *frame;
    int stream, draining, ended, poisoned, current_valid;
    unsigned int stream_count;
    uint64_t frames, packets, samples, io_bytes, deadline;
    DeadpanCancelled cancelled;
    const void *cancel_opaque;
    DeadpanSourceError *error;
};
static int fail(DeadpanAudio *s, const char *code, const char *format, ...) {
    if (s->error && !s->error->code[0]) {
        va_list args;
        (void)snprintf(s->error->code, sizeof(s->error->code), "%s", code);
        va_start(args, format);
        (void)vsnprintf(s->error->message, sizeof(s->error->message), format, args);
        va_end(args);
    }
    return -1;
}
static int fferror(DeadpanAudio *s, const char *operation, int error) {
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
static int check(DeadpanAudio *s) {
    // Demuxers may turn an AVIO error into EOF or return buffered frames. A host
    // I/O/budget failure must still fail this operation instead of being hidden.
    if (s->error && s->error->code[0]) return -1;
    if (s->cancelled && s->cancelled(s->cancel_opaque)) return fail(s, "cancelled", "source decode cancelled");
    uint64_t now = monotonic_ns();
    if (now == UINT64_MAX || now >= s->deadline) return fail(s, "deadline_exceeded", "source decode exceeded its cooperative deadline");
    return 1;
}
static int interrupt(void *opaque) { return check(opaque) < 0; }
static int begin(DeadpanAudio *s, uint64_t timeout, DeadpanCancelled cancelled,
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
static int finish(DeadpanAudio *s, int result) {
    if (result < 0) s->poisoned = 1;
    s->error = NULL;
    s->cancelled = NULL;
    s->cancel_opaque = NULL;
    return result;
}
static int read_descriptor(void *opaque, uint8_t *buffer, int size) {
    DeadpanAudio *s = opaque;
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
    return (int)count;
}
static int64_t seek_descriptor(void *opaque, int64_t offset, int whence) {
    DeadpanAudio *s = opaque;
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
static int runtime(DeadpanAudio *s) {
    if (avcodec_version() != LIBAVCODEC_VERSION_INT || avformat_version() != LIBAVFORMAT_VERSION_INT ||
        avutil_version() != LIBAVUTIL_VERSION_INT)
        return fail(s, "runtime_mismatch", "loaded FFmpeg libraries differ from pinned 8.0.3");
    const char *required[] = {"--disable-gpl", "--disable-nonfree", "--disable-version3", "--disable-network"};
    const char *forbidden[] = {"--enable-gpl", "--enable-nonfree", "--enable-version3", "--enable-network"};
    const char *configs[] = {avcodec_configuration(), avformat_configuration(), avutil_configuration()};
    const char *licenses[] = {avcodec_license(), avformat_license(), avutil_license()};
    for (size_t i = 0; i < 3; i++) {
        if (strcmp(licenses[i], "LGPL version 2.1 or later")) return fail(s, "runtime_mismatch", "loaded FFmpeg license differs from LGPL 2.1+");
        for (size_t j = 0; j < 4; j++)
            if (!strstr(configs[i], required[j]) || strstr(configs[i], forbidden[j]))
                return fail(s, "runtime_mismatch", "loaded FFmpeg configuration violates %s", required[j]);
    }
    return 1;
}
static int allowed_codec(enum AVCodecID id) {
    return id == AV_CODEC_ID_AAC || id == AV_CODEC_ID_PCM_S16LE;
}
static int layout(DeadpanAudio *s, const AVChannelLayout *value, DeadpanAudioLayout *out) {
    if (!av_channel_layout_check(value) || value->nb_channels <= 0 ||
        (uint32_t)value->nb_channels > s->limits.max_channels)
        return fail(s, "unsupported_layout", "audio channel count or layout is invalid or exceeds its bound");
    if (value->order != AV_CHANNEL_ORDER_NATIVE && value->order != AV_CHANNEL_ORDER_UNSPEC)
        return fail(s, "unsupported_layout", "custom and ambisonic channel layouts are unqualified");
    *out = (DeadpanAudioLayout){.channels=value->nb_channels,.order=value->order,
        .mask=value->order == AV_CHANNEL_ORDER_NATIVE ? value->u.mask : 0};
    return 1;
}
static int same_layout(const DeadpanAudioLayout *a, const DeadpanAudioLayout *b) {
    return a->channels == b->channels && a->order == b->order && a->mask == b->mask;
}
static int rate(DeadpanAudio *s, int value) {
    if (value <= 0 || (uint32_t)value > s->limits.max_sample_rate)
        return fail(s, "unsupported_rate", "audio sample rate is missing or exceeds its bound");
    return 1;
}
static int profile(DeadpanAudio *s, enum AVCodecID codec, int value, int allow_unknown) {
    if (codec == AV_CODEC_ID_AAC && value != AV_PROFILE_AAC_LOW &&
        !(allow_unknown && value == AV_PROFILE_UNKNOWN))
        return fail(s, "unsupported_profile", "only AAC-LC is qualified");
    return 1;
}
static int table(DeadpanAudio *s, int initial) {
    if (s->format->nb_streams == 0 || s->format->nb_streams > MAX_STREAMS ||
        (!initial && s->format->nb_streams != s->stream_count) ||
        s->stream < 0 || (unsigned int)s->stream >= s->format->nb_streams)
        return fail(s, "unsupported_streams", "audio stream selection or stream table is invalid");
    for (unsigned int i = 0; i < s->format->nb_streams; i++) {
        AVStream *stream = s->format->streams[i];
        AVCodecParameters *p = stream->codecpar;
        if (stream->index != (int)i) return fail(s, "unsupported_streams", "stream indices are not stable");
        if (p->codec_type != AVMEDIA_TYPE_VIDEO && p->codec_type != AVMEDIA_TYPE_AUDIO)
            return fail(s, "unsupported_streams", "container includes an unqualified stream type");
        if ((int)i != s->stream) stream->discard = AVDISCARD_ALL;
    }
    AVStream *stream = s->format->streams[s->stream];
    AVCodecParameters *p = stream->codecpar;
    if (p->codec_type != AVMEDIA_TYPE_AUDIO) return fail(s, "unsupported_streams", "selected stream is not audio");
    if (!allowed_codec(p->codec_id)) return fail(s, "unsupported_codec", "selected audio codec is outside the qualified allowlist");
    if (p->extradata_size < 0 || p->extradata_size > MAX_PROBE_BYTES)
        return fail(s, "resource_limit", "selected audio codec extradata exceeds its byte bound");
    if (profile(s, p->codec_id, p->profile, 1) < 0 || rate(s, p->sample_rate) < 0) return -1;
    DeadpanAudioLayout channels;
    if (layout(s, &p->ch_layout, &channels) < 0) return -1;
    if (stream->time_base.num <= 0 || stream->time_base.den <= 0)
        return fail(s, "invalid_time_base", "selected audio stream has no positive time base");
    if (p->initial_padding < 0 || p->trailing_padding < 0 || p->seek_preroll < 0)
        return fail(s, "invalid_padding", "codec padding and preroll observations must be nonnegative");
    if (initial) {
        s->header_layout = channels;
        s->stream_count = s->format->nb_streams;
        s->info = (DeadpanAudioInfo){.stream_index=stream->index,.time_base_num=stream->time_base.num,
            .time_base_den=stream->time_base.den,.sample_rate=p->sample_rate,.channel_layout=channels,
            .stream_start=stream->start_time,.stream_duration=stream->duration,.initial_padding=p->initial_padding,
            .trailing_padding=p->trailing_padding,.seek_preroll=p->seek_preroll};
        (void)snprintf(s->info.codec, sizeof(s->info.codec), "%s", avcodec_get_name(p->codec_id));
    } else if (stream->index != s->info.stream_index || stream->time_base.num != s->info.time_base_num ||
        stream->time_base.den != s->info.time_base_den || p->sample_rate != s->info.sample_rate ||
        !same_layout(&channels, &s->header_layout) || strcmp(avcodec_get_name(p->codec_id), s->info.codec))
        return fail(s, "stream_changed", "selected audio interpretation changed while decoding");
    return 1;
}
// Reject decoder-owned audio buffers before libavcodec allocates their samples.
static int bounded_buffer(AVCodecContext *context, AVFrame *frame, int flags) {
    DeadpanAudio *s = context->opaque;
    if (check(s) < 0 || frame->nb_samples <= 0 ||
        (uint32_t)frame->nb_samples > s->limits.max_samples_per_frame ||
        frame->ch_layout.nb_channels <= 0 || (uint32_t)frame->ch_layout.nb_channels > s->limits.max_channels) {
        fail(s, "resource_limit", "decoder audio allocation exceeds sample or channel bounds");
        return AVERROR(EINVAL);
    }
    return avcodec_default_get_buffer2(context, frame, flags);
}
static int open_impl(DeadpanAudio *s) {
    if (runtime(s) < 0) return -1;
    if (!s->limits.max_input_bytes || s->limits.max_input_bytes > 64ULL*1024*1024*1024 ||
        !s->limits.max_frames || s->limits.max_frames > 10000000 || !s->limits.max_packets || s->limits.max_packets > 40000000 ||
        !s->limits.max_decoded_samples || s->limits.max_decoded_samples > 1000000000000ULL ||
        !s->limits.max_io_bytes_per_call || s->limits.max_io_bytes_per_call > 1024ULL*1024*1024 ||
        !s->limits.max_samples_per_frame || s->limits.max_samples_per_frame > 65536 ||
        !s->limits.max_channels || s->limits.max_channels > 32 ||
        !s->limits.max_sample_rate || s->limits.max_sample_rate > 384000 ||
        !s->limits.max_packets_per_frame || s->limits.max_packets_per_frame > 10000 ||
        !s->limits.max_packet_bytes || s->limits.max_packet_bytes > 16*1024*1024)
        return fail(s, "invalid_configuration", "audio decode limits exceed hard bounds");
    struct stat status;
    if (fstat(s->fd, &status) || !S_ISREG(status.st_mode) || status.st_size != s->length ||
        s->length <= 0 || (uint64_t)s->length > s->limits.max_input_bytes)
        return fail(s, "invalid_input", "audio source must be a nonempty regular snapshot within its byte bound");
    uint8_t *buffer = av_malloc(IO_BUFFER_BYTES);
    if (!buffer) return fail(s, "resource_exhausted", "allocate audio I/O buffer");
    s->io = avio_alloc_context(buffer, IO_BUFFER_BYTES, 0, s, read_descriptor, NULL, seek_descriptor);
    if (!s->io) { av_free(buffer); return fail(s, "resource_exhausted", "allocate audio I/O context"); }
    s->io->seekable = AVIO_SEEKABLE_NORMAL;
    s->format = avformat_alloc_context();
    if (!s->format) return fail(s, "resource_exhausted", "allocate audio demux context");
    s->format->pb = s->io;
    s->format->flags |= AVFMT_FLAG_CUSTOM_IO;
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
        return fail(s, "resource_exhausted", "allocate audio allowlists");
    AVDictionary *options = NULL;
    int result = av_dict_set(&options, "enable_drefs", "0", 0);
    if (result >= 0) result = av_dict_set(&options, "use_absolute_path", "0", 0);
    if (result >= 0) result = avformat_open_input(&s->format, NULL, NULL, &options);
    av_dict_free(&options);
    if (result < 0) return fferror(s, "open audio descriptor", result);
    // The admitted containers carry the selected stream contract in their header.
    // Do not run find_stream_info: it can decode other streams or consume skip
    // evidence before this explicitly controlled decoder is opened.
    if (check(s) < 0 || table(s, 1) < 0) return -1;
    AVStream *stream = s->format->streams[s->stream];
    AVCodecParameters *p = stream->codecpar;
    if (!strcmp(s->format->iformat->name, "wav")) {
        // Header preflight admits only plain PCM16. Preserve FFmpeg's normal
        // packet size unless a tighter caller budget requires smaller blocks.
        int64_t packet_size = 0;
        uint64_t alignment = 2ULL * (uint64_t)p->ch_layout.nb_channels;
        uint64_t ceiling = (uint64_t)s->limits.max_samples_per_frame * alignment;
        if (ceiling > s->limits.max_packet_bytes) ceiling = s->limits.max_packet_bytes;
        if (!alignment || ceiling < alignment ||
            av_opt_get_int(s->format->priv_data, "max_size", 0, &packet_size) < 0 || packet_size <= 0)
            return fail(s, "resource_limit", "WAV packet alignment exceeds its byte budget");
        if ((uint64_t)packet_size > ceiling) packet_size = (int64_t)ceiling;
        packet_size -= packet_size % (int64_t)alignment;
        if (av_opt_set_int(s->format->priv_data, "max_size", packet_size, 0) < 0)
            return fail(s, "invalid_configuration", "could not apply bounded WAV packet size");
    }
    const AVCodec *codec = avcodec_find_decoder_by_name(s->info.codec);
    if (!codec || codec->id != p->codec_id) return fail(s, "unsupported_codec", "qualified software audio decoder is unavailable");
    s->decoder = avcodec_alloc_context3(codec);
    s->packet = av_packet_alloc(); s->frame = av_frame_alloc();
    if (!s->decoder || !s->packet || !s->frame) return fail(s, "resource_exhausted", "allocate audio decoder context");
    if ((result = avcodec_parameters_to_context(s->decoder, p)) < 0) return fferror(s, "copy audio codec parameters", result);
    s->decoder->thread_count = 1;
    s->decoder->thread_type = 0;
    s->decoder->max_samples = (int64_t)s->limits.max_samples_per_frame * s->limits.max_channels;
    s->decoder->err_recognition = AV_EF_EXPLODE | AV_EF_CAREFUL;
    s->decoder->pkt_timebase = stream->time_base;
    s->decoder->flags2 |= AV_CODEC_FLAG2_SKIP_MANUAL;
    s->decoder->opaque = s;
    s->decoder->get_buffer2 = bounded_buffer;
    if ((result = avcodec_open2(s->decoder, codec, NULL)) < 0) return fferror(s, "open audio decoder", result);
    if (profile(s, p->codec_id, s->decoder->profile, 1) < 0) return -1;
    // AAC's AudioSpecificConfig may supply the real speaker layout even when
    // the container header lists only a channel count. This is decoder evidence,
    // not av_channel_layout_default or a count-based speaker guess.
    if (layout(s, &s->decoder->ch_layout, &s->info.channel_layout) < 0 ||
        rate(s, s->decoder->sample_rate) < 0) return -1;
    if (s->decoder->sample_rate != p->sample_rate)
        return fail(s, "stream_changed", "audio decoder disagrees with header sample rate");
    s->info.sample_format = s->decoder->sample_fmt;
    return check(s);
}
void deadpan_audio_close(DeadpanAudio *s) {
    if (!s) return;
    av_frame_free(&s->frame);
    av_packet_free(&s->packet);
    avcodec_free_context(&s->decoder);
    if (s->format) { s->format->pb = NULL; avformat_close_input(&s->format); }
    if (s->io) { av_freep(&s->io->buffer); avio_context_free(&s->io); }
    av_free(s);
}
int deadpan_audio_open(int fd, int64_t length, uint32_t selected, const DeadpanAudioLimits *limits,
    uint64_t preflight_io_bytes, uint64_t timeout, DeadpanCancelled cancelled, const void *opaque, DeadpanAudio **out,
    DeadpanAudioInfo *info, DeadpanSourceError *error) {
    *out = NULL; memset(error, 0, sizeof(*error));
    DeadpanAudio *s = av_mallocz(sizeof(*s));
    if (!s) { (void)snprintf(error->code, sizeof(error->code), "resource_exhausted"); return -1; }
    s->fd = fd; s->length = length; s->limits = *limits;
    s->stream = selected <= INT_MAX ? (int)selected : -1;
    int result = begin(s, timeout, cancelled, opaque, error);
    if (result > 0 && preflight_io_bytes > s->limits.max_io_bytes_per_call)
        result = fail(s, "resource_limit", "header guard exhausted the opening input byte budget");
    s->io_bytes = preflight_io_bytes;
    if (result > 0) result = open_impl(s);
    result = finish(s, result);
    if (result < 0) { deadpan_audio_close(s); return -1; }
    *info = s->info; *out = s; return 1;
}
static int validate_frame(DeadpanAudio *s) {
    AVFrame *f = s->frame;
    if (f->pts == AV_NOPTS_VALUE) return fail(s, "missing_pts", "decoded audio has no original frame PTS");
    if (f->flags & AV_FRAME_FLAG_CORRUPT || f->decode_error_flags)
        return fail(s, "corrupt_frame", "decoder reported corrupt audio");
    if (f->nb_samples <= 0 || (uint32_t)f->nb_samples > s->limits.max_samples_per_frame ||
        s->frames >= s->limits.max_frames || (uint64_t)f->nb_samples > s->limits.max_decoded_samples - s->samples)
        return fail(s, "resource_limit", "decoded audio frame/sample count exceeds configured bound");
    if (rate(s, f->sample_rate) < 0 || profile(s, s->decoder->codec_id, s->decoder->profile, 0) < 0) return -1;
    DeadpanAudioLayout channels;
    if (layout(s, &f->ch_layout, &channels) < 0) return -1;
    if (f->sample_rate != s->info.sample_rate || !same_layout(&channels, &s->info.channel_layout))
        return fail(s, "stream_changed", "decoded audio rate or layout differs from selected stream contract");
    if ((s->decoder->codec_id == AV_CODEC_ID_AAC && f->format != AV_SAMPLE_FMT_FLTP) ||
        (s->decoder->codec_id != AV_CODEC_ID_AAC && f->format != AV_SAMPLE_FMT_S16))
        return fail(s, "unsupported_sample_format", "decoded audio sample representation is unqualified");
    s->current = (DeadpanAudioFrame){.pts=f->pts,.duration=f->duration,.dts=f->pkt_dts,.nb_samples=f->nb_samples,
        .sample_rate=f->sample_rate,.sample_format=f->format,.channel_layout=channels,
        .discard=!!(f->flags & AV_FRAME_FLAG_DISCARD)};
    for (int i = 0; i < f->nb_side_data; i++) {
        AVFrameSideData *data = f->side_data[i];
        if (data->type != AV_FRAME_DATA_SKIP_SAMPLES) continue;
        if (s->current.skip_present || data->size != 10 || !data->data)
            return fail(s, "invalid_padding", "audio skip evidence has an invalid or duplicate payload");
        s->current.skip_present = 1;
        s->current.leading = AV_RL32(data->data);
        s->current.trailing = AV_RL32(data->data + 4);
        s->current.leading_reason = data->data[8];
        s->current.trailing_reason = data->data[9];
    }
    s->frames++; s->samples += (uint64_t)f->nb_samples;
    s->current_valid = 1;
    return 1;
}
static int next_impl(DeadpanAudio *s, DeadpanAudioFrame *out) {
    s->current_valid = 0;
    av_frame_unref(s->frame);
    if (s->ended) return 0;
    uint32_t packets = 0;
    for (uint32_t step = 0; step <= s->limits.max_packets_per_frame + 1; step++) {
        if (check(s) < 0) return -1;
        int result = avcodec_receive_frame(s->decoder, s->frame);
        if (check(s) < 0) return -1;
        if (result == 0) {
            if (table(s, 0) < 0 || validate_frame(s) < 0) return -1;
            *out = s->current; return 1;
        }
        if (result == AVERROR_EOF) { s->ended = 1; return 0; }
        if (result != AVERROR(EAGAIN)) return fferror(s, "receive audio frame", result);
        if (s->draining) return fail(s, "decode_protocol", "audio decoder requested packets after draining");
        for (;;) {
            if (check(s) < 0) return -1;
            if (packets >= s->limits.max_packets_per_frame || s->packets >= s->limits.max_packets)
                return fail(s, "resource_limit", "audio packet count exceeds configured bound");
            result = av_read_frame(s->format, s->packet);
            if (check(s) < 0) { av_packet_unref(s->packet); return -1; }
            if (result == AVERROR_EOF) {
                result = avcodec_send_packet(s->decoder, NULL);
                if (result < 0) return fferror(s, "drain audio decoder", result);
                s->draining = 1; break;
            }
            if (result < 0) return fferror(s, "read audio packet", result);
            packets++; s->packets++;
            if (s->packet->flags & AV_PKT_FLAG_CORRUPT) { av_packet_unref(s->packet); return fail(s, "corrupt_packet", "demuxer reported a corrupt audio-container packet"); }
            if (s->packet->size < 0 || (uint32_t)s->packet->size > s->limits.max_packet_bytes) {
                av_packet_unref(s->packet); return fail(s, "resource_limit", "demuxed packet exceeds configured byte bound");
            }
            uint64_t packet_bytes = (uint64_t)s->packet->size;
            for (int i = 0; i < s->packet->side_data_elems; i++) {
                size_t bytes = s->packet->side_data[i].size;
                if (bytes > s->limits.max_packet_bytes - packet_bytes) {
                    av_packet_unref(s->packet);
                    return fail(s, "resource_limit", "packet payload and side data exceed configured byte bound");
                }
                packet_bytes += bytes;
            }
            if (s->packet->stream_index == s->stream) {
                if (table(s, 0) < 0) { av_packet_unref(s->packet); return -1; }
                result = avcodec_send_packet(s->decoder, s->packet);
                av_packet_unref(s->packet);
                if (result < 0) return fferror(s, "send audio packet", result);
                break;
            }
            av_packet_unref(s->packet);
        }
    }
    return fail(s, "resource_limit", "bounded audio decode progress budget exhausted");
}
int deadpan_audio_next(DeadpanAudio *s, uint64_t timeout, DeadpanCancelled cancelled,
    const void *opaque, DeadpanAudioFrame *frame, DeadpanSourceError *error) {
    if (begin(s, timeout, cancelled, opaque, error) < 0) return finish(s, -1);
    return finish(s, next_impl(s, frame));
}
static int copy_impl(DeadpanAudio *s, DeadpanAudioFrame *out, float *samples, size_t length) {
    if (!s->current_valid) return fail(s, "no_current_frame", "decode an audio frame before copying samples");
    AVFrame *f = s->frame;
    size_t channels = (size_t)f->ch_layout.nb_channels;
    if (!samples || length != (size_t)f->nb_samples * channels)
        return fail(s, "invalid_output", "audio output has an incorrect sample count");
    if (!f->extended_data) return fail(s, "invalid_frame", "decoded audio has no planes");
    int planar = f->format == AV_SAMPLE_FMT_FLTP;
    for (size_t c = 0; c < (planar ? channels : 1); c++)
        if (!f->extended_data[c]) return fail(s, "invalid_frame", "decoded audio has a missing plane");
    for (size_t n = 0; n < (size_t)f->nb_samples; n++) {
        if ((n & 1023) == 0 && check(s) < 0) return -1;
        for (size_t c = 0; c < channels; c++) {
            float value = planar ? ((const float *)f->extended_data[c])[n] :
                (float)((const int16_t *)f->extended_data[0])[n * channels + c] / 32768.0f;
            if (!isfinite(value)) return fail(s, "invalid_samples", "decoded audio contains a non-finite sample");
            samples[n * channels + c] = value;
        }
    }
    *out = s->current;
    return check(s);
}
int deadpan_audio_copy(DeadpanAudio *s, uint64_t timeout, DeadpanCancelled cancelled,
    const void *opaque, DeadpanAudioFrame *frame, float *samples, size_t length, DeadpanSourceError *error) {
    if (begin(s, timeout, cancelled, opaque, error) < 0) return finish(s, -1);
    return finish(s, copy_impl(s, frame, samples, length));
}
