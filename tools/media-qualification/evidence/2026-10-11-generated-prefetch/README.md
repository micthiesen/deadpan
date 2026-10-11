# Generated entry preparation evidence

See the [qualification](../../../../docs/qualification/generated-prefetch-2026-10-11.md).
`manifest.json` retains exact sources, executable identity, fixture/library
pins, review notes, commands, complete logs and the fixed Clippy failure.

`generated-playback` runs four fixture-dependent release tests explicitly;
`app-worker` runs 44 ordinary worker tests; `media` runs 60 media unit tests.
`resources` repeats the four fixture tests from a copied release executable
under `time -l`. It does not activate audio output or a native window.

Use the fixture-export commands in the
[preceding evidence](../2026-10-11-generated-playback-cache/README.md) to create
an accepted package, then run the same app test filter. The retained package
is `/tmp/deadpan-generated-lookahead-20261011/conditioned.deadpan`; the copied
executable is `/tmp/deadpan-generated-prefetch-20261011/release-tests`.

`python3 check.py` verifies current source and retained evidence.
`python3 check.py --committed` verifies source against HEAD at this milestone.
The small-fixture timing observations do not establish full-resolution device
playback through the app's reader.
