#ifndef DEADPAN_MEDIA_CONVERTER_H
#define DEADPAN_MEDIA_CONVERTER_H

#include <stddef.h>
#include <stdint.h>

typedef struct {
    uint32_t width;
    uint32_t height;
    uint32_t frames;
    uint32_t rate_num;
    uint32_t rate_den;
    uint64_t input_byte_length;
    uint64_t max_input_bytes;
    uint64_t max_output_bytes;
    uint64_t max_scratch_bytes;
    uint64_t timeout_ms;
} DeadpanConversionRequest;

typedef struct {
    uint64_t output_bytes;
    char input_rgb_sha256[65];
    char output_rgb_sha256[65];
    uint32_t input_time_base_num;
    uint32_t input_time_base_den;
    uint32_t output_time_base_num;
    uint32_t output_time_base_den;
    int64_t first_output_pts;
    int64_t last_output_pts;
    uint32_t ffv1_version;
    uint8_t slice_crc;
    uint32_t discarded_audio_streams;
} DeadpanConversionReport;

typedef struct {
    char code[48];
    char message[256];
} DeadpanConversionError;

int deadpan_convert(int input_fd, int output_fd, int scratch_fd,
                    const DeadpanConversionRequest *request,
                    DeadpanConversionReport *report,
                    DeadpanConversionError *error);

#endif
