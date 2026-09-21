/* Development-only fixture remuxer. Build against DEADPAN_FFMPEG_PREFIX:
 * cc -std=c11 -Wall -Wextra -Werror -I"$DEADPAN_FFMPEG_PREFIX/include" \
 *   native/deadpan-source/tests/generate_stream_hdr_fixture.c \
 *   -L"$DEADPAN_FFMPEG_PREFIX/lib" -Wl,-rpath,"$DEADPAN_FFMPEG_PREFIX/lib" \
 *   -lavformat -lavcodec -lavutil -o /tmp/deadpan-stream-hdr-fixture
 * /tmp/deadpan-stream-hdr-fixture \
 *   native/deadpan-source/tests/fixtures/limited709.mkv \
 *   native/deadpan-source/tests/fixtures/sdr-with-stream-hdr.mkv
 * The existing 4x2, one-frame FFV1 packet and SDR tags are retained unchanged.
 * Only Matroska stream content-light metadata is added. No decoder is opened.
 */
#include <stdio.h>
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/mastering_display_metadata.h>
#include <libavutil/mem.h>

int main(int argc, char **argv) {
    AVFormatContext *input = NULL, *output = NULL;
    AVPacket *packet = NULL;
    AVContentLightMetadata *light = NULL;
    int result = AVERROR(EINVAL), exit_code = 1;
    if (argc != 3) {
        fprintf(stderr, "usage: %s INPUT.mkv OUTPUT.mkv\n", argv[0]);
        return 1;
    }
    if ((result = avformat_open_input(&input, argv[1], NULL, NULL)) < 0) goto done;
    if (input->nb_streams != 1 || input->streams[0]->codecpar->codec_id != AV_CODEC_ID_FFV1 ||
        input->streams[0]->codecpar->color_trc != AVCOL_TRC_BT709) {
        result = AVERROR_INVALIDDATA;
        goto done;
    }
    if ((result = avformat_alloc_output_context2(&output, NULL, "matroska", argv[2])) < 0) goto done;
    output->flags |= AVFMT_FLAG_BITEXACT;
    AVStream *stream = avformat_new_stream(output, NULL);
    if (!stream) { result = AVERROR(ENOMEM); goto done; }
    if ((result = avcodec_parameters_copy(stream->codecpar, input->streams[0]->codecpar)) < 0) goto done;
    stream->time_base = input->streams[0]->time_base;
    size_t light_size;
    light = av_content_light_metadata_alloc(&light_size);
    if (!light) { result = AVERROR(ENOMEM); goto done; }
    light->MaxCLL = 1000;
    light->MaxFALL = 400;
    if (!av_packet_side_data_add(&stream->codecpar->coded_side_data,
        &stream->codecpar->nb_coded_side_data, AV_PKT_DATA_CONTENT_LIGHT_LEVEL,
        light, light_size, 0)) { result = AVERROR(ENOMEM); goto done; }
    light = NULL;
    if ((result = avio_open(&output->pb, argv[2], AVIO_FLAG_WRITE)) < 0 ||
        (result = avformat_write_header(output, NULL)) < 0) goto done;
    packet = av_packet_alloc();
    if (!packet) { result = AVERROR(ENOMEM); goto done; }
    while ((result = av_read_frame(input, packet)) >= 0) {
        av_packet_rescale_ts(packet, input->streams[0]->time_base, stream->time_base);
        packet->pos = -1;
        result = av_interleaved_write_frame(output, packet);
        av_packet_unref(packet);
        if (result < 0) goto done;
    }
    if (result != AVERROR_EOF || (result = av_write_trailer(output)) < 0) goto done;
    exit_code = 0;
done:
    if (exit_code) fprintf(stderr, "fixture remux failed: %s\n", av_err2str(result));
    av_free(light);
    av_packet_free(&packet);
    avformat_close_input(&input);
    if (output) avio_closep(&output->pb);
    avformat_free_context(output);
    return exit_code;
}
