# Deterministic keymap-error bootstrap proposal

Scratch-only candidate; no repository edit, Cargo or replay/runtime invocation.
Stop unchanged-binary retries. Both original and retained retry failures remain.

Proposed ownership is one file only:
crates/deadpan-app/src/preview/harness.rs, the common Original bootstrap branch.
Do not apply while Root's full replay/source freeze remains active.
The exact full current source is harness.before.rs.txt. The full formatted
candidate is harness.candidate.rs.txt; keymap-setup.patch is the one-hunk patch.

Before source SHA256:
9896bbca1033a9b265f66fa0cd2d8d72ba05a290201a9efddd5e11d7d0ca1ce4.
Candidate SHA256:
889947030d59d79da34f25e6eed3e2ce467d553198d8306b311e8b9ddc7fb48b.

Only keymap-error bootstrap changes. Before choosing the Original, a new check
requires rejected-keymap status (does.not.exist, whole shipped fallback and
private Documents/keymap.json provenance) and a fully visible painted persistent
Keymap error header notice. It then sends real egui Cmd+N through the existing
Driver::key_modified/production router, the same startup path full-session uses.
The scripted OS picker already supplies the actual fixture; no app/document
state, dialog result or observer is forged or bypassed by this patch.

All other scenarios retain the existing Choose video pointer setup.
Global Driver::click/press/release behavior is unchanged.
keymap::error itself is completely unchanged: its later persistent-header pointer
click, navigation, malformed-map whole rejection, shipped h/Hold/Undo and visible
Help diagnostics still execute. This is a scenario setup repair, not a general
production pointer/layout fix or a claim that the measured movement disappeared.

No initialization/settling/assertion budget changes. Original initialization
still uses the existing15second wait and exact Ready/displayed/nonloading gate;
all later captures, metrics and assertions remain. Cmd+N replaces two setup
pointer frames with one production key event frame; elapsed times remain actual,
not normalized to the earlier failed runs. The new check fails explicitly if
the persistent diagnostic is absent rather than starting a misleading scenario.

Static scope review confirms exactly one diff hunk (21 added lines replacing
one setup click; net20 lines), with no unrelated formatting. rustfmt --edition2024
--config skip_children=true on the scratch candidate exits0. Existing source
still byte-equals the before snapshot. This is syntax formatting only; no
compile or runtime pass is claimed. Root should re-read current source before
applying; if its hash differs, rebuild the patch from current text.

After the full replay run terminates and Root permits this narrow source change,
appropriate verification is the existing App compile plus one fresh retained
keymap-error replay. Keep both first failures and distinguish repaired-source
result/binary/source hashes. Do not repeat the unchanged binary, enlarge timeout,
add sleep, alter admission, or change detector code.

Repeated failure schema pin:
keymap-error-retry-corrected/report.json SHA256
0ac009263f4b8d16d040ae6cc8452cd99067c1cec6e5cfc239b71efe85d6ba3b.
Its fourth check is project retention; initialization still never begins.
The package82ea and media739c drivers remain unchanged.
