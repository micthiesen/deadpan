# Owned audio timing bindings

Core 16/database 22 store bounded timing bindings separately from the raw owned
tree. The normal plan and `StageAudio` paths evaluate these bindings with retained
sampling, current owned policies and post-mapping fades. A pure capture helper
can capture previously unbound physical owners without expanding Repeat plays.
No editor command creates bindings in an empty project yet. Arbitrary Hold
insertion and the complete authoring lifecycle remain open.

The [single-Original workflow](SINGLE_ORIGINAL.md) remains the product model:
reshape the full source with reversible changes. Timing records are internal
authored state, not another source, media bin, or interface mode.

## One recipe, explicit clocks

The current owned Source, Hold or nonunity Preserve supplies the raw recipe.
Its binding supplies the sampling lattice. A flat table stores immutable
`FrozenAudioLayout` records under allocation-revision/ordinal identities. A
record contains timing geometry and stable play placement, not another binding
or a historical media body. Several bindings and phase terms can share it.

`AudioPlacementTemplate` identifies a physical alias in one retained layout,
the clock root, its Repeat arguments, and ordered birth clauses. Clock roots are
explicit:

- Project output uses absolute 48 kHz ties-to-even allocation.
- A Preserve input uses a PointCeil grid at its exact selected input origin.
- A definition birth uses a local-zero PointCeil grid over an explicit retained
  definition root.

PointCeil is not simulated by shifting a round-even grid. Ties-to-even parity
would make such a substitution incorrect. Scoped projection retains exact
origin and scale and does not clamp hidden context to project zero.

Live Repeat arguments resolve stable identities in the current occurrence.
Captured arguments name a specific old play, including an owned override.
Surviving default plays retain their placement. A born default contribution
selects the innermost applicable birth clause and its recorded definition root
in the same retained layout. It never substitutes an old ordinal or uses a
current default alias as historical geometry. Exposing a previously overridden
default also requires a definition birth.

Explicit definition evaluation names excluded outer Repeat scopes. Only those
exclusions permit canonical birth resolution; a missing argument in an ordinary
occurrence remains an error. A later wrapper does not rewrite an established
intrinsic lattice merely because it adds an enclosing Repeat.

## Phase expressions

A resume retains a local boundary and an exact local-frame phase. The phase is
a constant plus bounded terms, each with its own placement template and two
local boundaries. A term evaluates:

```text
(B(clock(to_local)) - B(clock(from_local))) * local_frames_per_sample
```

Each `B` uses that placement's actual grid. This permits compact shared-definition
edits without expanding plays. At 30000/1001 fps a local one-frame cut can own
1602 samples in one play and 1601 in another. A single precomputed rational
delta would erase this difference.

For a chosen old lattice, let `g0` be its independently allocated meaningful
start and `u` its local frames per reference sample. The intended consumer map
is:

```text
q(anchor) = g0 + resolved_local_phase / u
q(n) = q(anchor) + (n - B(current_local_anchor)) * current_local_step / u
```

A subsequent cut adds its difference on the then-current clock to the existing
phase expression. It must not reconstruct phase from picture coordinates or
change speed to fit rounded sample counts. Concrete occurrence authoring may
fold evaluated terms to an exact constant after the occurrence is isolated.

## Persistence and ownership

Documents omit empty `audio_bindings`. Nonempty state has strict bounded JSON,
unique timing records and owned binding keys, validated reference paths and
clock roots, and no unused timing table entries. Every persisted replacement is
an exactly guarded reversible patch. Timing and retained identity allocations
participate in the store's never-reused revision namespace.

Transparent Split and occurrence isolation copy live owner/Repeat names in both
the primary lattice and every phase term. Historical aliases and captured play
names stay fixed. Removal prunes dead owners and timing tables in the same
transaction; inverse history restores them. These operations do not implement
the remaining raw-recipe, movement, crop and birth lifecycle rules.

Database schemas 1 through 21 replay through their frozen grammars into schema
22. Legacy documents gain empty binding state. Legacy wire formats reject the
new field, including empty/null values. Migration does not infer bindings from
lineage, source coordinates, matching PCM, or old structural edits.

`FrozenAudioContext` schema 1 cannot retain binding state. Capture therefore
rejects a nonempty state explicitly; it cannot silently export an unbound recipe
as an equivalent historical context.

## Rendering and policy

