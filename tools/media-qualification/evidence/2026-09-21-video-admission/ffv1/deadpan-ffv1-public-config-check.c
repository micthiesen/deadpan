
#include <stdio.h>
#include <stdlib.h>
#include <libavcodec/avcodec.h>
#include <libavutil/mem.h>
int main(int argc, char **argv) {
    int failures = 0;
    for (int i = 1; i < argc; ++i) {
        FILE *input = fopen(argv[i], "rb");
        if (!input) return 2;
        unsigned char bytes[65537];
        size_t length = fread(bytes, 1, sizeof(bytes), input);
        fclose(input);
        if (length < 6 || length > 65536) return 3;
        const AVCodec *codec = avcodec_find_decoder(AV_CODEC_ID_FFV1);
        AVCodecContext *ctx = avcodec_alloc_context3(codec);
        if (!ctx) return 4;
        ctx->width = 8192; ctx->height = 2048;
        ctx->max_pixels = 16777216;
        ctx->thread_count = 1; ctx->thread_type = 0;
        ctx->extradata = av_mallocz(length + AV_INPUT_BUFFER_PADDING_SIZE);
        if (!ctx->extradata) return 5;
        memcpy(ctx->extradata, bytes, length);
        ctx->extradata_size = (int)length;
        int result = avcodec_open2(ctx, codec, NULL);
        printf("%s bytes=%zu avcodec_open2=%d\n", argv[i], length, result);
        failures += result < 0;
        avcodec_free_context(&ctx);
    }
    return failures != 0;
}
