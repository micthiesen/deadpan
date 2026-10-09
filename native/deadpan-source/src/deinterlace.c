/* The progressive picture view of an admitted interlaced source. */
#include "deinterlace.h"
#include <errno.h>
#include <limits.h>
#include <stdint.h>
#include <stdio.h>
#include <libavfilter/avfilter.h>
#include <libavfilter/buffersink.h>
#include <libavfilter/buffersrc.h>
#include <libavutil/error.h>
#include <libavutil/mem.h>
#include <libavutil/pixdesc.h>
#include <libavutil/intreadwrite.h>

#define FIELD_QUEUE 3
typedef struct {
    int64_t ordinal, pts, duration, dts;
    int keyframe, interlaced, fields;
} FieldClock;
struct DeadpanFields {
    AVFilterGraph *graph;
    AVFilterContext *source, *sink;
    AVFrame *single, *repeated;
    FieldClock clocks[FIELD_QUEUE];
    unsigned head, count;
    int64_t inputs, last_pts;
    int flushed, phase;
};

void deadpan_fields_close(DeadpanFields **fields) {
    if (!*fields) return;
    avfilter_graph_free(&(*fields)->graph);
    av_frame_free(&(*fields)->single);
    av_frame_free(&(*fields)->repeated);
    av_freep(fields);
}

int deadpan_fields_open(DeadpanFields **out, const AVFrame *first, unsigned threads) {
    *out = NULL;
    DeadpanFields *f = av_mallocz(sizeof(*f));
    if (!f) return AVERROR(ENOMEM);
    int result = AVERROR(ENOMEM);
    f->graph = avfilter_graph_alloc();
    if (!f->graph) goto failed;
    f->graph->nb_threads = (int)threads;
    avfilter_graph_set_auto_convert(f->graph, AVFILTER_AUTO_CONVERT_NONE);
    AVFilterContext *filter;
    char arguments[256];
    int written = snprintf(arguments, sizeof(arguments),
        "video_size=%dx%d:pix_fmt=%d:time_base=1/1:pixel_aspect=%d/%d:frame_rate=1/1:colorspace=%d:range=%d",
        first->width, first->height, first->format,
        first->sample_aspect_ratio.num ? first->sample_aspect_ratio.num : 1,
        first->sample_aspect_ratio.num ? first->sample_aspect_ratio.den : 1,
        first->colorspace, first->color_range);
    if (written < 0 || (size_t)written >= sizeof(arguments)) {
        result = AVERROR(EINVAL); goto failed;
    }
    const AVFilter *source = avfilter_get_by_name("buffer");
    const AVFilter *bwdif = avfilter_get_by_name("bwdif");
    const AVFilter *sink = avfilter_get_by_name("buffersink");
    if (!source || !bwdif || !sink) { result = AVERROR_FILTER_NOT_FOUND; goto failed; }
    if ((result = avfilter_graph_create_filter(&f->source, source, "source", arguments, NULL, f->graph)) < 0 ||
        (result = avfilter_graph_create_filter(&filter, bwdif, "fields",
            "mode=send_field:parity=auto:deint=interlaced", NULL, f->graph)) < 0 ||
        (result = avfilter_graph_create_filter(&f->sink, sink, "sink", NULL, NULL, f->graph)) < 0 ||
        (result = avfilter_link(f->source, 0, filter, 0)) < 0 ||
        (result = avfilter_link(filter, 0, f->sink, 0)) < 0 ||
        (result = avfilter_graph_config(f->graph, NULL)) < 0) goto failed;
    AVRational clock = av_buffersink_get_time_base(f->sink);
    if (clock.num != 1 || clock.den != 2 ||
        av_buffersink_get_format(f->sink) != first->format ||
        av_buffersink_get_w(f->sink) != first->width ||
        av_buffersink_get_h(f->sink) != first->height) {
        result = AVERROR_INVALIDDATA; goto failed;
    }
    *out = f;
    return 0;
failed:
    deadpan_fields_close(&f);
    return result;
}

