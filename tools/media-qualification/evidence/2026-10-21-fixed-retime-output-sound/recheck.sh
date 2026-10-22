#!/bin/sh
set -eu
scratch=/tmp/deadpan-fixed-retime-owned-sound-2026-10-21
sh "$scratch/lint.sh" > "$scratch/lint.log" 2>&1
sh "$scratch/focused.sh"
