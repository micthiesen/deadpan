#include <fcntl.h>

int main(int argc, char **argv) {
    if (argc != 3) return 2;
    int fd = open(argv[1], O_CREAT | O_EXCL | O_RDWR, 0600);
    if (fd < 0) return 3;
    probe_raw = fopen(argv[2], "wbx");
    if (!probe_raw) return 4;
    const AVCodec *decoder = avcodec_find_decoder_by_name("h264");
    probe_decoder = avcodec_alloc_context3(decoder);
    probe_frame = av_frame_alloc();
    if (!probe_decoder || !probe_frame) return 5;
    probe_decoder->thread_count = 1;
    if (avcodec_open2(probe_decoder, decoder, NULL) < 0) return 6;
    DeadpanProxyRequest request = {
        .source_width=256, .source_height=144, .width=256, .height=144,
        .time_base_num=1, .time_base_den=30000, .sar_num=1, .sar_den=1,
        .transfer=AVCOL_TRC_BT709, .primaries=AVCOL_PRI_BT709, .quality=60,
        .frames=120, .max_output_bytes=16U*1024*1024, .timeout_ms=20000,
    };
    DeadpanConversionError error = {0};
    if (deadpan_proxy_open(fd, &request, &error)) goto failed;
    uint8_t *rgba = malloc(256U * 144U * 4U);
    if (!rgba) return 7;
    for (unsigned frame = 0; frame < 120; frame++) {
        for (unsigned y = 0; y < 144; y++) {
            for (unsigned x = 0; x < 256; x++) {
                uint8_t value = (frame & (1U << (x / 32))) ? 224 : 32;
                unsigned at = (y * 256 + x) * 4;
                rgba[at]=value; rgba[at+1]=value; rgba[at+2]=value; rgba[at+3]=255;
            }
        }
        if (deadpan_proxy_push(rgba, 256U*144U*4U, 256U*4U, frame*1001, 1001, &error)) goto failed;
    }
    if (avcodec_send_frame(proxy.encoder, NULL) < 0 || !proxy_write_packets()) goto failed;
    if (avcodec_send_packet(probe_decoder, NULL) < 0 || !probe_read_pictures()) goto failed;
    printf("{\"kind\":\"finished\",\"packets\":%" PRIu64 ",\"decoded\":%" PRIu64 "}\n", proxy.packets, probe_decoded);
    fflush(stdout);
    fclose(probe_raw);
    close(fd);
    /* Like the production proxy worker, exit without calling VT teardown. */
    _Exit(proxy.packets == 120 && probe_decoded == 120 ? 0 : 8);
failed:
    fprintf(stderr, "%s: %s\n", error.code, error.message);
    fflush(stdout);
    fclose(probe_raw);
    close(fd);
    _Exit(9);
}