int deadpan_fields_push(DeadpanFields *f, AVFrame *input) {
    if (f->flushed || f->count == FIELD_QUEUE || f->inputs >= 10000000 ||
        input->repeat_pict < 0 || input->repeat_pict > 4 || input->repeat_pict == 3 ||
        ((input->flags & AV_FRAME_FLAG_INTERLACED) && input->repeat_pict > 1) ||
        input->pts == AV_NOPTS_VALUE || (f->inputs && input->pts <= f->last_pts))
        return AVERROR_INVALIDDATA;
    FieldClock clock = {.ordinal=f->inputs, .pts=input->pts,
        .duration=input->duration, .dts=input->pkt_dts,
        .keyframe=!!(input->flags & AV_FRAME_FLAG_KEY),
        .interlaced=!!(input->flags & AV_FRAME_FLAG_INTERLACED) || input->repeat_pict == 1};
    clock.fields = input->repeat_pict == 1 ? 3 : clock.interlaced ? 2 : 1;
    if (!f->inputs) {
        f->single = av_frame_clone(input);
        if (!f->single) return AVERROR(ENOMEM);
    } else av_frame_free(&f->single);
    /* Only bounded ordinals enter libavfilter's multiply/add/extrapolation.
       Color, field order, geometry and owned planes remain untouched. */
    input->pts = f->inputs;
    input->duration = 1;
    input->time_base = (AVRational){1, 1};
    /* H.264 reports pic_struct 5/6 as progressive even when the coded picture
       contains different fields. Its declared three-field sequence is explicit.
       We own repeats; leaving their hints in BWDIF can bypass neighboring
       interlaced pictures instead of deinterlacing them. */
    if (clock.interlaced) input->flags |= AV_FRAME_FLAG_INTERLACED;
    input->repeat_pict = 0;
    int result = av_buffersrc_add_frame_flags(f->source, input, 0);
    if (result < 0) return result;
    f->clocks[(f->head + f->count) % FIELD_QUEUE] = clock;
    f->count++;
    f->inputs++;
    f->last_pts = clock.pts;
    return 0;
}

int deadpan_fields_flush(DeadpanFields *f) {
    if (f->flushed) return AVERROR_INVALIDDATA;
    int result = av_buffersrc_add_frame_flags(f->source, NULL, 0);
    if (result >= 0) f->flushed = 1;
    return result;
}

/* BWDIF's single-input EOF repeats the same temporal neighbor and can weave
   the two moving fields. With no temporal neighbors, use a defined spatial
   bob: retain the current field's rows, average its nearest rows for missing
   lines, and replicate the nearest row at the boundary. No other field enters
   this interpolation. The admitted planar 8/16-bit formats are unchanged. */
static int single_field(const AVFrame *input, AVFrame *output, int phase) {
    const AVPixFmtDescriptor *desc = av_pix_fmt_desc_get(input->format);
    if (!desc || (desc->flags & (AV_PIX_FMT_FLAG_BE | AV_PIX_FMT_FLAG_PAL)) ||
        (!(desc->flags & AV_PIX_FMT_FLAG_PLANAR) && desc->nb_components != 1))
        return AVERROR_INVALIDDATA;
    int result = av_frame_make_writable(output);
    if (result < 0) return result;
    int parity = !(input->flags & AV_FRAME_FLAG_TOP_FIELD_FIRST) ^ phase;
    for (int component = 0; component < desc->nb_components; component++) {
        int plane = desc->comp[component].plane;
        int bytes = (desc->comp[component].depth + 7) / 8;
        if (bytes < 1 || bytes > 2 || desc->comp[component].step != bytes ||
            desc->comp[component].offset || desc->comp[component].shift)
            return AVERROR_INVALIDDATA;
        int chroma = !(desc->flags & AV_PIX_FMT_FLAG_RGB) && (component == 1 || component == 2);
        int width = AV_CEIL_RSHIFT(input->width, chroma ? desc->log2_chroma_w : 0);
        int height = AV_CEIL_RSHIFT(input->height, chroma ? desc->log2_chroma_h : 0);
        for (int y = 0; y < height; y++) {
            int lo = y, hi = y;
            if ((y & 1) != parity) {
                lo = y ? y - 1 : y + 1;
                hi = y + 1 < height ? y + 1 : y - 1;
            }
            const uint8_t *a = input->data[plane] + (ptrdiff_t)lo * input->linesize[plane];
            const uint8_t *b = input->data[plane] + (ptrdiff_t)hi * input->linesize[plane];
            uint8_t *dst = output->data[plane] + (ptrdiff_t)y * output->linesize[plane];
            for (int x = 0; x < width; x++) {
                if (bytes == 1) dst[x] = (a[x] + b[x] + 1) / 2;
                else AV_WL16(dst + x * 2, (AV_RL16(a + x * 2) + AV_RL16(b + x * 2) + 1) / 2);
            }
        }
    }
    return 0;
}

