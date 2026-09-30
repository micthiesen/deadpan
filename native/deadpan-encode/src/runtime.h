#ifndef DEADPAN_ENCODE_RUNTIME_H
#define DEADPAN_ENCODE_RUNTIME_H

#include <stdint.h>

#define DP_RUNTIME_ABI 1
#define DP_RUNTIME_IMAGES 5
#define DP_RUNTIME_PATH 1024

typedef struct {
    uint32_t abi_version, kind;
    uint64_t device, inode, file_size;
    int64_t modification_seconds, change_seconds, birth_seconds;
    uint32_t modification_nanoseconds, change_nanoseconds, birth_nanoseconds;
    uint32_t generation;
    uint8_t uuid[16];
    uint32_t cpu_type, cpu_subtype, file_type;
    uint64_t header_address, anchor_address, header_file_offset;
    char path[DP_RUNTIME_PATH];
} dp_runtime_image;

typedef struct {
    uint32_t abi_version, cpu_family;
    char os_build[64], hardware_model[64];
} dp_runtime_platform;

typedef struct { char code[48], message[256]; } dp_runtime_error;

int dp_runtime_capture(uint32_t kind, dp_runtime_image *image, dp_runtime_error *error);
int dp_runtime_revalidate(const dp_runtime_image *image, int fd, dp_runtime_error *error);
int dp_runtime_observe_platform(dp_runtime_platform *platform, dp_runtime_error *error);

#endif
