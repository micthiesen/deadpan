#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
scratch=/tmp/deadpan-owned-gap-hold-duration-next-2026-10-21
python "$scratch/identity.py" > "$scratch/identity.log"
"$scratch/bin/deadpan-app" --ui-check --scenario owned-gap-duration --mode visual --output "$scratch/visual" --kestrel-source /Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift > "$scratch/visual.log" 2>&1
ps -axo pid,ppid,etime,pcpu,comm | rg 'cargo|rustc|deadpan' > "$scratch/pre-performance-processes.txt" || test "$?" -eq 1
if test -s "$scratch/pre-performance-processes.txt"; then
    exit 75
fi
pmset -g batt > "$scratch/power.txt"
shasum -a 256 /Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift > "$scratch/kestrel-source.txt"
"$scratch/bin/deadpan-app" --ui-check --scenario owned-gap-duration --mode performance --output "$scratch/performance" --kestrel-source /Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift > "$scratch/performance.log" 2>&1
python "$scratch/summarize_replay.py" > "$scratch/replay-summary.log"
