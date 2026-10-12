// Isolated codec diagnostic. All decoded samples retain their packet-derived PTS.
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/opt.h>
#include <libavutil/error.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
static void check(int code, const char *label) {
    if(code<0) { char error[AV_ERROR_MAX_STRING_SIZE]; av_strerror(code,error,sizeof(error)); fprintf(stderr,"%s: %s\n",label,error); exit(1); }
}
static void write_bytes(FILE *out,const void *value,size_t bytes) { if(fwrite(value,1,bytes,out)!=bytes) { perror("write"); exit(1); } }
static void drain_decoder(AVCodecContext *decoder, FILE *out) {
    AVFrame *frame=av_frame_alloc(); if(!frame) exit(1);
    int result;
    while((result=avcodec_receive_frame(decoder,frame))>=0) {
        if(frame->format!=AV_SAMPLE_FMT_FLTP || frame->ch_layout.nb_channels!=2 || frame->sample_rate!=48000 || frame->pts==AV_NOPTS_VALUE) { fprintf(stderr,"unexpected decoded format/PTS\n"); exit(1); }
        int64_t pts=frame->pts; uint32_t count=frame->nb_samples;
        write_bytes(out,&pts,sizeof(pts)); write_bytes(out,&count,sizeof(count));
        for(uint32_t i=0;i<count;i++) {
            float pair[2]={((float*)frame->extended_data[0])[i],((float*)frame->extended_data[1])[i]};
            write_bytes(out,pair,sizeof(pair));
        }
        av_frame_unref(frame);
    }
    if(result!=AVERROR(EAGAIN) && result!=AVERROR_EOF) check(result,"decode frame");
    av_frame_free(&frame);
}
static void drain_encoder(AVCodecContext *encoder,AVCodecContext *decoder,FILE *out) {
    AVPacket *packet=av_packet_alloc(); if(!packet) exit(1);
    int result;
    while((result=avcodec_receive_packet(encoder,packet))>=0) {
        check(avcodec_send_packet(decoder,packet),"decode packet"); drain_decoder(decoder,out); av_packet_unref(packet);
    }
    if(result!=AVERROR(EAGAIN) && result!=AVERROR_EOF) check(result,"encode packet");
    av_packet_free(&packet);
}
int main(int argc,char **argv) {
    if(argc<4) { fprintf(stderr,"probe encode|decode input output [key=value ...]\n"); return 1; }
    FILE *out=fopen(argv[3],"wb"); if(!out) return 1;
    AVCodecContext *decoder=avcodec_alloc_context3(avcodec_find_decoder(AV_CODEC_ID_AAC)); if(!decoder) return 1;
    decoder->thread_count=1;
    decoder->pkt_timebase=(AVRational){1,48000};
    // Expose physical priming, like Deadpan's Manual decode path.
    decoder->flags2|=AV_CODEC_FLAG2_SKIP_MANUAL;
    if(!strcmp(argv[1],"decode")) {
        AVFormatContext *format=NULL; check(avformat_open_input(&format,argv[2],NULL,NULL),"open file");
        check(avformat_find_stream_info(format,NULL),"stream info");
        int stream=av_find_best_stream(format,AVMEDIA_TYPE_AUDIO,-1,-1,NULL,0); check(stream,"audio stream");
        if(format->streams[stream]->time_base.num!=1 || format->streams[stream]->time_base.den!=48000) return 1;
        check(avcodec_parameters_to_context(decoder,format->streams[stream]->codecpar),"decoder parameters");
        check(avcodec_open2(decoder,NULL,NULL),"open decoder");
        AVPacket *packet=av_packet_alloc(); if(!packet) return 1;
        int result;
        while((result=av_read_frame(format,packet))>=0) {
            if(packet->stream_index==stream) { check(avcodec_send_packet(decoder,packet),"file packet"); drain_decoder(decoder,out); }
            av_packet_unref(packet);
        }
        if(result!=AVERROR_EOF) check(result,"read packet");
        check(avcodec_send_packet(decoder,NULL),"flush decoder"); drain_decoder(decoder,out);
        av_packet_free(&packet); avformat_close_input(&format);
    } else if(!strcmp(argv[1],"encode")) {
        FILE *input=fopen(argv[2],"rb"); if(!input) return 1;
        if(fseek(input,0,SEEK_END)) return 1;
        long bytes=ftell(input); if(bytes<=0 || bytes>1024*1024 || bytes%8) return 1;
        rewind(input); float *pcm=malloc(bytes); if(!pcm || fread(pcm,1,bytes,input)!=(size_t)bytes) return 1;
        fclose(input);
        int64_t samples=bytes/8;
        AVCodecContext *encoder=avcodec_alloc_context3(avcodec_find_encoder_by_name("aac")); if(!encoder) return 1;
        encoder->thread_count=1; encoder->flags|=AV_CODEC_FLAG_GLOBAL_HEADER;
        encoder->sample_rate=48000; encoder->sample_fmt=AV_SAMPLE_FMT_FLTP;
        av_channel_layout_default(&encoder->ch_layout,2);
        encoder->time_base=(AVRational){1,48000}; encoder->bit_rate=384000; encoder->profile=AV_PROFILE_AAC_LOW;
        AVDictionary *options=NULL;
        for(int i=4;i<argc;i++) { char *separator=strchr(argv[i],'='); if(!separator) return 1; *separator=0; check(av_dict_set(&options,argv[i],separator+1,0),"option"); }
        check(avcodec_open2(encoder,NULL,&options),"open encoder"); if(av_dict_count(options)) { fprintf(stderr,"unconsumed options\n"); return 1; }
        AVCodecParameters *parameters=avcodec_parameters_alloc(); if(!parameters) return 1;
        check(avcodec_parameters_from_context(parameters,encoder),"encode parameters");
        check(avcodec_parameters_to_context(decoder,parameters),"decode parameters"); avcodec_parameters_free(&parameters);
        check(avcodec_open2(decoder,NULL,NULL),"open decoder");
        AVFrame *frame=av_frame_alloc(); if(!frame) return 1;
        frame->nb_samples=1024; frame->format=AV_SAMPLE_FMT_FLTP; frame->sample_rate=48000;
        check(av_channel_layout_copy(&frame->ch_layout,&encoder->ch_layout),"layout"); check(av_frame_get_buffer(frame,0),"frame buffer");
        for(int64_t first=-1024;first<samples;first+=1024) {
            frame->nb_samples=1024; check(av_frame_make_writable(frame),"writable frame");
            int count=first<0?1024:(int)((samples-first)<1024?samples-first:1024);
            for(int i=0;i<count;i++) for(int channel=0;channel<2;channel++) ((float*)frame->extended_data[channel])[i]=first<0?0.0f:pcm[(first+i)*2+channel];
            frame->pts=first; frame->duration=count; frame->nb_samples=count;
            check(avcodec_send_frame(encoder,frame),"encode frame"); drain_encoder(encoder,decoder,out);
        }
        check(avcodec_send_frame(encoder,NULL),"flush encoder"); drain_encoder(encoder,decoder,out);
        check(avcodec_send_packet(decoder,NULL),"flush decoder"); drain_decoder(decoder,out);
        av_frame_free(&frame); avcodec_free_context(&encoder); free(pcm); av_dict_free(&options);
    } else return 1;
    avcodec_free_context(&decoder); if(fclose(out)) return 1; return 0;
}
