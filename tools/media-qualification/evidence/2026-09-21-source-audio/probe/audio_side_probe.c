#include <inttypes.h>
#include <stdio.h>
#include <libavcodec/avcodec.h>
#include <libavcodec/packet.h>
#include <libavformat/avformat.h>
#include <libavutil/frame.h>
#include <libavutil/intreadwrite.h>

int main(int argc, char **argv) {
    AVFormatContext *fmt = NULL;
    AVCodecContext *codec = NULL;
    AVPacket *pkt = av_packet_alloc();
    AVFrame *frame = av_frame_alloc();
    int ret, stream;
    if (argc != 2 || !pkt || !frame) return 2;
    if ((ret = avformat_open_input(&fmt, argv[1], NULL, NULL)) < 0) return ret;
    if ((ret = avformat_find_stream_info(fmt, NULL)) < 0) return ret;
    stream = av_find_best_stream(fmt, AVMEDIA_TYPE_AUDIO, -1, -1, NULL, 0);
    if (stream < 0) return stream;
    const AVCodecParameters *par = fmt->streams[stream]->codecpar;
    const AVCodec *decoder = avcodec_find_decoder(par->codec_id);
    codec = avcodec_alloc_context3(decoder);
    if (!codec) return 3;
    if ((ret = avcodec_parameters_to_context(codec, par)) < 0) return ret;
    codec->pkt_timebase = fmt->streams[stream]->time_base;
    codec->flags2 |= AV_CODEC_FLAG2_SKIP_MANUAL;
    if ((ret = avcodec_open2(codec, decoder, NULL)) < 0) return ret;
    printf("stream tb=%d/%d rate=%d channels=%d initial=%d trailing=%d preroll=%d delay=%d\\n",
           fmt->streams[stream]->time_base.num, fmt->streams[stream]->time_base.den,
           par->sample_rate, par->ch_layout.nb_channels, par->initial_padding,
           par->trailing_padding, par->seek_preroll, codec->delay);
    int done = 0;
    while (!done) {
        ret = avcodec_receive_frame(codec, frame);
        if (ret == AVERROR(EAGAIN)) {
            ret = av_read_frame(fmt, pkt);
            if (ret == AVERROR_EOF) { avcodec_send_packet(codec, NULL); done = 1; continue; }
            if (ret < 0) return ret;
            if (pkt->stream_index == stream) ret = avcodec_send_packet(codec, pkt);
            av_packet_unref(pkt);
            if (ret < 0) return ret;
            continue;
        }
        if (ret == AVERROR_EOF) break;
        if (ret < 0) return ret;
        AVFrameSideData *side = av_frame_get_side_data(frame, AV_FRAME_DATA_SKIP_SAMPLES);
        if (side && side->size >= 10)
            printf("frame pts=%"PRId64" dts=%"PRId64" duration=%"PRId64" nb=%d skip=%u discard=%u sr=%d ch=%d size=%zu\\n",
                   frame->pts, frame->pkt_dts, frame->duration, frame->nb_samples,
                   AV_RL32(side->data), AV_RL32(side->data+4), frame->sample_rate,
                   frame->ch_layout.nb_channels, side->size);
        else
            printf("frame pts=%"PRId64" dts=%"PRId64" duration=%"PRId64" nb=%d side=none sr=%d ch=%d\\n",
                   frame->pts, frame->pkt_dts, frame->duration, frame->nb_samples,
                   frame->sample_rate, frame->ch_layout.nb_channels);
        av_frame_unref(frame);
    }
    av_frame_free(&frame); av_packet_free(&pkt); avcodec_free_context(&codec); avformat_close_input(&fmt); return 0;
}
