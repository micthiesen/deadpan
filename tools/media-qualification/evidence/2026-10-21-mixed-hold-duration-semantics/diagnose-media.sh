#!/bin/sh
set -eu
mkdir -p /tmp/deadpan-mixed-hold-duration-next-2026-10-21/media-tree
export TMPDIR=/tmp/deadpan-mixed-hold-duration-next-2026-10-21/media-tree
export DEADPAN_PREVIEW_EXPORT_KEEP=1
export DEADPAN_REQUIRE_PHYSICAL_MAC=1
/tmp/deadpan-mixed-hold-duration-next-2026-10-21/first-runtime-bin/preview_export-09f24f1122af7985 --exact mixed_hold_duration_semantics_preview_and_encoded_output_keep_selected_source_clocks --nocapture > /tmp/deadpan-mixed-hold-duration-next-2026-10-21/media-clone-diagnostic.log 2>&1
