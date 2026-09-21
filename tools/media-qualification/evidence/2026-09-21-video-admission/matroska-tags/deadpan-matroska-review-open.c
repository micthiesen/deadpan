#include <stdio.h>
#include <libavformat/avformat.h>
#include <libavutil/dict.h>
int main(int argc,char **argv){setbuf(stdout,NULL);for(int i=1;i<argc;i++){AVFormatContext *c=NULL; int r=avformat_open_input(&c,argv[i],NULL,NULL);printf("%s open=%d metadata_entries=%d\n",argv[i],r,c?av_dict_count(c->metadata):0);avformat_close_input(&c);}return 0;}
