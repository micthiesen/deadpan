# Scratch-only forest replay patch

Source file: crates/deadpan-app/src/preview/harness/splice/structural_capture.rs

Apply forest-replay.patch after the core Children capture/placement implementation is integrated. The patch extends the existing structural capture replay before fixture cleanup. It closes the service writer, saves a two-root all-empty Children capture in named register f, reopens a fresh session, chooses f through the production register command, and opens production :splice. It checks the restored selector/payload/session, painted forest labels and roots, exact equal-time slot, absent endpoints/media work, one placement commit, and one Undo with the source structure intact.

Named rendered checkpoints:

- Restored empty forest placement at 960 by 640
- Restored empty forest placement at 1280 by 820

The current checkout contains the requested label fix: preview/copied.rs says “Copied empty contents,” preview/splice/empty.rs says “Empty group contents,” and preview/splice/controls.rs says “Insert empty contents” for Children.

Verification: rustfmt completed on the scratch copy; git apply --check accepts forest-replay.patch against the unchanged source file. Cargo, apps, and the replay were not run, as requested. No repository file was edited by this task.

SHA-256:

- Before source: 0321adfea5412b20ac66b2b63df1eb767abd5028163d1faaac83fc6fe9512727
- After scratch source: 65bdbb43ac0ed87758101d9695c83a612cfaf13e2bfe03f8184d464d3cee17d3
- Unified patch: fbed15282802b9fae907e3a85d75dac616c602343e231fcc30dc787fb12047c5
