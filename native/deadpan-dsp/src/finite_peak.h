#pragma once
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
#define DP_DSP_PEAK_NOEXCEPT noexcept
extern "C" {
#else
#define DP_DSP_PEAK_NOEXCEPT
#endif

// Copies exactly coefficient_count == 15 * 129 finite bounded doubles before
// returning. On failure, *output is null.
int32_t dp_dsp_peak_create(const double *coefficients, size_t coefficient_count,
                           void **output) DP_DSP_PEAK_NOEXCEPT;

// Processes exactly 1152 stereo f32 input frames and writes 1024 linked peak
// magnitudes. The caller owns both input arrays for the duration of the call.
// On failure, output is unchanged. The bank becomes unusable after a processing
// failure that occurs after validation.
int32_t dp_dsp_peak_read(void *bank, const float *left, const float *right,
                         uint32_t input_frames, double *output,
                         uint32_t output_frames) DP_DSP_PEAK_NOEXCEPT;
void dp_dsp_peak_destroy(void *bank) DP_DSP_PEAK_NOEXCEPT;

#ifdef __cplusplus
}
#endif
#undef DP_DSP_PEAK_NOEXCEPT