int deadpan_fields_pull(DeadpanFields *f, AVFrame *output) {
    av_frame_unref(output);
    int phase = f->phase;
    if (phase == 2 && !f->repeated) return AVERROR_INVALIDDATA;
    int result = phase == 2 ? av_frame_ref(output, f->repeated) :
        av_buffersink_get_frame(f->sink, output);
    if (result < 0) return result == AVERROR_EOF && f->count ? AVERROR_INVALIDDATA : result;
    if (!f->count || output->pts < 0 || (output->flags & AV_FRAME_FLAG_INTERLACED))
        return AVERROR_INVALIDDATA;
    FieldClock clock = f->clocks[f->head];
    /* With one input, upstream's EOF neighbor has the same ordinal, so its
       second output also reports zero. Output order and the retained field
       flag identify that field; its real time still requires measured duration. */
    int single_end = f->flushed && f->inputs == 1 && phase && output->pts == 0;
    if (phase != 2 && ((!single_end && output->pts != clock.ordinal * 2 + phase) ||
        output->duration != (clock.interlaced ? 1 : 2) ||
        (phase && !clock.interlaced))) return AVERROR_INVALIDDATA;
    if (phase != 2 && f->flushed && f->inputs == 1 && clock.interlaced) {
        result = single_field(f->single, output, phase);
        if (result < 0) return result;
    }
    if (!phase && clock.fields == 3) {
        if (f->repeated) return AVERROR_INVALIDDATA;
        f->repeated = av_frame_clone(output);
        if (!f->repeated) return AVERROR(ENOMEM);
    }
    int64_t interval;
    if (f->count > 1) {
        FieldClock next = f->clocks[(f->head + 1) % FIELD_QUEUE];
        if (__builtin_sub_overflow(next.pts, clock.pts, &interval)) return AVERROR(EOVERFLOW);
    } else {
        /* EOF's last field uses its own measured duration, never the filter's
           repeated penultimate interval. Unknown terminal duration is an error. */
        if (!f->flushed) return AVERROR_INVALIDDATA;
        interval = clock.duration;
        if (interval <= 0) return AVERROR(ENODATA);
    }
    if (interval <= 0) return AVERROR_INVALIDDATA;
    int64_t pts, duration, offset, end, dts = AV_NOPTS_VALUE;
    /* Sixth ticks represent both halves and thirds without rounding, even for
       odd coded-picture intervals. Container PTS/duration own the total span;
       repeat_pict determines subdivisions, never extra time beyond that span. */
    if (__builtin_mul_overflow(clock.pts, (int64_t)6, &pts) ||
        __builtin_mul_overflow(interval, (int64_t)(6 / clock.fields), &duration) ||
        __builtin_mul_overflow(duration, (int64_t)phase, &offset) ||
        __builtin_add_overflow(pts, offset, &pts) ||
        __builtin_add_overflow(pts, duration, &end) || pts == AV_NOPTS_VALUE ||
        (clock.dts != AV_NOPTS_VALUE &&
            (__builtin_mul_overflow(clock.dts, (int64_t)6, &dts) || dts == AV_NOPTS_VALUE)))
        return AVERROR(EOVERFLOW);
    int complete = phase + 1 == clock.fields;
    output->pts = pts;
    output->best_effort_timestamp = pts;
    output->duration = duration;
    output->pkt_dts = dts;
    output->repeat_pict = 0;
    output->flags &= ~AV_FRAME_FLAG_KEY;
    if (!phase && clock.keyframe) output->flags |= AV_FRAME_FLAG_KEY;
    f->phase = complete ? 0 : phase + 1;
    if (complete) {
        f->head = (f->head + 1) % FIELD_QUEUE; f->count--;
        av_frame_free(&f->repeated);
    }
    return 0;
}
