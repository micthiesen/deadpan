#pragma once
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif
typedef struct DpAlignSession DpAlignSession;
typedef int (*DpAlignCancelled)(const void *context);
DpAlignSession *dp_align_create(const uint8_t *graph, size_t graph_len,
                               char *error, size_t error_len);
void dp_align_destroy(DpAlignSession *session);
int dp_align_run(DpAlignSession *session, const float *features, size_t frames,
                 float *output, size_t output_capacity, size_t *output_frames,
                 DpAlignCancelled cancelled, const void *context,
                 char *error, size_t error_len);
#ifdef __cplusplus
}
#endif
