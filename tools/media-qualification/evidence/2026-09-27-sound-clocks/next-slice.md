# Next authored-sound integration slice

Core 28/database 34 remain unchanged. The current increment supplies a sampled
route view, current Hold issuers and playback LRU residency. None installs a sound
event, persists a sample lattice, grants an allowance or adds a final bus voice.

Before adding a persisted schema, implement a complete independent voice operand
and read path through canonical preparation. Existing Original readers concatenate
one partitioned voice; adding another span kind alone does not mix overlapping
voices. Do not split Original Preserve history into separate beat engines. The
actual sum belongs at StageAudio::prepare_bus before LimitedAudio's one limiter.
With no sound events, keep the existing Original path byte-for-byte unchanged.

Retain source dependencies and complete per-voice processing contexts. A borrowed
AudioSignalMix supplies explicit aggregate processing, not the default independent
voice time/pitch behavior. Do not loosen AudioStageProjection's descendant/owner
scope checks to admit arbitrary assets. Add a real qualified voice input instead.

SoundRoute contains exact windows and ripple maps. AudioSoundRoute requires a
captured grid for every chronological node, retaining grid spacing and boundary
rule. Window and Keep also retain the old selection's physical audible mask.
Extra samples introduced by changed rounding are silent; filter/DSP context stays
complete. Retained route maps alone are not a persisted source or PCM recipe.

AudioHoldPolicyQuery reports current structural issuer identities only. Resolve
allowances for the particular sound and particular Hold/occurrence, before mixing.
Retained historical masks require their own qualified context and cannot be
reconstructed from current policy. Original scalar policy remains unchanged.

Persistence design already mapped: one logical event with complete recipe plus
stable physical fragments, record-array wires for composite identities, explicit
event-vs-fragment editing, and independent copying/isolation that forks recipes
while retaining within-copy sharing. A fragment key cannot be (event, owner),
because Ungroup can leave two projections on one owner. Split preserves event
identity but creates fragments. Root Split moves the old root bus into retained
contexts and clears the replacement root's copy. Partition refinement must
separately project the selected Partition's own sounds. Delete keeps intersecting
ancestor-event suffixes; Ungroup uses pre-edit exact prefixes and keeps full
processing history. InsertTime and Splice capture old clocks before mutation.

FrozenAudioContext schema 3 cannot silently omit new sound vocabulary. Preserve
strict legacy adapters: OccurrenceIdentities is reused directly by old adapters,
so adding default fields would open their historical command grammar. A later
core 29/database 35 migration will need frozen core 28 and actual old-CLI fixtures.
The authentic previous CLI is retained at this scratch directory's old-deadpan-cli
with hash in baseline.json. It has not been used to fabricate any new schema.

Full project requirements, gates, aesthetic/physical keyboard/accessibility QA,
real AI, source acquisition, export and release qualification remain active.

## Read-only source-voice integration map

The follow-up explorer proposes a narrower first vertical slice than a separate
voice reader. This is a proposal, not implemented or qualified behavior:

- Retain normal catalog AssetRecords in RenderPlan; it currently retains only
  frozen-context assets. Keep frozen admission mode distinct from ordinary
  project/revision admission. The app already captures every qualified catalog
  asset in playback snapshots, including assets without structural references.
- Derive an opaque AudioSourceVoice from an existing checked AudioSignal owner.
  A recipe contains SourceAudio, an explicit natural SourceAudioMapping and a
  signed AudioSample offset. Reject implicit FitBeat/overflow in this first API.
  Preserve root, definition and enclosing Repeat scope, and retain an immutable
  voice identity. Add a private independent provider discriminator to AudioSignal.
- Emit ordinary Source/Silence leaf spans and reuse TapeProvider::Signal,
  StageAudio::read_tape, WorkControl source preflight/fingerprints, resolve_source,
  signal_source_recipe and prepare_source_block. Existing host providers already
  admit exact original/receipt/index/layout contracts. No new provider trait
  method or parallel PCM engine is needed.
- Retain the structural owner as a separate policy carrier. An input view has
  provider support/route masks; an output view also applies current scoped Hold
  issuers on the consuming grid. Independent source query_flattened cannot be
  used to rediscover the owner's Hold, because it shadows structural content.
  Original timing bindings and fades are not the sound's own processing history.
- Feed the input view to AudioStageProjection's input tape and the output view
  to its separate output policy. Keep all existing descendant, definition and
  Repeat admission checks. A voice on a Retime itself stays outside that stage;
  a properly scoped child voice may enter it. Do not gate away the raw sound
  before Preserve when output-only Hold policy is intended.
- Later allowances must match both immutable voice identity and exact Hold
  issuer. Add a Hold query on an explicit grid for remapped policy, not rescaled
  rounded masks. Keep full processing recipe, source support and projection
  identity through every fragment.
- Route-to-PCM must consume AudioSoundRoute's exact sample map directly, never
  reconstruct a fresh frame-mapped TapeRun from each sampled slice. Selecting a
  prepared Preserve output differs from routing its raw input.

Concrete navigation: deadpan-plan/src/plan.rs RenderPlan; audio_signal.rs
AudioSignal/query_inner; deadpan-audio/src/sequence.rs resolve_source;
session.rs PreparedSource::new; stages.rs read_signal_queries and prepare_bus;
deadpan-app/src/project/service.rs snapshot. The original explorer and this
follow-up did not change files or run Cargo.

First real-PCM tests should register pcm-mono-44100.wav with insertion None,
assert no structural reference, read the voice through the existing tape reader,
and compare arbitrary 48 kHz onsets to an independent resampling reference. Then
compare one enclosing Preserve to a complete canonical preparation, including
cold/cropped/shuffled reads. A silent-Hold owner must expose raw input, suppress
default output and retain its issuer without inheriting Original bindings/fades.
Wrong plan/scope, bad spans, missing qualification and revoked sources must fail;
ordinary Original PCM must remain unchanged. Only after that proof add routing,
allowances, persisted event commands and the complete bus sum.
