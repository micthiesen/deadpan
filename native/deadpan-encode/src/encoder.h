#ifndef DEADPAN_ENCODE_H
#define DEADPAN_ENCODE_H

#include <stddef.h>
#include <stdint.h>

#define DP_ENCODE_ABI_VERSION 1U
#define DP_ENCODE_MAX_DIMENSION 8192U
#define DP_ENCODE_MAX_PIXELS 33554432ULL
#define DP_ENCODE_MAX_FRAMES 1000000ULL
#define DP_ENCODE_MAX_AUDIO_SAMPLES 4147200000ULL
#define DP_ENCODE_MAX_OUTPUT_BYTES 68719476736ULL
#define DP_ENCODE_MAX_PACKETS 2000000ULL
#define DP_ENCODE_MAX_PACKET_BYTES 33554432ULL
#define DP_ENCODE_MAX_TIMEOUT_MILLIS 86400000ULL
#define DP_ENCODE_AUDIO_BLOCK 1024U
/* Version of the additive HDR extension structs below. The base ABI structs
 * (config/info/report) and entry points keep DP_ENCODE_ABI_VERSION 1. */
#define DP_ENCODE_HDR_ABI_VERSION 1U
#define DP_ENCODE_TRANSFER_PQ 1U
#define DP_ENCODE_TRANSFER_HLG 2U

typedef struct dp_encode_session dp_encode_session;

typedef struct {
    uint32_t abi_version;
    uint32_t width;
    uint32_t height;
    uint32_t fps_num;
    uint32_t fps_den;
    uint64_t video_frames;
    uint64_t audio_samples;
    uint64_t video_bitrate;
    uint32_t gop_frames;
    uint32_t b_frames; /* Exactly 0 or 2. Requested, not measured bitstream evidence. */
    uint32_t mode; /* 0 requires hardware; 1 requires OS software. No fallback. */
    uint64_t maximum_output_bytes;
    uint64_t maximum_packets;
    uint64_t maximum_packet_bytes;
} dp_encode_config;

/* The callback and opaque pointer are borrowed only until this call returns.
 * Rust passes the remaining time from one deadline, never a fresh job budget. */
typedef struct {
    void *opaque;
    int (*cancelled)(void *opaque);
    uint64_t timeout_millis;
} dp_encode_control;

/* HEVC Main10 BT.2020 NCL limited-range output. Chromaticities use 1/50000
 * units in R,G,B order and luminances 1/10000 cd/m2 (SMPTE ST 2086). HLG
 * requires has_mastering == 0. Input pictures are tight planar 10-bit 4:2:0
 * little-endian u16 samples: Y w*h, Cb w/2*h/2, Cr w/2*h/2; limited range. */
typedef struct {
    uint32_t abi_version;
    uint32_t transfer;
    uint32_t has_mastering;
    uint16_t primaries[3][2];
    uint16_t white_point[2];
    uint32_t max_luminance;
    uint32_t min_luminance;
} dp_encode_hdr_config;

/* CTA-861.3 static content light levels in cd/m2. */
typedef struct {
    uint16_t max_cll;
    uint16_t max_fall;
} dp_encode_content_light;

/* Queried selected video encoder and stream declarations. Names come from
 * libavcodec/libavutil; codec_tag is the stream tag after the MP4 header. These
 * are requested/declared values, not emitted-bitstream evidence. */
typedef struct {
    uint32_t abi_version;
    int32_t profile;
    int32_t pix_fmt;
    int32_t color_primaries;
    int32_t color_trc;
    int32_t colorspace;
    int32_t color_range;
    int32_t chroma_location;
    uint32_t mastering_side_data;
    uint32_t content_light_side_data;
    /* HEVC NAL units of the unspecified types 62/63 removed before muxing.
     * VideoToolbox attaches Dolby Vision 8.4 RPUs (type 62) to HLG output;
     * the base-layer HLG stream decodes identically without them. */
    uint64_t removed_unspecified_nal_units;
    char encoder[32];
    char profile_name[32];
    char pix_fmt_name[32];
    char codec_tag[8];
} dp_encode_video_info;

