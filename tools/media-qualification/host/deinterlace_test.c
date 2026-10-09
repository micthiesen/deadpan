/* Direct native contract checks; run with run_deinterlace_test.py (ASan/UBSan). */
#include "deinterlace.h"
#include <assert.h>
#include <errno.h>
#include <limits.h>
#include <stdio.h>
#include <libavutil/error.h>
#include <libavutil/intreadwrite.h>

static AVFrame *picture(int bits, int top, int interlaced, int64_t pts, int64_t duration) {
    AVFrame *f = av_frame_alloc();
    assert(f);
    f->width = 16; f->height = 16;
    f->format = bits == 8 ? AV_PIX_FMT_YUV420P : AV_PIX_FMT_YUV420P10LE;
    f->sample_aspect_ratio = (AVRational){1,1};
    f->color_range = AVCOL_RANGE_MPEG;
    f->colorspace = bits == 8 ? AVCOL_SPC_BT709 : AVCOL_SPC_BT2020_NCL;
    f->color_trc = bits == 8 ? AVCOL_TRC_BT709 : AVCOL_TRC_SMPTE2084;
    f->color_primaries = bits == 8 ? AVCOL_PRI_BT709 : AVCOL_PRI_BT2020;
    f->pts = pts; f->duration = duration; f->pkt_dts = AV_NOPTS_VALUE;
    f->flags = (interlaced ? AV_FRAME_FLAG_INTERLACED : 0) |
        (top ? AV_FRAME_FLAG_TOP_FIELD_FIRST : 0) | AV_FRAME_FLAG_KEY;
    assert(av_frame_get_buffer(f, 32) == 0);
    for (int plane = 0; plane < 3; plane++) {
        int size = plane ? 8 : 16;
        for (int y = 0; y < size; y++) {
            unsigned value = (y & 1) ? 200 : 20;
            for (int x = 0; x < size; x++) {
                uint8_t *p = f->data[plane] + y * f->linesize[plane] + x * (bits == 8 ? 1 : 2);
                if (bits == 8) *p = value;
                else AV_WL16(p, value * 4);
            }
        }
    }
    return f;
}

static void single_fields(void) {
    for (int bits = 8; bits <= 10; bits += 2) {
        for (int top = 0; top <= 1; top++) {
            AVFrame *in = picture(bits, top, 1, -101, 31), *out = av_frame_alloc();
            DeadpanFields *f = NULL;
            assert(deadpan_fields_open(&f, in, 1) == 0);
            assert(deadpan_fields_push(f, in) == 0);
            assert(deadpan_fields_pull(f, out) == AVERROR(EAGAIN));
            assert(deadpan_fields_flush(f) == 0);
            for (int phase = 0; phase < 2; phase++) {
                assert(deadpan_fields_pull(f, out) == 0);
                assert(out->pts == -202 + phase * 31 && out->duration == 31);
                assert(!(out->flags & AV_FRAME_FLAG_INTERLACED));
                assert(!!(out->flags & AV_FRAME_FLAG_KEY) == !phase);
                assert(out->color_trc == (bits == 8 ? AVCOL_TRC_BT709 : AVCOL_TRC_SMPTE2084));
                unsigned expected = ((top ^ phase) ? 20 : 200) * (bits == 8 ? 1 : 4);
                for (int plane = 0; plane < 3; plane++) {
                    int size = plane ? 8 : 16;
                    for (int y = 0; y < size; y++) for (int x = 0; x < size; x++) {
                        uint8_t *p = out->data[plane] + y * out->linesize[plane] + x * (bits == 8 ? 1 : 2);
                        assert((bits == 8 ? *p : AV_RL16(p)) == expected);
                    }
                }
            }
            assert(deadpan_fields_pull(f, out) == AVERROR_EOF);
            deadpan_fields_close(&f); av_frame_free(&in); av_frame_free(&out);
        }
    }
}

static void mixed_clocks(void) {
    const int64_t input_pts[] = {-101, -71, -40, 0};
    const int64_t pts[] = {-202, -172, -142, -80, -40, 0, 37};
    const int64_t duration[] = {30, 30, 62, 40, 40, 37, 37};
    AVFrame *out = av_frame_alloc();
    DeadpanFields *f = NULL;
    unsigned seen = 0;
    for (int i = 0; i <= 4; i++) {
        if (i < 4) {
            AVFrame *in = picture(10, 1, i != 1, input_pts[i], i == 3 ? 37 : 0);
            if (!f) assert(deadpan_fields_open(&f, in, 8) == 0);
            assert(deadpan_fields_push(f, in) == 0);
            av_frame_free(&in);
        } else assert(deadpan_fields_flush(f) == 0);
        int result;
        while ((result = deadpan_fields_pull(f, out)) == 0) {
            assert(seen < 7 && out->pts == pts[seen] && out->duration == duration[seen]);
            seen++;
        }
        assert(result == (i == 4 ? AVERROR_EOF : AVERROR(EAGAIN)));
    }
    assert(seen == 7);
    deadpan_fields_close(&f); av_frame_free(&out);
}

static void bad_clocks(void) {
    for (int kind = 0; kind < 4; kind++) {
        AVFrame *in = picture(8, 1, 1, kind == 0 ? INT64_MAX / 2 : 0, kind == 1 ? 0 : 3);
        if (kind == 2) in->pkt_dts = INT64_MIN / 2;
        if (kind == 3) in->pts = INT64_MIN / 2;
        AVFrame *out = av_frame_alloc();
        DeadpanFields *f = NULL;
        assert(deadpan_fields_open(&f, in, 1) == 0);
        assert(deadpan_fields_push(f, in) == 0);
        assert(deadpan_fields_flush(f) == 0);
        assert(deadpan_fields_pull(f, out) == (kind == 1 ? AVERROR(ENODATA) : AVERROR(EOVERFLOW)));
        deadpan_fields_close(&f); av_frame_free(&in); av_frame_free(&out);
    }
    AVFrame *in = picture(8, 1, 1, 0, 3);
    DeadpanFields *f = NULL;
    assert(deadpan_fields_open(&f, in, 1) == 0);
    assert(deadpan_fields_push(f, in) == 0);
    av_frame_free(&in);
    in = picture(8, 1, 1, 0, 3);
    assert(deadpan_fields_push(f, in) == AVERROR_INVALIDDATA);
    deadpan_fields_close(&f); av_frame_free(&in);
    in = picture(8, 1, 1, 0, 3);
    in->repeat_pict = 1;
    assert(deadpan_fields_open(&f, in, 1) == 0);
    assert(deadpan_fields_push(f, in) == AVERROR_INVALIDDATA);
    deadpan_fields_close(&f); av_frame_free(&in);
}

int main(void) {
    single_fields(); mixed_clocks(); bad_clocks();
    puts("single TFF/BFF 8/10-bit spatial fields, mixed negative/odd clocks, terminal duration, overflow/sentinel refusals: passed");
    return 0;
}
