# DB22 baseline binary compatibility qualification

Passed on 2026-09-24. No shared repository source or target files were changed.

The old executable was rebuilt now from commit 7277f29fe1ae96df5900d8871435f9961f697869, not retrieved as an archived shipped binary. All 2,127 tracked files were hash-verified against that commit's git archive. Only a scratch fixture-producer example was added. The qualified build used Rust 1.97.1, locked offline dependencies, the existing selected FFmpeg prefix, and a separate Cargo target. An initial accidental Homebrew Rust 1.98 build was discarded and is disclosed in provenance.json.

The producer used the baseline core/store APIs without schema-tag rewriting or direct SQL mutation. It created a DB22/core16 six-frame SilentHold with a nonzero resume at local 1, phase constant 1/7, and a nonzero placement phase term from 0 to 1. It then committed Split and Rename and performed undo/redo/undo, retaining six revision documents, two history transactions and pending redo.

The baseline CLI opened, validated and dumped the project. The current CLI migrated a closed copy to DB23/core17 and opened, validated and dumped it. Every revision document, request, edit, state row, redo row and table definition matched except the intended schema tags. The retained migration backup has DB version 22 and its SQL dump exactly matches the original. The original baseline package remains unchanged.

Actual time-mapped and edge-faded CLI PCM reads match exactly for windows [0,64), [1580,1644), [3180,3244), and [9536,9600). This SilentHold fixture verifies compatibility and binding admission, not nonzero DSP behavior.

Build command (cwd baseline):

```sh
PATH=/Users/michael/.cargo/bin:$PATH RUSTUP_TOOLCHAIN=1.97.1 DEADPAN_FFMPEG_PREFIX=/tmp/deadpan-media-compatible-xyhilms4/prefix CARGO_TARGET_DIR=/tmp/deadpan-db22-binary-20260924/target cargo build -p deadpan-cli -p deadpan-store --bin deadpan-cli --example create_v22_fixture --locked --offline
```

Qualification commands:

```sh
python3 /tmp/deadpan-db22-binary-20260924/qualify.py baseline
python3 /tmp/deadpan-db22-binary-20260924/qualify.py current /Users/michael/Code/deadpan/target/debug/deadpan-cli
python3 /tmp/deadpan-db22-binary-20260924/finish_evidence.py
```

See commands.jsonl for each exact CLI invocation, resolved executable hash, UTC timestamp and exit code. See provenance.json for compiler versions, source archive and binary/database hashes. The two project packages, producer source, SQL/history ledgers, scripts and captured CLI outputs are included. Rebuilt binaries, source tree and Cargo cache remain in the scratch directory but are excluded from this compact evidence archive.
