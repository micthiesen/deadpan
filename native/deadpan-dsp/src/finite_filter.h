#pragma once
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
#define DP_DSP_FILTER_NOEXCEPT noexcept
extern "C" {
#else
#define DP_DSP_FILTER_NOEXCEPT
#endif

// Copies 4097 finite coefficients with magnitude <=16. Offsets run from
// -2048 through +2048. The caller owns the array until this call returns.
// On any failure with a valid output pointer, *output is null.
int32_t dp_dsp_filter_create(const double *coefficients, size_t coefficient_count,
                            void **output) DP_DSP_FILTER_NOEXCEPT;

// 5120 frames per input channel: 2048 preceding + 1024 core + 2048 following.
// Produces the 1024 signed core samples per channel with delay compensated.
// Inputs must be finite with magnitude <=1e100; the input arrays may alias.
// Outputs must be writable and overlap neither each other nor either input.
// No pointer is retained.
// Failure leaves both output arrays unchanged. Processing failures poison the
// handle; invalid caller arguments do not. No allocation occurs during read.
int32_t dp_dsp_filter_read(void *filter, const double *left, const double *right,
                          uint32_t input_frames, double *output_left,
                          double *output_right,
                          uint32_t output_frames) DP_DSP_FILTER_NOEXCEPT;
void dp_dsp_filter_destroy(void *filter) DP_DSP_FILTER_NOEXCEPT;

#ifdef __cplusplus
}
#endif
#undef DP_DSP_FILTER_NOEXCEPT
