# Focused polish review: Camera face status and badges

## Findings

None. The proposal overlays now use compact page digits while the inspector retains each full `Face N · proposal` label. The per-Camera footer transitions from pending to found/empty/failure status when the matching result is admitted. The service updates its general pending notice only if it still owns the `FINDING_FACES` message; saves and other later notices therefore remain authoritative. The service regression covers a delayed completion after a typed save, and replay checks found and empty completion notices.

Read-only review only. No build or app run.
