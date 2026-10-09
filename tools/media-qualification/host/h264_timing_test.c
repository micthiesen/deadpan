/* Exercise the production bounded reader, including malformed HRD prefixes. */
#include "decoder.c"
#include <assert.h>

static int parse(const uint8_t *bytes, size_t length, int delays, int *structure) {
    DeadpanSource source = {0};
    DeadpanSourceError error;
    source.limits.max_packet_bytes = 1024;
    source.h264_timing.delay_bits = delays;
    assert(begin(&source, 1000, NULL, NULL, &error) == 1);
    return picture_timing(&source, bytes, length, structure);
}

int main(void) {
    /* 28 HRD bits + pic_struct 5 occupy all four payload bytes. The three
       clock_timestamp_flag bits do not exist; padding is not their value. */
    const uint8_t truncated[] = {6, 1, 4, 0, 0, 3, 0, 5, 0x80};
    int structure = -1;
    assert(parse(truncated, sizeof(truncated), 28, &structure) < 0);
    assert(structure == -1);

    const uint8_t valid[] = {6, 1, 5, 0, 0, 3, 0, 5, 0x10, 0x80};
    assert(parse(valid, sizeof(valid), 28, &structure) == 0 && structure == 5);
    structure = -1;
    const uint8_t short_delay[] = {6, 1, 1, 0x51, 0x80};
    assert(parse(short_delay, sizeof(short_delay), 64, &structure) < 0);
    const uint8_t duplicate[] = {6, 1, 1, 0x51, 1, 1, 0x51, 0x80};
    assert(parse(duplicate, sizeof(duplicate), 0, &structure) < 0);
    structure = -1;
    const uint8_t bad_escape[] = {6, 1, 4, 0, 0, 3, 4, 0x80};
    assert(parse(bad_escape, sizeof(bad_escape), 0, &structure) < 0);
    const uint8_t huge[] = {6, 1, 255, 255, 255, 255, 255, 0x80};
    assert(parse(huge, sizeof(huge), 0, &structure) < 0);
    puts("H264 HRD prefixes, clock flags, duplicate timing, emulation prevention and size limits: passed");
    return 0;
}
