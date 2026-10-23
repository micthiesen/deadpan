#!/usr/bin/env python3
"""Strict parser for DEADPAN_PAUSE_ROUTE_PROFILE_V1 stderr blocks."""
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
from typing import Any

PREFIX = "DEADPAN_PAUSE_ROUTE_PROFILE_V1"
PREFIX_FAMILY = "DEADPAN_PAUSE_ROUTE_PROFILE"
MAX_U64 = (1 << 64) - 1
PHASES = (
    "native_admission",
    "semantic_plan",
    "native_continuation",
    "store_entry_misc",
    "store_prepare",
    "host_runtime_bank",
    "store_write",
    "sqlite_commit",
    "store_cache_publication",
    "native_postcommit",
    "workspace_refresh",
    "native_finish",
    "native_publication",
)

_DECIMAL = r"([0-9]+)"
_BEGIN = re.compile(rf"^{PREFIX} BEGIN count=1$")
_PHASE = re.compile(rf"^{PREFIX} PHASE name=([a-z_]+) elapsed_ns={_DECIMAL} count=1$")
_END = re.compile(
    rf"^{PREFIX} END count=1 elapsed_ns={_DECIMAL} committed=1 refreshed=1 published=1$"
)
_EVENT = re.compile(r"^(?:BEGIN|PHASE|END)")


class ProfileError(ValueError):
    """Malformed, incomplete, duplicated, or inconsistent profile output."""


def _u64(value: str, *, line_number: int, label: str) -> int:
    number = int(value)
    if number > MAX_U64:
        raise ProfileError(f"line {line_number}: {label} exceeds u64")
    return number


def _looks_like_trace(line: str) -> bool:
    return (
        PREFIX_FAMILY in line
        or line.startswith("DEADPAN_PAUSE_ROUTE")
        or _EVENT.match(line) is not None
    )


def parse_profile(text: str) -> dict[str, Any]:
    """Parse complete trace blocks; first complete block is selected by order."""
    blocks: list[dict[str, Any]] = []
    active: dict[str, Any] | None = None
    next_phase = 0

    for line_number, raw in enumerate(text.splitlines(), 1):
        if not _looks_like_trace(raw):
            continue
        if _BEGIN.fullmatch(raw):
            if active is not None:
                raise ProfileError(f"line {line_number}: nested BEGIN before prior END")
            active = {"phases": []}
            next_phase = 0
            continue
        if active is None:
            raise ProfileError(f"line {line_number}: unmatched or malformed profile trace line")

        match = _PHASE.fullmatch(raw)
        if match:
            name = match.group(1)
            if next_phase >= len(PHASES):
                raise ProfileError(f"line {line_number}: extra PHASE row")
            expected = PHASES[next_phase]
            if name != expected:
                raise ProfileError(
                    f"line {line_number}: phase {name!r} out of order; expected {expected!r}"
                )
            elapsed_ns = _u64(match.group(2), line_number=line_number, label="elapsed_ns")
            active["phases"].append({"name": name, "elapsed_ns": elapsed_ns, "count": 1})
            next_phase += 1
            continue

        match = _END.fullmatch(raw)
        if match:
            if next_phase != len(PHASES):
                raise ProfileError(
                    f"line {line_number}: END after {next_phase} phases; expected {len(PHASES)}"
                )
            elapsed_ns = _u64(match.group(1), line_number=line_number, label="END elapsed_ns")
            phase_sum_ns = sum(row["elapsed_ns"] for row in active["phases"])
            if phase_sum_ns > MAX_U64:
                raise ProfileError(f"line {line_number}: phase duration sum exceeds u64")
            if phase_sum_ns != elapsed_ns:
                raise ProfileError(
                    f"line {line_number}: phase sum {phase_sum_ns} != END elapsed_ns {elapsed_ns}"
                )
            active.update({
                "schema": PREFIX,
                "begin_count": 1,
                "end_count": 1,
                "elapsed_ns": elapsed_ns,
                "phase_sum_ns": phase_sum_ns,
                "committed": 1,
                "refreshed": 1,
                "published": 1,
            })
            blocks.append(active)
            active = None
            continue

        raise ProfileError(f"line {line_number}: malformed or unexpected profile trace line")

    if active is not None:
        raise ProfileError("input ends before profile END")
    if not blocks:
        raise ProfileError("no complete profile block found")
    return {
        "schema": PREFIX,
        "block_count": len(blocks),
        "selected_block_index": 0,
        "selected": blocks[0],
        "blocks": blocks,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", required=True, type=Path, help="captured stderr file")
    parser.add_argument("--output", required=True, type=Path, help="parsed JSON destination")
    args = parser.parse_args()
    try:
        text = args.input.read_text(encoding="utf-8", errors="strict")
        result = parse_profile(text)
    except (OSError, UnicodeError, ProfileError) as exc:
        parser.error(str(exc))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
