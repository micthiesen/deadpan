#pragma once
#include <stdint.h>

#ifdef __cplusplus
#define DP_DSP_NOEXCEPT noexcept
extern "C" {
#else
#define DP_DSP_NOEXCEPT
#endif

// The caller retains both immutable input arrays until destroy returns. Each
// array has exactly input_frames readable floats. No pointer is shared with a
// second thread. On failure create leaves *output null.
int32_t dp_dsp_create(const float *left, const float *right, uint32_t input_frames,
                      uint32_t output_frames, int32_t pitch, void **output) DP_DSP_NOEXCEPT;

// Explicit-rate mode: the exact positive numerator/denominator in [1/8,8]
// controls consumption independently of input/output buffer lengths. Input is
// limited to 1048576 frames and output to 8388608 frames. The same lifetime and
// failure contract applies. Unreduced positive ratios are accepted equivalently.
int32_t dp_dsp_create_exact_rate(const float *left, const float *right,
                      uint32_t input_frames, uint32_t output_frames,
                      uint64_t rate_numerator, uint64_t rate_denominator,
                      int32_t pitch, void **output) DP_DSP_NOEXCEPT;

// Each output array has requested writable floats, with requested <= 256.
// Success initializes only the first *written samples; the suffix is untouched.
// Failure leaves output arrays unchanged and poisons a renderer if DSP ran.
int32_t dp_dsp_read(void *engine, float *left, float *right, uint32_t requested,
                    uint32_t *written) DP_DSP_NOEXCEPT;
void dp_dsp_destroy(void *engine) DP_DSP_NOEXCEPT;

#ifdef __cplusplus
}
#endif
#undef DP_DSP_NOEXCEPT
