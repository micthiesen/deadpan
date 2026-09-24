# Owned audio timing bindings

Core 16/database 22 store bounded timing bindings separately from the raw owned
tree. This is the representation and persistence layer for inserted-time audio.
It does not implement the inserted-time command or a binding-aware renderer.
No existing editor command creates bindings in an empty project. Render-plan
compilation explicitly rejects a nonempty binding state rather than ignoring it.
The complete editing lifecycle and PCM integration remain open.

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

## Remaining integration

The renderer must intercept a binding at its physical owner, evaluate the current
owned recipe on the selected lattice, and bypass only that root binding while
honoring descendants. Preserve cache identity must include the evaluation scope.
Current and retained policy must be queried independently on every input/output
grid, including silent intervals that own no input point. Scaling a returned
input mask is insufficient.

Both policy paths evaluate the current owned children and their descendant
bindings. The timing table supplies coordinates; its old policy flags do not
authorize an additional silence mask. Otherwise a Hold changed from Silence to
RoomTone could remain muted by an obsolete flag. Empty retained support needs an
explicit result without discarding the exact policy extent. Repeat gaps still
need their own binding ownership before arbitrary insertion can preserve them.

Only after that consumer and the complete lifecycle are verified can a typed
atomic insertion command capture clocks, isolate a selected occurrence, split
the required context, insert the fallback Hold, transform marks and generation
relevance, and commit one revision. The native interface must then expose the
operation with its visible keybinding and undergo keyboard and aesthetic review.
