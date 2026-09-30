# Publication documentation review

Read-only draft. No repository files were changed and no checks were executed. The parent reports 26 focused tests passing; fresh seven-project publication and independent-reader evidence are still pending. Do not reuse the earlier verifier's seven-file results as publication qualification. Keep existing historical measurements attached to their original milestones. Add final publication measurements only after the parent's run completes.

The new `docs/RENDER_PUBLICATION.md` accurately describes the implemented library boundary. The main documentation defect is that older current-state summaries still call destination publication unimplemented. Proposed replacements below preserve DP-17 as Open and leave every DP/gate status unchanged.

## 1. `docs/RENDER_PUBLICATION.md`

Keep the contract. Two short clarifications would improve its public API meaning.

Replace the final sentence of the opening paragraph:

```markdown
This library boundary leaves native Render, public headless render commands and
durable render jobs as separate work.
```

After the first paragraph under **Report evidence and bounds**, add:

```markdown
The sibling report is named `deadpan-render-<publication_id>.json`. It records
evidence prepared for publication, since it is committed before the movie.
Only the returned outcome distinguishes `Published` from `PublishedUnconfirmed`;
the report alone does not prove the movie rename or final durability succeeded.
```

The report's actual scope is `verified_candidate_prepared_for_atomic_publication`.
Keep the existing separate treatment of orphan reports, the movie rename commit
point, retained candidates before that point and bounded post-commit readback.
Do not add a passing publication qualification claim yet.

## 2. `docs/ENCODED_RENDER.md`

Keep the opening statement that `encode` does not verify or publish: it correctly
describes this particular API. Replace the final paragraph with:

```markdown
The separate [finished-file verifier](FINISHED_FILE_VERIFICATION.md) checks actual
MP4 tables, every decoded picture, fresh GOPs and manual/ordinary AAC presentation
before returning a private verified candidate. The [publication host](RENDER_PUBLICATION.md)
then checks an exact destination copy and publishes the local report and MP4 under
exclusive names, with explicit outcomes for failures after the movie rename.
Durable render jobs/recovery, native Render and public headless render commands
remain required. Full audio/effects, HDR, automatic platform policy, performance
and release hardware/OS coverage also remain open. Fixture content and platform
qualification remain separate from per-file structural/decode admission.
```

## 3. `docs/FINISHED_FILE_VERIFICATION.md`

Keep the opening API boundary and the statement that copying a verified candidate
alone grants no publication authority. Replace the last three sentences after
“rendered picture or sound.” with:

```markdown
Fixture content, absolute event synchronization, AVFoundation compatibility and
platform/runtime behavior have separate actual qualification. The library
[publication host](RENDER_PUBLICATION.md) checks destination byte identity against
the verified candidate, writes the historical local report and atomically commits
the movie without replacing an existing entry. Automatic platform policy, durable
render jobs/recovery, native Render, public headless render commands, complete
mastering/effects, HDR and release coverage remain required.
```

## 4. `docs/ARCHITECTURE.md`

### Present implementation: `deadpan-cli` row

In the responsibility cell, replace `isolated encoding/verification and diagnostics`
with `isolated encoding/verification, destination publication and diagnostics`.

Replace the last sentence of its boundary cell with:

```markdown
The [publication host](RENDER_PUBLICATION.md) owns bounded destination copies,
byte readback, historical provenance, exclusive report/movie renames and truthful
post-commit outcomes. Durable render jobs/recovery, legacy Accepted/Still readers,
full mixes, native Render and public headless render commands remain open.
```

### Paragraph below the implementation table

Replace the final two sentences beginning “These limits can reject…” with:

```markdown
These limits can reject candidates within encoder capacity. The separate
[publication host](RENDER_PUBLICATION.md) carries verified byte identity through
destination staging and readback, publishes a bounded local report, and treats
the movie rename as the commit point. The report and movie are separate atomic
renames; a later failure is reported as `PublishedUnconfirmed`. Durable jobs and
recovery, automatic platform policy, full mastering/effects, HDR and the native
Render workflow remain required.
```

### Section 24 map: lower `deadpan-cli` row

Replace its implementation cell with:

```markdown
Project/command/history/migration, picture/audio-plan and source-PCM inspection,
boundary resolution, isolated committed SDR encoding, finished-file
structural/decode verification and library destination publication with historical
provenance implemented. Durable render jobs/recovery, native Render, public
headless render commands, automatic platform policy, full audio/effects/HDR and
benchmarks remain open.
```

No changes are needed to the encoder/source/render crate ownership rows: they
correctly deny those lower layers publication responsibility.

## 5. `docs/REQUIREMENTS.md`

### New leading implementation paragraph

Insert after the introductory paragraph, before the existing verifier milestone:

```markdown
The library [publication host](RENDER_PUBLICATION.md) now accepts a private verified
candidate, captures its historical revision and dependency evidence, and copies it
to an exclusive destination `.partial`. Exact descriptor readback preserves the
verified movie identity. It publishes a bounded local report before the movie's
atomic no-replace rename, preserves retryable candidates on earlier failure and
reports later failures as `PublishedUnconfirmed`. Report and movie publication
are separate commits. Durable render jobs/recovery, native Render, public headless
render commands, automatic platform policy, complete mastering/effects, HDR and
release qualification remain open. DP-17 stays Open; no requirement or gate is
completed by this library boundary.
```

Append final publication measurements and the evidence link here when available.
The current 26 focused passes are interim evidence, not the complete qualification.

### Earlier milestone summaries near the top

Use these exact replacements to remove stale current-work lists while preserving
their historical measurements:

