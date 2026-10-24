# Compound wire reuse production evidence package plan

This package records one uninstrumented production build, its retained test and media evidence, and ten diagnostic timing runs. It has no memory-measurement lane.

The approved manifest must carry the exact SHA-256 of this plan. The copier validates that digest before staging and again before publishing. It also retains and rechecks all six build inputs, the candidate and qualified source maps, the two changed Rust source snapshots, all terminal records, the ten native fixture maps, the media evidence inventory, and every approved payload.

Native timing fixtures are archived as authored project bytes only. SQLite `project.sqlite-shm` and `project.sqlite-wal` are retained as separately pinned runtime metadata; the expected sidecar sizes are 32768 and 0 bytes. They are not placed in authored fixture archives. The media evidence payload retains the actual emitted movies and verification data listed by its inventory.

The collector refuses symlinks, special files, unsafe archive paths, destination collisions, stale terminal or source identities, and an existing final destination. It stages the complete package and publishes with Darwin exclusive rename.

Compressed payloads keep their logical `.json` or `.html` destination. Their `encoding` field in the approved manifest and package ledger is authoritative; the stored bytes are gzip and are checked against the original byte count and SHA-256 after decompression. The collector caps each source payload at 25 MB, all approved source payloads plus fixture contents at 150 MB, and each authored fixture at 25 MB. It checks those limits before reading or staging large content and checks the completed staged inventory before publication.
