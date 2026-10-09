#ifndef DEADPAN_SOURCE_DEINTERLACE_H
#define DEADPAN_SOURCE_DEINTERLACE_H
#include <libavutil/frame.h>
typedef struct DeadpanFields DeadpanFields;
/* Bounded buffer -> BWDIF -> sink, with no automatic format conversion.
   Original timestamps are retained separately. Internal ordinal timestamps avoid
   BWDIF's unchecked timestamp arithmetic and extrapolated terminal interval. */
int deadpan_fields_open(DeadpanFields **out, const AVFrame *first, unsigned threads);
/* Consumes the input frame's references on success. Pull owns its output. */
int deadpan_fields_push(DeadpanFields *fields, AVFrame *input);
int deadpan_fields_flush(DeadpanFields *fields);
int deadpan_fields_pull(DeadpanFields *fields, AVFrame *output);
void deadpan_fields_close(DeadpanFields **fields);
#endif