- Verifier paragraph: replace `Durable render jobs/recovery, destination .partial verification and atomic publication, native Render, full mastering/effects and HDR remain required.` (retain the existing backticks in the anchor) with `Library destination publication is described above. Durable render jobs/recovery, native Render, automatic platform policy, full mastering/effects and HDR remain required.`
- Encoded-worker paragraph: replace `Durable jobs, publication, native Render, full audio/effects and HDR remain required.` with `Durable jobs/recovery, native Render, automatic platform policy, full audio/effects and HDR remain required.`
- Native-encoder paragraph: replace `Isolated structural/decode verification now exists; durable jobs, publication, native Render, full audio/effects and HDR remain required.` with `Isolated verification and library publication now exist; durable jobs/recovery, native Render, automatic platform policy, full audio/effects and HDR remain required.`
- Raw-worker paragraph: replace `Later boundaries add encoding and structural/decode verification; durable render jobs, complete audio/effects, publication and native Render remain required.` with `Later boundaries add encoding, structural/decode verification and library publication; durable render jobs/recovery, complete audio/effects and native Render remain required.`
- Encoder-picture paragraph: replace `Later boundaries add encoding and structural/decode verification. Complete audio/effects, publication and native Render remain required; statuses do not change.` with `Later boundaries add encoding, structural/decode verification and library publication. Complete audio/effects, durable render jobs/recovery and native Render remain required; statuses do not change.`

### DP-17 row

After the production-verifier evidence sentence, add:

```markdown
The [publication host](RENDER_PUBLICATION.md) adds exclusive destination staging,
exact byte readback, a historical local report and atomic movie publication with
explicit post-commit outcomes.
```

Replace `Source capacity, content/runtime qualification, publication and the native Render workflow remain open.` with `Source capacity, content/runtime qualification, automatic platform policy and the native Render workflow remain open.`

Replace the **Remaining work** cell with:

```markdown
Durable render jobs/recovery, native Render and public headless render commands,
automatic platform policy, full mastering/effects and HDR, expanded content/runtime
qualification, and the complete native workflow and emitted-file corpus.
```

Keep `Open` and `No complete product export path is qualified.`

### DP-18 row

Append to the implementation cell:

```markdown
The [publication host](RENDER_PUBLICATION.md) preserves the verified candidate and
retained paths on precommit failures, while distinguishing a committed movie
whose final durability or integrity could not be confirmed.
```

Remove `verified atomic publication,` from its remaining-work cell. Retain
durable render jobs/recovery, scheduling, app integration, application lifecycle
and chaos coverage. Do not change its Partial status.

DP-21's `final render operations` remains valid because this work exposes a library
API, not an ordinary CLI/JSON command. Gate F and Gate G should remain unchanged.

## 6. `docs/spec/AGENT_HANDOFF.md`

Insert the following newest section before **Isolated finished-file verification**:

```markdown
## Verified destination publication, 2026-09-29

The library [publication host](../RENDER_PUBLICATION.md) now takes a private
verified candidate and an explicit MP4 destination. It pins the destination
parent, stages exclusive sibling partials, checks exact destination bytes and
publishes the local report before atomically renaming the movie without replacing
an existing entry. Before that rename, failure preserves the verified candidate
and diagnostic recovery paths. After it, bounded integrity and durability checks
finish despite late cancellation; a failure returns `PublishedUnconfirmed`.
The report and movie are separate commits, so an orphan report can remain.

Provenance binds the captured historical revision and full document hash. Source
receipts supply original object identities and SHA-256; the catalog is explicitly
a committed superset. Effective Generated intervals follow the indexed picture
resolver through sparse plays, gaps and retiming, carrying complete artifact and
immutable provenance identities. Source labels, URLs and linked paths are omitted.
Capacity limits fail explicitly and do not truncate the report.

Durable render jobs/recovery, native Render, public headless render commands,
automatic platform policy, complete mastering/effects, HDR and release
qualification remain open. No DP requirement or Gate A through G is complete.
```

Insert actual measured publication results between the second and third
paragraphs after the parent's checks complete. Keep earlier milestone counts and
their source links intact.

Then update the five obsolete follow-up lists in preceding milestone text:

1. Finished-file verification: replace the two lines beginning `Next add durable render jobs/recovery, destination` through `and native Render.` with `The library publication boundary is described above. Next add durable render jobs/recovery, native Render and public headless render commands.` Keep the full mastering/HDR/product caveat.
2. Encoded render: replace `Next add durable jobs/recovery, verified atomic destination publication and native Render.` with `Library publication now exists. Next add durable jobs/recovery, native Render and public headless render commands.`
3. Native encoding: replace `Add durable render jobs/recovery, verified atomic destination publication and native Render.` with `Library publication now exists. Add durable render jobs/recovery, native Render and public headless render commands.`
4. Raw picture worker: replace `Durable render jobs/recovery, full audio/effects, atomic publication, native Render, HDR and all remaining product requirements stay open.` with `Library publication is described above. Durable render jobs/recovery, full audio/effects, native Render, HDR and all remaining product requirements stay open.`
5. Encoder pictures: replace `Complete the shared audio/effects graphs, durable render jobs/recovery, verified atomic publication and native Render.` with `Library publication is described above. Complete the shared audio/effects graphs, durable render jobs/recovery and native Render.`

## Claims to preserve

- The MP4 rename is atomic; the report/MP4 pair is not one atomic transaction.
- Destination readback proves exact equality with the already decoded private candidate. It does not perform another media decode at the destination.
- `PublishedUnconfirmed` means the movie was renamed; it is not a retryable unpublished failure.
- Local reports and receipts do not reconstruct `VerifiedCandidate`, authorize cleanup/adoption or make crash recovery implemented.
- The Generated flag supports a future nonblocking disclosure reminder. No upload metadata or reminder UI was added.
- Automatic encoding policy, native UI, public command routing and complete §22 export remain open.
