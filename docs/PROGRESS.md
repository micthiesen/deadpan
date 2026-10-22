# Progress reporting

[Open the dashboard](https://artifacts.boo/a/a3ysikxugbmr4).
Artifact ID: **`a3ysikxugbmr4`**.

[Spec §29.2](spec/DEADPAN_SPEC.md#292-persistent-progress-dashboard) requires
occasional milestone updates whenever an agent works against the specification.
No separate reminder, scheduled task or periodic MCP call is needed.

## Update a milestone

1. Reconcile the implemented work and verification in [Requirements](REQUIREMENTS.md).
2. Edit feature notes in [progress.json](progress.json) if the high-level story changed.
3. Record the milestone. Keep the estimate unchanged when work adds confidence
   without materially changing coverage. Use the verified implementation commit:

   ```sh
   python3 tools/progress/update.py record \
     --percent 85 \
     --summary "A concise description of the verified milestone" \
     --focus "What is being worked on now" \
     --revision VERIFIED_COMMIT
   python3 tools/progress/update.py check
   ```

   `--revision` defaults to HEAD. `--stage ai=mostly` can change a feature's stage.
   The script appends history, updates the date and strict section count, and
   validates the report. It does not commit, push, call MCP or increase progress
   automatically. It refuses to call a group done while a mapped requirement
   remains partial.
4. Review and commit the data with the normal milestone documentation, then
   `git push`. The dashboard's live data source re-fetches the raw file from
   GitHub `main` every 15 minutes. Local and unpushed changes are not visible.

Update after meaningful verified milestones or a substantial work block, not
each test or commit. Skip updates when nothing useful changed. Preserve earlier
history; a correction is a new observation with a short explanation. Unfinished
implementation can appear in Current work without raising the percentage.

## Status meanings

| Stored stage | Display | Meaning |
| --- | --- | --- |
| `planned` | Planned | No implemented usable path yet. |
| `partial` | Partly built | Important capabilities still need implementation. |
| `mostly` | Mostly built | Main workflow works; meaningful spec or verification gaps remain. |
| `done` | Done & verified | Every mapped requirement is Complete under spec §29.1. |

Feature groups partition all DP-01 through DP-24 requirements. The top percentage
is a rough engineering estimate across the detailed spec, not the fraction of
closed groups. Strict section completion remains visible separately. The initial
75% baseline is retrospective. Each estimate has roughly ±5 percentage points
of uncertainty. The displayed average uses calendar days
between observations and is not a completion forecast.

## How the dashboard gets its data

The artifact (Artifacts, https://artifacts.boo) contains
`app.tsx` and `data/progress.json`. Its live data source replaces
`data/progress.json` with
`https://raw.githubusercontent.com/micthiesen/deadpan/refs/heads/main/docs/progress.json`
every 15 minutes; the owner approved `raw.githubusercontent.com` for this.
Check or refresh it with the Artifacts `artifact_source` tool
(`{"id": "a3ysikxugbmr4"}` lists it; add `"path": "data/progress.json",
"refresh": true` to fetch now).

## Change the dashboard itself

The checked-in component is [app.tsx](../tools/progress/app.tsx). It reads the
report with `useData("progress.json")` and uses the `@a/ui` kit.

Use the Artifacts MCP tools (through Executor as `artifacts`, or
directly), not Executor's built-in artifacts. Call `artifact_guide` once. For a
small change, call `artifact_edit` with `id: "a3ysikxugbmr4"`, `path: "app.tsx"`
and exact `oldText`/`newText` edits; for a rewrite, `artifact_write_files` with
the full `app.tsx`. Each write returns a render check and `artifact_check` returns
a screenshot; fix failures before finishing. Update the local component in the
same change and keep the ID stable.

The MCP only needs calling when the layout changes. Data updates are ordinary
repository changes. If publishing is unavailable, retain the update, report that
the dashboard is stale, continue independent work and retry at the next useful
checkpoint.
