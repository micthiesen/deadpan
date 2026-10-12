#pragma once
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
#define DP_DSP_CAUSAL_NOEXCEPT noexcept
extern "C" {
#else
#define DP_DSP_CAUSAL_NOEXCEPT
#endif

// Copy independent left/right kernels of 1..384001 coefficients, each finite
// with magnitude <=16. h[j] weights input at lag j. Inputs may alias. A valid
// output pointer receives null on failure. No array pointer is retained.
int32_t dp_dsp_causal_create(const double *left, const double *right,
    size_t coefficient_count, void **output) DP_DSP_CAUSAL_NOEXCEPT;

// Exactly coefficient_count-1 history samples then 4096 core samples per
// channel. Output preserves each core sample's label, with no look-ahead.
// Caller supplies all history; no state carries between independent reads.
// Inputs may alias and must be finite with magnitude <=1e100. Writable outputs
// must overlap neither inputs nor each other. Failure leaves both untouched.
// Processing allocates nothing; exceptions poison the handle, bad inputs do not.
int32_t dp_dsp_causal_read(void *filter, const double *left, const double *right,
    uint32_t input_frames, double *output_left, double *output_right,
    uint32_t output_frames) DP_DSP_CAUSAL_NOEXCEPT;
void dp_dsp_causal_destroy(void *filter) DP_DSP_CAUSAL_NOEXCEPT;

#ifdef __cplusplus
}
#endif
#undef DP_DSP_CAUSAL_NOEXCEPT
