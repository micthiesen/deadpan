# Retained extension input evidence, 2026-10-08

Extension context schema 2 binds the complete saved input descriptor and the
picture signatures behind the context rejection rule into one immutable
manifest. Retention repeats that rule from the captured bytes. This completes
the input-evidence connection; DP-12 remains Partial for output admission,
accepted extension media and application integration.

## What is retained and checked

The manifest includes exact relative model samples, every intervening support
span, its closed terminal, the explicitly unconditioned opposite seam, canvas,
duration, frame rate, operation and selected target record. It checks those
against the chronological PNG declarations and definition clocks. A selected
region is reconstructed from its saved target record, including the hash and
anchor geometry. Intrinsic target validation also runs when the anchor is
unavailable. Store recapture remains responsible for actual source/index truth
and request relevance.

The signature sidecar names every unique decoded picture and the physical
source count for each covered interval. Its versioned binary format preserves
the 32×18 cell means and 32-bin histogram used by `deadpan-context-shots-1`.
The host repeats the padded interval checks and the structural seam checks;
missing, duplicate or unrelated signatures, contradictory source counts,
source jumps and detected transitions reject the capture. All-authored-black
context has an explicit empty binary header.

The manifest stays within 1 MiB, the input descriptor within 512 KiB and PNGs
within their existing aggregate 16 MiB limit. Signatures use at most 950,284
bytes. The existing combined limit of 512 reader opens and unique decoded
pictures still applies. Signature retention shares the capture deadline and
cancellation. The sidecar cannot alias a PNG or the manifest. Identical PNG
declarations at distinct context positions remain valid.

Schema-2 conditioning receipts bind the manifest, every PNG and the signature
sidecar to private BLAKE3 snapshots. Worker-side modifications cannot change
those snapshots. The Python reader checks the matching descriptor grammar,
coverage, artifact identities and binary shape before inference. It sends only
the chronological model PNGs to the backend. The runtime build inventory now
includes its new reader module.

The continuity rule remains a conservative heuristic. It does not prove the
signature was measured from the claimed source, guarantee a single shot, or
establish the quality of generated output. Qualified project capture supplies
the source facts; later durable admission must compare the saved descriptor
with an independently captured request. Cross-provider seam checks still
cover abrupt change, with no new claim of full gradual-transition coverage
across different providers.

## Verification

`cargo xtask gate` passed on the reference M5 Max with Rust 1.97.1 and the pinned
FFmpeg 8.0.3 prefix: formatting, strict workspace and UI-harness Clippy, all
5,274 workspace tests, all 1,071 UI-harness tests and both doctests. The suites
reported 10 and 2 skipped tests respectively; these are not counted as passes.
The final worker suite passed all 111 Python tests. An isolated import from the
seven runtime distribution files verified that the new helper is included.

[Retained evidence](../../tools/model-qualification/evidence/2026-10-08-extension-evidence/)
includes complete compressed Rust logs, source hashes, machine/toolchain facts,
the isolated worker inventory and review results. The Python result came from
the worker agent's tool output; no separate raw Python log was saved.

The focused pass before the final target-record regression ran 97 tests,
including real CFR B-frame footage in both extension directions. That test
compares the captured descriptor and recomputed measurements, sends the exact
Rust manifest through the Python reader, retains all inputs, corrupts the
worker-side signature file, and verifies the old private snapshot still reads
the original bytes while a new capture fails. Pure tests cover fades, cuts,
seams, padded coverage, corrupted headers/counts/histograms, missing rows,
cancelled/deadline-expired work, metadata tampering and unavailable anchors.

Independent review found and corrected identical-PNG alias handling, omitted
nullable fields and malformed target records hidden by unavailable anchors.
Strict tagged empty variants also reject unknown fields. Document target
validation retains its existing asset/span checks and error ordering.

The initial compile/lint passes found hash formatting, test module/borrow wiring
and an example still expecting three retained objects. Those were fixed; no runtime
assertion or input bound was weakened. The prior development schema 1 is
intentionally refused. Bridge context schemas and accepted Bridge media are
unchanged.

## Remaining work

Implement extension output motion, lighting, endpoint, face/mouth and selected
region admission; persist and reopen accepted extension artifacts; connect the
native controls, audition and explicit acceptance. Qualify a longer duration
envelope before advertising it. V3 completion/Ready/acceptance and approved
provider extension capabilities remain closed. No new model inference,
packaged application, export or visible UI claim is made by this milestone.
