#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/hwcontext.h>
#include <libavutil/imgutils.h>
#include <libavutil/pixdesc.h>
#include <libavutil/sha.h>
#include <libswscale/swscale.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static int hardware;
static enum AVPixelFormat format(AVCodecContext *ctx, const enum AVPixelFormat *formats) {
    (void)ctx;
    for(int i=0; i<64 && formats[i]!=AV_PIX_FMT_NONE; i++) {
        if(hardware && formats[i]==AV_PIX_FMT_VIDEOTOOLBOX)return formats[i];
        const AVPixFmtDescriptor *desc=av_pix_fmt_desc_get(formats[i]);
        if(!hardware && desc && !(desc->flags & AV_PIX_FMT_FLAG_HWACCEL))return formats[i];
    }
    return AV_PIX_FMT_NONE;
}
static int picture(AVFrame *frame,int number,AVCodecContext *codec) {
    if (!number) fprintf(stderr,"software format=%s field_order=%d profile=%d\n",av_get_pix_fmt_name(codec->sw_pix_fmt),codec->field_order,codec->profile);
    const char *native=av_get_pix_fmt_name(frame->format);
    int w=frame->width,h=frame->height;
    size_t crop[4]={frame->crop_left,frame->crop_top,frame->crop_right,frame->crop_bottom};
    AVFrame *pixels=frame,*transfer=NULL;
    if(hardware){
        if(frame->format!=AV_PIX_FMT_VIDEOTOOLBOX)return 0;
        transfer=av_frame_alloc();if(!transfer)return 0;
        if(av_hwframe_transfer_data(transfer,frame,0)<0 || av_frame_copy_props(transfer,frame)<0)return 0;
        pixels=transfer;
    }
    if(av_frame_apply_cropping(pixels,AV_FRAME_CROP_UNALIGNED)<0)return 0;
    const AVPixFmtDescriptor *desc=av_pix_fmt_desc_get(pixels->format);
    if(!desc||pixels->width<1||pixels->height<1||pixels->width>8192||pixels->height>8192)return 0;
    enum AVPixelFormat dst=desc->comp[0].depth>8 ? AV_PIX_FMT_YUV420P10LE : AV_PIX_FMT_YUV420P;
    uint8_t *data[4]={0};int lines[4]={0};
    int length=av_image_alloc(data,lines,pixels->width,pixels->height,dst,1);
    if(length<0)return 0;
    struct SwsContext *sws=sws_getContext(pixels->width,pixels->height,pixels->format,pixels->width,pixels->height,dst,SWS_POINT|SWS_BITEXACT,NULL,NULL,NULL);
    if(!sws)return 0;
    const int *coefficients=sws_getCoefficients(SWS_CS_ITU709);
    int full=pixels->color_range==AVCOL_RANGE_JPEG;
    if(sws_setColorspaceDetails(sws,coefficients,full,coefficients,full,0,1<<16,1<<16)<0)return 0;
    int rows=sws_scale(sws,(const uint8_t *const*)pixels->data,pixels->linesize,0,pixels->height,data,lines);
    if(rows!=pixels->height)return 0;
    struct AVSHA *sha=av_sha_alloc();uint8_t digest[32];if(!sha)return 0;
    av_sha_init(sha,256);av_sha_update(sha,data[0],length);av_sha_final(sha,digest);char hex[65];
    for(int i=0;i<32;i++)snprintf(hex+i*2,3,"%02x",digest[i]);
    printf("{\"ordinal\":%d,\"pts\":%lld,\"duration\":%lld,\"native\":\"%s\",\"native_size\":[%d,%d],\"crop\":[%zu,%zu,%zu,%zu],\"transfer\":\"%s\",\"size\":[%d,%d],\"range\":%d,\"matrix\":%d,\"chroma\":%d,\"normalized\":\"%s\",\"sha256\":\"%s\"}\n",number,(long long)frame->pts,(long long)frame->duration,native,w,h,crop[0],crop[1],crop[2],crop[3],av_get_pix_fmt_name(pixels->format),pixels->width,pixels->height,frame->color_range,frame->colorspace,frame->chroma_location,av_get_pix_fmt_name(dst),hex);
    av_free(sha);sws_freeContext(sws);av_freep(&data[0]);av_frame_free(&transfer);return 1;
}
int main(int argc,char **argv) {
    if(argc!=3)return 2;hardware=!strcmp(argv[1],"vt");
    AVFormatContext *input=NULL;AVCodecContext *codec=NULL;AVPacket *packet=av_packet_alloc();AVFrame *frame=av_frame_alloc();
    if(!packet||!frame||avformat_open_input(&input,argv[2],NULL,NULL)<0||avformat_find_stream_info(input,NULL)<0)return 3;
    int stream=av_find_best_stream(input,AVMEDIA_TYPE_VIDEO,-1,-1,NULL,0);if(stream<0)return 4;
    const AVCodec *decoder=avcodec_find_decoder(input->streams[stream]->codecpar->codec_id);
    codec=avcodec_alloc_context3(decoder);if(!codec)return 5;
    if(avcodec_parameters_to_context(codec,input->streams[stream]->codecpar)<0)return 6;
    codec->get_format=format;codec->thread_count=1;codec->thread_type=0;codec->apply_cropping=0;codec->pkt_timebase=input->streams[stream]->time_base;
    if(hardware&&av_hwdevice_ctx_create(&codec->hw_device_ctx,AV_HWDEVICE_TYPE_VIDEOTOOLBOX,NULL,NULL,0)<0)return 7;
    if(avcodec_open2(codec,decoder,NULL)<0)return 8;
    int count=0,ret=0,eof=0;
    while(!eof){
        ret=av_read_frame(input,packet);
        if(ret<0){if(ret!=AVERROR_EOF)return 9;eof=1;ret=avcodec_send_packet(codec,NULL);}
        else if(packet->stream_index==stream)ret=avcodec_send_packet(codec,packet);
        else {av_packet_unref(packet);continue;}
        av_packet_unref(packet);if(ret<0)return 10;
        while((ret=avcodec_receive_frame(codec,frame))>=0){if(!picture(frame,count++,codec))return 11;av_frame_unref(frame);}
        if(ret!=AVERROR(EAGAIN)&&ret!=AVERROR_EOF)return 12;
    }
    av_frame_free(&frame);av_packet_free(&packet);avcodec_free_context(&codec);avformat_close_input(&input);
    fprintf(stderr,"decoded %d pictures via %s\n",count,hardware?"VideoToolbox":"software");return count?0:13;
}
