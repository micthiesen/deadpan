# Default binding wire audit

## Scope and evidence

Inspected only the closed package `instrumented-1.deadpan` with SQLite URI `mode=ro&immutable=1`. The smallest history id whose request command is `insert_time` is id 3, revision `perf-rev-62759-4`, description `Insert pause`; it requests a 12-frame Hold at frame 17,933. Its stored edit is 4,515,926 UTF-8 bytes after compact JSON reserialization, matching the raw string length. The edit's `forward.audio_bindings` patch is 4,234,473 bytes: one timing change and 10,001 binding ValueChanges. No whole edit was retained or printed.

All package files were SHA-256 hashed immediately before and after read-only inspection; hashes matched:

- `.writer.lock`: e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
- `project.sqlite`: cf27bcc56e8dd67438530ec5775f98d31c12bfa1053e9a9c43c3c9ccf85bcbb5
- `project.sqlite-shm`: fd4c9fda9cd3f9ae7c962b0ddf37232294d55580e1aa165aa06129b8549389eb
- `project.sqlite-wal`: e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
- `manifest.json`: 8fce85b103ad4d3dafd2869369b57c72a89855e890518209ea1b9412aabcb9c7

## Exact values and omission-size arithmetic

At `forward.audio_bindings.bindings.<owner>.after.lattice`, all 10,001 entries have empty `arguments`, empty `births`, and `reference.root = {"type":"project_root_round_even"}`. At `.after.resume`, 1,495 entries are null; 8,506 are objects. Of those objects, all 8,506 have zero `phase.constant`, 8,505 have zero `local_boundary`, and one has a nonzero local boundary. These observations do not authorize dropping or normalizing any nondefault value.

A bounded stored-value example (one binding) is:

```json
{"before":null,"after":{"lattice":{"reference":{"timing":{"allocation":"perf-rev-62759-4","ordinal":0},"root":{"type":"project_root_round_even"},"physical":"hold-000000"},"arguments":[],"births":[]},"resume":null}}
```

Computed by compact-serializing each containing JSON object, then measuring the byte-length difference after omitting only the specified default-valued key:

| Candidate default omission | Count | Bytes in these stored values |
|---|---:|---:|
| `arguments: []` | 10,001 | 150,015 |
| `births: []` | 10,001 | 120,012 |
| `reference.root: ProjectRootRoundEven` | 10,001 | 420,042 |
| `resume: null` | 1,495 | 20,930 |
| `resume.local_boundary: 0` | 8,505 | 450,765 |
| `resume.phase.constant: 0` | 8,506 | 399,782 |
| **Combined candidate reduction** | | **1,561,546** |

This is 36.9% of the observed `forward.audio_bindings` JSON and 34.6% of the complete stored edit JSON. It is a format-size calculation only. It makes no claim about edit, history, SQLite, or pause latency.

## Source codecs and consumers

- `crates/deadpan-core/src/audio_binding.rs:35-42` defines `AudioClockRoot`; `AudioReferenceClock.root` is required at lines 58-65. Omitting the tagged default variant needs a deserialize default function that returns `ProjectRootRoundEven` and a serializer predicate for that exact variant. Its semantic use is in `audio_binding_lifecycle.rs:103,615` and `audio_binding.rs:529` (`clock_scope` conversion); nondefault clock roots encode provider and gap context and must stay explicit.
- `AudioPlacementTemplate.arguments` and `births` are required fields with bounded custom deserializers at `audio_binding.rs:103-122` (`path_vec`). Omitting empty vectors needs a default empty vector without bypassing `path_vec`'s bounded/duplicate-aware decoder for present values. Nonempty lexical repeat and birth paths remain data.
- `AudioLocalPhase.constant` and `AudioResume.local_boundary` are required at `audio_binding.rs:139-160`; `AudioLocalPhase::default` already gives zero constant and empty terms at lines 147-153. To omit zero, decoding must restore exactly `ExactRatio::ZERO`; the nonzero local boundary and nonempty phase terms remain explicit.
- `OwnedAudioBinding.resume` is `Option<AudioResume>` at `audio_binding.rs:221-231`. The stored `null` entries can be omitted and ordinarily decode to `None` as an absent Option field; verify the chosen serde shape with round-trip tests. `reanchors` already uses `default` plus empty-vector omission.
- `AudioBindingPatch` serializes its `bindings` map as the typed `ValueChange<OwnedAudioBinding>` data at `audio_binding_patch.rs:32-43`; its custom strict decoder validates each present before/after binding at lines 315-345. `DocumentPatch.audio_bindings` is the containing optional field at `command.rs:761-765`; apply and inverse are at `command.rs:909-910` and `1042-1045`.
- The stored history edit is replay input. Store planning serializes and immediately decodes the edit, then verifies its inverse restores the prior document at `crates/deadpan-store/src/lib.rs:1415-1427`. Store history validation recalculates the edit and decodes the saved edit at `crates/deadpan-store/src/validation.rs:612-631`. Omission therefore needs exact equality through serialize/decode, apply, inverse and history replay, not just a smaller JSON string.

`legacy_audio_binding_v20.rs` and `legacy_audio_binding_v21.rs` are explicit historical decoders; do not silently alter their accepted historical grammar as part of this current-wire change. Current `DOCUMENT_SCHEMA_VERSION` is 57 (`document.rs:13`) and SQLite package `schema::VERSION` is 86 (`deadpan-store/src/schema.rs:6`). Their gates are separate: schema-57 authored-document validation and schema-86 package opening do not make the unversioned nested patch JSON self-describing. If older package compatibility mattered, an old decoder that requires these fields would reject omitted patch values; explicit versioned decoding/migration would be needed. This session authorizes refusing old unused development packages, so avoid adding a migration solely for this format change. Keep the versions and refusal tests aligned with the current format change.

## Retained machine-readable summary

See sibling `default-binding-wire-audit.json` for counts, arithmetic, query identity, and before/after package hashes.
