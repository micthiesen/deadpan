# Independent AVFoundation audio evidence

See [qualification](../../../../docs/qualification/native-audio-2026-09-28.md).
Both readers complete. The default-edit-list file aligns; the disabled file
loses its opening impulse and shifts subsequent events 1,088 samples early.
The nonzero exit is a retained timing failure, not a passing export gate.

Initial output boxed CoreMedia Boolean values as numeric JSON. The strict
oracle rejected that schema; the original source, logs and raw PCM remain here.
The corrected and sanitizer observations explicitly use JSON booleans.

Full command logs, raw observations, synthetic input MP4s and untouched output
PCM are compressed without truncation. No user media, executable, library or
project is published. The input files are byte-identical to the preceding
encoder experiment. No event alignment, manual trim or extra encode occurred.
Source inventories record concurrent checkout state; the native reports check
their own complete local source dependencies, which stayed unchanged per run.
The Rust renderer changes in the checkout were outside these Python/native tests.

manifest.json hashes every retained file except itself. archive-contents.json
records every archive member. Run audit-native.py to verify both levels without
extracting files. Scripts retain their task-specific paths for attribution.
The raw reports distinguish completed reading, timing failure and unqualified
interpretations. These results do not qualify physical playback, native video,
boundary-content quality, other macOS versions or product export.
