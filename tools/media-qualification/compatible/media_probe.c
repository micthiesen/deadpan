/* Reuse the original authored fixtures unchanged, with one explicit MP4 option.
 * FFmpeg 8's default 1000 Hz movie timescale loses 32 samples at our AAC offset.
 * 240000 is the LCM of the fixture's 30000 video ticks and 48000 audio samples.
 * This still uses edit lists and is not the spec's final export configuration.
 */
#define _POSIX_C_SOURCE 200809L
#include <string.h>
#include <libavutil/dict.h>

static int exact_movie_timescale = 1;

static int fixture_dict_set(AVDictionary **dictionary, const char *key,
                            const char *value, int flags) {
    int result = av_dict_set(dictionary, key, value, flags);
    if (result >= 0 && exact_movie_timescale && strcmp(key, "movflags") == 0)
        result = av_dict_set(dictionary, "movie_timescale", "240000", 0);
    return result;
}

#define av_dict_set fixture_dict_set
#define main original_main
#include "../media_probe.c"
#undef main
#undef av_dict_set

int main(int argc, char **argv) {
    if (argc > 1 && strcmp(argv[1], "encode-coarse-timescale") == 0) {
        exact_movie_timescale = 0;
        argv[1] = "encode";
    }
    return original_main(argc, argv);
}
