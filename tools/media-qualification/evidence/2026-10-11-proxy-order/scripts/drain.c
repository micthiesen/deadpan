/* Diagnostic-only replacement for proxy_write_packets. Records the actual
   packet sequence and independently decodes its barcode pictures. It does not
   mux a usable proxy or relax any production check. */
static AVCodecContext *probe_decoder;
static AVFrame *probe_frame;
static FILE *probe_raw;
static uint64_t probe_decoded;

static int probe_read_pictures(void) {
    for (;;) {
        int code = avcodec_receive_frame(probe_decoder, probe_frame);
        if (code == AVERROR_EOF || code == AVERROR(EAGAIN)) return 1;
        if (code < 0) return fail_ffmpeg("probe decode", code);
        if (probe_frame->width != 256 || probe_frame->height != 144 ||
            probe_frame->format != AV_PIX_FMT_YUV420P)
            return fail("probe", "unexpected decoded raster");
        unsigned identity = 0;
        for (unsigned bit = 0; bit < 8; bit++) {
            unsigned x = bit * 32 + 16;
            unsigned y = 72;
            if (probe_frame->data[0][y * probe_frame->linesize[0] + x] > 128)
                identity |= 1U << bit;
        }
        printf("{\"kind\":\"decoded\",\"ordinal\":%" PRIu64 ",\"pts\":%" PRId64
               ",\"picture_id\":%u}\n", probe_decoded++, probe_frame->pts, identity);
        av_frame_unref(probe_frame);
    }
}

static int proxy_write_packets(void) {
    for (;;) {
        int code = avcodec_receive_packet(proxy.encoder, proxy.packet);
        if (code == AVERROR(EAGAIN) || code == AVERROR_EOF) return 1;
        if (code < 0) return proxy_encoder_failure("probe receive", code);
        if (!proxy.timing_count) return fail("probe", "unrequested picture");
        int64_t expected = proxy.timing_pts[proxy.timing_head];
        proxy.timing_head = (proxy.timing_head + 1) % PROXY_TIMING_QUEUE;
        proxy.timing_count--;
        printf("{\"kind\":\"packet\",\"ordinal\":%" PRIu64 ",\"pts\":%" PRId64
               ",\"dts\":%" PRId64 ",\"expected_pts\":%" PRId64
               ",\"key\":%d,\"bytes\":%d}\n", proxy.packets++, proxy.packet->pts,
               proxy.packet->dts, expected, !!(proxy.packet->flags & AV_PKT_FLAG_KEY), proxy.packet->size);
        if (proxy.packet->size <= 0 || proxy.packet->size > 4 * 1024 * 1024 ||
            fwrite(proxy.packet->data, 1, proxy.packet->size, probe_raw) != (size_t)proxy.packet->size)
            return fail("probe", "cannot retain bounded raw packet");
        code = avcodec_send_packet(probe_decoder, proxy.packet);
        av_packet_unref(proxy.packet);
        if (code < 0) return fail_ffmpeg("probe send", code);
        if (!probe_read_pictures()) return 0;
        if (!within_deadline()) return 0;
    }
}
