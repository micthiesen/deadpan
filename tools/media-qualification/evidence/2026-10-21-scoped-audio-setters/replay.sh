#!/bin/sh
set -eu
scratch=/tmp/deadpan-scoped-audio-setters-2026-10-21
python "$scratch/identity.py" snapshot > "$scratch/identity.log"
"$scratch/bin/deadpan-app" --ui-check --scenario scoped-plays --mode visual --output "$scratch/visual" --kestrel-source /Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift > "$scratch/visual.log" 2>&1
ps -axo pid,ppid,etime,pcpu,comm | rg 'cargo|rustc|deadpan' > "$scratch/pre-performance-processes.txt" || test "$?" -eq 1
if test -s "$scratch/pre-performance-processes.txt"; then
    exit 75
fi
pmset -g batt > "$scratch/power.txt"
shasum -a 256 /Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift > "$scratch/kestrel-source.txt"
"$scratch/bin/deadpan-app" --ui-check --scenario scoped-plays --mode performance --output "$scratch/performance" --kestrel-source /Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift > "$scratch/performance.log" 2>&1
