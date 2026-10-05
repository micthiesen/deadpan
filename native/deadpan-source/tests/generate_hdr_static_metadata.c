/* Development-only HDR fixture remuxer. Build against DEADPAN_FFMPEG_PREFIX:
 * cc -std=c11 -Wall -Wextra -Werror -I"$DEADPAN_FFMPEG_PREFIX/include" \
 *   native/deadpan-source/tests/generate_hdr_static_metadata.c \
 *   -L"$DEADPAN_FFMPEG_PREFIX/lib" -Wl,-rpath,"$DEADPAN_FFMPEG_PREFIX/lib" \
 *   -lavformat -lavcodec -lavutil -o /tmp/deadpan-hdr-remux
 * /tmp/deadpan-hdr-remux INPUT.mp4 OUTPUT.mp4 pq|hlg static|none
 *
 * Copies every packet unchanged through the pinned FFmpeg 8.0.3 MP4 muxer
 * (bitexact, faststart, write_colr) without opening a decoder. The video
 * stream's nclx tags are set explicitly (BT.2020 primaries, BT.2020 NCL,
 * limited range, left chroma, and the named PQ/HLG transfer), matching the
 * encoder's VUI, because the copied demux parameters are not probed. `static` adds stream
 * mastering display (SMPTE ST 2086, P3-D65 primaries, 1000/0.0001 cd/m2) and
 * content light (MaxCLL 1000, MaxFALL 400) coded side data, which movenc
 * writes as `mdcv`/`clli` boxes in the visual sample entry. The values equal
 * the in-band SEI written by tests/generate_hdr_fixtures.py.
 */
#include <stdio.h>
#include <string.h>
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/mastering_display_metadata.h>
#include <libavutil/mem.h>

int main(int argc, char **argv) {
    AVFormatContext *input = NULL, *output = NULL;
    AVPacket *packet = NULL;
    AVDictionary *options = NULL;
    int result = AVERROR(EINVAL), exit_code = 1;
    if (argc != 5 || (strcmp(argv[3], "pq") && strcmp(argv[3], "hlg")) ||
        (strcmp(argv[4], "static") && strcmp(argv[4], "none"))) {
        fprintf(stderr, "usage: %s INPUT.mp4 OUTPUT.mp4 pq|hlg static|none\n", argv[0]);
        return 1;
    }
    enum AVColorTransferCharacteristic transfer = strcmp(argv[3], "pq") ? AVCOL_TRC_ARIB_STD_B67 : AVCOL_TRC_SMPTE2084;
    int with_static = !strcmp(argv[4], "static");
    if ((result = avformat_open_input(&input, argv[1], NULL, NULL)) < 0) goto done;
    if ((result = avformat_alloc_output_context2(&output, NULL, "mp4", argv[2])) < 0) goto done;
    output->flags |= AVFMT_FLAG_BITEXACT;
    for (unsigned int i = 0; i < input->nb_streams; i++) {
        AVStream *in = input->streams[i];
        AVStream *stream = avformat_new_stream(output, NULL);
        if (!stream) { result = AVERROR(ENOMEM); goto done; }
        if ((result = avcodec_parameters_copy(stream->codecpar, in->codecpar)) < 0) goto done;
        stream->time_base = in->time_base;
        stream->codecpar->codec_tag = in->codecpar->codec_tag;
        if (in->codecpar->codec_type != AVMEDIA_TYPE_VIDEO) continue;
        stream->codecpar->color_primaries = AVCOL_PRI_BT2020;
        stream->codecpar->color_trc = transfer;
        stream->codecpar->color_space = AVCOL_SPC_BT2020_NCL;
        stream->codecpar->color_range = AVCOL_RANGE_MPEG;
        stream->codecpar->chroma_location = AVCHROMA_LOC_LEFT;
        stream->codecpar->field_order = AV_FIELD_PROGRESSIVE;
        if (!with_static) continue;
        size_t size;
        AVMasteringDisplayMetadata *mastering = av_mastering_display_metadata_alloc_size(&size);
        if (!mastering) { result = AVERROR(ENOMEM); goto done; }
        const int primaries[3][2] = {{34000, 16000}, {13250, 34500}, {7500, 3000}};
        for (int c = 0; c < 3; c++)
            for (int k = 0; k < 2; k++)
                mastering->display_primaries[c][k] = av_make_q(primaries[c][k], 50000);
        mastering->white_point[0] = av_make_q(15635, 50000);
        mastering->white_point[1] = av_make_q(16450, 50000);
        mastering->max_luminance = av_make_q(10000000, 10000);
        mastering->min_luminance = av_make_q(1, 10000);
        mastering->has_primaries = 1;
        mastering->has_luminance = 1;
        if (!av_packet_side_data_add(&stream->codecpar->coded_side_data, &stream->codecpar->nb_coded_side_data,
                AV_PKT_DATA_MASTERING_DISPLAY_METADATA, mastering, size, 0)) {
            av_free(mastering); result = AVERROR(ENOMEM); goto done;
        }
        AVContentLightMetadata *light = av_content_light_metadata_alloc(&size);
        if (!light) { result = AVERROR(ENOMEM); goto done; }
        light->MaxCLL = 1000;
        light->MaxFALL = 400;
        if (!av_packet_side_data_add(&stream->codecpar->coded_side_data, &stream->codecpar->nb_coded_side_data,
                AV_PKT_DATA_CONTENT_LIGHT_LEVEL, light, size, 0)) {
            av_free(light); result = AVERROR(ENOMEM); goto done;
        }
    }
    av_dict_set(&options, "movflags", "+write_colr+faststart", 0);
    if ((result = avio_open(&output->pb, argv[2], AVIO_FLAG_WRITE)) < 0 ||
        (result = avformat_write_header(output, &options)) < 0) goto done;
    packet = av_packet_alloc();
    if (!packet) { result = AVERROR(ENOMEM); goto done; }
    while ((result = av_read_frame(input, packet)) >= 0) {
        AVStream *in = input->streams[packet->stream_index];
        av_packet_rescale_ts(packet, in->time_base, output->streams[packet->stream_index]->time_base);
        packet->pos = -1;
        result = av_interleaved_write_frame(output, packet);
        av_packet_unref(packet);
        if (result < 0) goto done;
    }
    if (result != AVERROR_EOF || (result = av_write_trailer(output)) < 0) goto done;
    exit_code = 0;
done:
    if (exit_code) fprintf(stderr, "fixture remux failed: %s\n", av_err2str(result));
    av_dict_free(&options);
    av_packet_free(&packet);
    avformat_close_input(&input);
    if (output) avio_closep(&output->pb);
    avformat_free_context(output);
    return exit_code;
}