typedef struct {
    char code[48];
    char message[256];
} dp_encode_error;

/* Queried codec context properties, not qualification of the emitted bitstream. */
typedef struct {
    uint32_t abi_version;
    uint32_t avcodec_version;
    uint32_t avformat_version;
    uint32_t avutil_version;
    uint32_t movie_timescale;
    uint32_t video_time_base_num;
    uint32_t video_time_base_den;
    uint32_t audio_time_base_num;
    uint32_t audio_time_base_den;
    uint32_t audio_frame_size;
    int32_t video_profile;
    int32_t video_has_b_frames;
    int32_t video_max_b_frames;
    int32_t video_gop_size;
    int32_t audio_profile;
    int32_t audio_initial_padding;
    int32_t audio_trailing_padding;
    uint32_t requested_mode;
    uint64_t video_bitrate;
    uint64_t audio_bitrate;
    uint64_t maximum_moov_bytes;
} dp_encode_info;

typedef struct {
    dp_encode_info info;
    uint64_t video_frames;
    uint64_t audio_samples;
    uint64_t video_packets;
    uint64_t audio_packets;
    uint64_t output_bytes;
    uint64_t packet_bytes;
    uint64_t video_duration_from_contract_packets;
    uint32_t faststart_read_opens;
    uint32_t faststart_read_closes;
    uint32_t video_eof;
    uint32_t audio_eof;
} dp_encode_report;

/* Return 1 on success, 0 on error. Every operation failure poisons the session.
 * The caller retains ownership of the fresh private read/write regular fd.
 * Calls must be serialized. Input must alternate in exact chronological order:
 * compare the next video PTS in 1/fps_num with the next audio sample in 1/48000;
 * video wins ties. No input reference survives a successful push call. */
int dp_encode_open(int fd, const dp_encode_config *config,
                   const dp_encode_control *control, dp_encode_session **session,
                   dp_encode_info *info, dp_encode_error *error);
int dp_encode_push_picture(dp_encode_session *session, uint64_t ordinal,
                           int64_t pts, int64_t duration, const uint8_t *bytes,
                           uint64_t length, const dp_encode_control *control,
                           dp_encode_error *error);
int dp_encode_push_audio(dp_encode_session *session, uint64_t first_sample,
                         const float *left, const float *right, uint32_t count,
                         const dp_encode_control *control, dp_encode_error *error);
int dp_encode_finish(dp_encode_session *session, const dp_encode_control *control,
                     dp_encode_report *report, dp_encode_error *error);
/* HDR variant of dp_encode_open; hdr must be non-null and valid. */
int dp_encode_open_hdr(int fd, const dp_encode_config *config,
                       const dp_encode_hdr_config *hdr,
                       const dp_encode_control *control, dp_encode_session **session,
                       dp_encode_info *info, dp_encode_error *error);
/* Finish with optional static content light. PQ sessions require it; SDR and
 * HLG sessions require NULL. dp_encode_finish equals passing NULL. PQ writes
 * mdcv (when configured) and clli as MP4 sample-entry boxes at the trailer. */
int dp_encode_finish_hdr(dp_encode_session *session, const dp_encode_content_light *light,
                         const dp_encode_control *control, dp_encode_report *report,
                         dp_encode_error *error);
/* Pure read of the live or finished session; returns 0 for null arguments. */
int dp_encode_query_video(const dp_encode_session *session, dp_encode_video_info *video);
/* Remove HEVC NAL types 62/63 from one Annex B packet in place; see encoder.c. */
int dp_encode_strip_unspecified_hevc_nal_units(uint8_t *data, size_t size, size_t *new_size,
                                               uint64_t *removed);
void dp_encode_close(dp_encode_session *session);

#endif
