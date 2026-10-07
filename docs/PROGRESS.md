# Progress reporting

[Open the dashboard](https://mcp.syas.ca/boris/artifacts/art_6hz2jxipjvvmuy979s3).
Executor artifact ID: **`art_6hz2jxipjvvmuy979s3`**.

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
   `git push`. The dashboard reads GitHub `main` on opening or **Refresh**.
   Local and unpushed changes are not visible there.

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

## Change the dashboard itself

The checked-in component is [artifact.tsx](../tools/progress/artifact.tsx).
It queries `tools.github_com.get_file_contents.queryOptions` with:

```json
{"owner":"micthiesen","repo":"deadpan","path":"docs/progress.json","ref":"refs/heads/main"}
```

The saved connection role is
`{"github_com":"github_com.user.personalGithubMcp"}`. The JSON arrives as a
text resource in the GitHub tool result. Loading and errors are shown explicitly;
there is no embedded fallback report and no polling loop.

Read Executor `skills` for `create-artifact` and `artifact-style`. For a small
change, call `mcp__executor__edit_artifact` with the existing ID and exact
`oldText`/`newText` replacements. For a full rewrite, call
`mcp__executor__create_artifact` with that ID, the full component and the saved
connection role. Update the local component in the same change. If the stored
source is unknown, show the existing artifact first. Keep the ID and URL stable.

The MCP only needs calling when the layout or query changes. Data updates are
ordinary repository changes. If publishing is unavailable, retain the update,
report that the dashboard is stale, continue independent work and retry at the
next useful checkpoint.
