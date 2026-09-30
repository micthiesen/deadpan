#!/usr/bin/env python3
"""Retain an authentic schema-41 SQLite snapshot without rewriting any cell."""

import argparse
import hashlib
import json
from pathlib import Path
import sqlite3
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("package", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    source = args.package.resolve() / "project.sqlite"
    if not source.is_file() or source.stat().st_size > 64 * 1024 * 1024:
        raise ValueError("expected a bounded existing project database")
    with tempfile.TemporaryDirectory(prefix="deadpan-schema41-fixture-") as scratch:
        backup_path = Path(scratch) / "consistent.sqlite"
        with sqlite3.connect(source.as_uri() + "?mode=ro", uri=True) as original:
            with sqlite3.connect(backup_path) as backup:
                original.backup(backup)
        with sqlite3.connect(backup_path) as retained:
            version = retained.execute("PRAGMA user_version").fetchone()[0]
            application = retained.execute("PRAGMA application_id").fetchone()[0]
            if version != 41 or application != 0x44504E31:
                raise ValueError("source is not an authentic schema-41 Deadpan database")
            if retained.execute("PRAGMA integrity_check").fetchone()[0] != "ok":
                raise ValueError("source integrity check failed")
            if retained.execute("PRAGMA foreign_key_check").fetchall():
                raise ValueError("source foreign-key check failed")
            counts = {}
            for table in (
                "render_jobs", "render_attempts", "render_job_heads",
                "render_candidate_checkpoints", "render_publications",
                "render_publication_operations",
            ):
                counts[table] = retained.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
                if counts[table] <= 0:
                    raise ValueError(f"source has no authentic {table} evidence")
            if retained.execute(
                "SELECT COUNT(*) FROM sqlite_schema WHERE name='render_encoding_decisions'"
            ).fetchone()[0]:
                raise ValueError("source already contains new decision vocabulary")
            lines = list(retained.iterdump())
        lines.extend((f"PRAGMA application_id = {application};", "PRAGMA user_version = 41;"))
        dump = ("\n".join(lines) + "\n").encode()
        if len(dump) > 64 * 1024 * 1024:
            raise ValueError("fixture dump exceeds bound")
        provenance = {
            "source": str(source),
            "source_schema": version,
            "prior_implementation_revision": "4594f5b",
            "method": "read-only SQLite backup followed by complete iterdump; no rewritten cells",
            "counts": counts,
            "snapshot_sha256": hashlib.sha256(backup_path.read_bytes()).hexdigest(),
            "fixture_sha256": hashlib.sha256(dump).hexdigest(),
        }
        with args.output.open("xb") as output:
            output.write(dump)
        with args.output.with_suffix(".provenance.json").open("x") as output:
            json.dump(provenance, output, indent=2)
            output.write("\n")
        print(json.dumps(provenance, indent=2))


if __name__ == "__main__":
    main()