The renderer intercepts a binding at its physical owner, evaluates the current
owned recipe on the selected lattice, and bypasses only that root binding while
honoring descendants. Preserve cache identity includes the evaluation scope and
root bypass. Current and retained policy are queried independently on every
input/output grid, including silent intervals that own no input point. Scaling
a returned input mask is insufficient.

Both policy paths evaluate the current owned children and their descendant
bindings. The timing table supplies coordinates; its old policy flags do not
authorize an additional silence mask. Otherwise a Hold changed from Silence to
RoomTone could remain muted by an obsolete flag. Empty retained support has an
explicit result without discarding the exact policy extent. Current Edit crops
constrain raw filter support; transparent partitions and root allocation do not
remove hidden physical context. Current coincident boundary owners retain their
creative Hard choices in the retained placement.

Raw endpoint masks stay on their physical sampling grid. A downstream Preserve
receives those masked inputs, then may produce ordinary processing decay through
an absent Source. Do not turn an empty PointCeil operand into an explicit output
mute after crossing that stage. Authored SilentHold policy still applies on each
output grid, even when its input interval contained no point. Capturing timing
alone must not convert absent audio into intentional silence.

Root and PointCeil transfers share source provenance, cancellation, deadlines,
work, residency and nested preparation limits. Cache hits retain relative depth
as well as transitive source dependencies. A warmed shallow cache cannot bypass
the limit in a deeper binding evaluation. Query work includes resolution and
both policy paths. Source-only `SequenceAudio` rejects nonempty bindings before
media access; use `StageAudio` for these documents. Context-schema-1 capture
still rejects them because that historical format cannot retain their meaning.
The transfer carrier requires its full retained support length to fit a positive
i64, even for a short requested block. Unrepresentable carriers fail explicitly;
the independent physical-domain inspection API supports wider signed spans.

## Creative fade clock

Creative fades are applied once, after all time/pitch mapping. Raw binding reads
retain endpoint suppression without baking fades into a Preserve input. Fade
queries stop at the first physical output before crossing a nonunity Preserve;
its owned output leaves supply envelope geometry. A descendant input binding
does not substitute its input-grid fade for an output-grid fade.

For a translated binding, retain its original meaningful envelope length and
advance the retained progress. This matters even for a one-frame clip: at
32000 fps its allocation can change from two samples to one after a move, but
the remaining sample must retain the two-sample fade.

A new rate uses a virtual creative clock. Keep the retained grid origin,
boundary rule and physical owner origin `o`; rescale exact envelope geometry
around `o` by the current-to-retained physical scale `s`. For raw envelope
extent `[a,b)`, retained boundary function `B`, mapped sample `q` and reference
samples per output sample `r`:

```text
V(x) = o + (x-o)*s
N = B(V(b)) - B(V(a))
progress = (q-B(a))/r
```

Progress advances one output sample per sample. A later translation changes
neither `N` nor progress. This deliberately uses the retained virtual origin,
not a new current absolute origin on each rate edit. Both root ties-to-even and
selected-origin PointCeil remain distinct. For `N >= 2`, the sample-centered
linear ramps use `F=min(96,N/2)` output samples; Hard disables only its applicable
ramp. `N < 2` has no creative attenuation. Retained raw endpoint and current
silence policies remain independent, so virtual fade sizing never grants a
sample outside the raw domain.

## Capture and remaining authoring

`capture_unbound_audio_bindings` returns a complete candidate binding state;
it does not mutate the document or allocate a revision. Existing bindings stay
unchanged. One bounded owned-tree traversal visits default and override
subtrees. Default edges retain live stable play arguments plus birth clauses;
override edges capture their exact play. Nonunity Preserve output uses the
enclosing clock, then its input resets lexical scope at the exact selected
origin. One shared frozen timing record is added only when needed.

Repeat gaps still need their own binding ownership. Complete capture rejects a
nonempty gap rather than claiming continuity while omitting it. Raw recipe/rate
and support changes still need explicit command lifecycle rules, including
replacement of affected opaque ancestors and preservation of unaffected owners.

After the complete lifecycle is verified, a typed
atomic insertion command capture clocks, isolate a selected occurrence, split
the required context, insert the fallback Hold, transform marks and generation
relevance, and commit one revision. The native interface must then expose the
operation with its visible keybinding and undergo keyboard and aesthetic review.
