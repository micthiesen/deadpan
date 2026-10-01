# Ripple Source edge trimming

The shared command supports qualified Source In/Out ripple trimming.
[Qualification](qualification/source-trim-2026-10-01.md) covers exact geometry,
indexed pictures, decoded PCM and store/headless admission. Full
[Trim mode](spec/DEADPAN_SPEC.md#77-trim-mode), overwrite and Roll remain required.

`TrimSource` moves either the In or Out edge by signed whole project frames.
Positive moves the edge later in Original material. In `mode: ripple`, moving
In later shortens the beat; moving Out later lengthens it. The suffix moves by
the resulting duration change. The command captures an explicit ordinary
Sequence parent, direct selected child, revision, timing identity and an optional
fresh crop-wrapper identity. It commits one reversible transaction.

## Scope and exact limits

The selected target is a qualified Source or one unity Partition of a
Source, beneath ordinary Sequence ancestors. Admission shares the
[Slip](SOURCE_SLIP.md) full measured spans, coherent linked audio/video affine
clock, independent audio offset and explicit editorial window checks. Picture
support bounds the current and proposed effective selection. Unsupported targets
and arithmetic overflow fail explicitly. The command does not refit source rate.

Let the physical Source occupy `[0,D)`, its delivered allocation be `C=[c0,c1)`,
and its exact editorial window be `W`. The effective selection is
`E=W∩C=[a,b)`. For integer movement `d`:

| Edge | New allocation | New effective selection | Duration change |
| --- | --- | --- | --- |
| In | `[c0+d,c1)` | `[a+d,b)` | `-d` |
| Out | `[c0,c1+d)` | `[a,b+d)` | `d` |

Both fractional gaps remain unchanged. The result must retain positive exact
selected time and at least one delivered frame. Requests clamp inward to whole
frames, with the exact rational bound, inclusive/exclusive boundary and limiting
picture or minimum-duration reason reported. A resolved zero preserves the full
Source representation, creates no candidate transaction and consumes no history.

Hidden selected context is retained where possible. If an edited effective edge
coincides with its allocation edge, contraction can leave W unchanged. An edge
with fractional padding moves the corresponding exact endpoint of W. Extension
beyond W widens only that endpoint. Video selection is W intersected with its
measured support. Linked audio uses `(W∩audio_support)-audio_offset`, preserving
empty support and absent audio as distinct states.

## Physical ownership and effects

A contraction crops the retained physical Source and never shrinks it. A direct
Source receives one neutral Partition if needed; an existing Partition keeps
its identity, including after re-extension to the full owner. Only new physical
prefix or tail needs growth. For the signed proposed allocation:

```
p = max(0, -new_c0)
new_D = max(D+p, new_c1+p)
```

Translate the allocation, W, explicit source maps and selected support by p.
Keep the same Source identity, full measured spans, affine slope and independent
audio offset. Source framing retains its old evaluation domain and endpoint
poses outside it. Prefix growth translates local gain/mute coordinates and audio
binding references once. Ancestor framing stays live on its changed duration.

A Partition keeps raw sampling transparent. When W is unchanged it retains
hidden Source filtering context; changing W may change its endpoint filtering.
An authored Trim separately records an editorial edge on the changed side and
the incident side of the adjacent beat. Neighbor lookup crosses ordinary
Sequence ancestors and skips zero-duration children. Positive silent time ends
that lookup, so a distant voice is not treated as adjacent.

These edges use the shared short creative fade on the consuming output clock.
They do not alter filtering support, retained sample phase or the independent
root sound bus. Existing exactly coincident Hard policies still win. The
unchanged opposite edge retains its original fade progress. Splitting a marked
owner retains its full context instead of copying the new fade to an interior
split. See [audio edges](AUDIO_EDGES.md) for exact precedence, retained
envelopes and delivered-interval fade widths.

## Timing, sounds and marks

Capture audio clocks from the unchanged tree before altering maps, duration or
crop. Existing lattices, resumes and chronological reanchors remain. In trims
capture the retained target entry; both edges capture each shifted suffix owner,
including later ordinary ancestors' siblings, compact Repeat/gap owners and
opaque Preserve outputs. Target and suffix share one capture and entry budget.
Append old-coordinate reanchor steps before applying any target-only physical
prefix. Absolute sample counts remain `B(end)-B(start)`.

The independent root sound bus receives exactly one insertion or deletion at
the edited output edge. It is detached for structural timing capture and restored
against the final document. Existing silent-Hold allowances retain issuer identity;
trimming introduces no silent Hold.

Translate decomposed physical content points by p before mark reconstruction.
Source PTS anchors and host edge sentinels retain their own semantics. A mark
stored on a retained physical Source can stay Bound behind a crop while a query
through that occurrence is unavailable. Ancestor-local and concrete Occurrence
marks apply their existing loss policies. Extension never silently revives an
already unresolved mark. Changed audio context invalidates its PCM identity while
preserving the retained sampling binding.

## Store and shared command path

The store preview resolves one immutable snapshot and rechecks its stored source
receipt, full asset metadata and ownership. It returns `source_trim` resolution
and a nullable `edit`. Zero previews still check revision, timing allocation,
wrapper absence and media admission. Commit re-resolves the original request and
uses normal atomic revision/history storage. Preview data grants no bypass.

Core schema 40 and database 49 identify this command grammar. Unused development
databases 39 through 48 are refused without migration under the session's format
policy; the retained adapters for older frozen grammars cannot author Trim.
Frozen audio context schema 6 retains the same editorial intent without using
it to constrain raw sampling. Older context grammars reject the new field,
including an explicitly empty value.

## Remaining work

The native boundary-picture pair, waveform, refinement, audition, `,v` mode entry,
Tab cycling and ripple/overwrite controls are not connected by this backend
increment. Audio-only picture lead/tail, broader nested or treated targets and
Repeat occurrence isolation remain open. The retained qualification does not
establish native Trim interaction, device playback, export or release acceptance.
Ungroup also refuses a Sequence carrying an editorial edge until an explicit
transfer or clear operation can preserve its intent.
