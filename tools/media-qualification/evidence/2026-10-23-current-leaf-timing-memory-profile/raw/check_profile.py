"""Draft only: bounded exact parser for the proposed current leaf profile."""

import argparse
import json
import re
from pathlib import Path

PREFIX = b"DEADPAN_CURRENT_LEAF_PROFILE"
MAX_U64 = (1 << 64) - 1
MAX_U128 = (1 << 128) - 1
MAX_LINE_BYTES = 65_536
MAX_STDERR_BYTES = 64 * 1024 * 1024
PHASES = (
    ("app_service_total", "inclusive_native_route"),
    ("semantic_leaf_apply_call", "disjoint_leaf_call_in_semantics"),
    ("replay_leaf_apply_call", "disjoint_leaf_call_in_replay"),
    ("app_refresh_succeeded", "zero_time_success_marker"),
)
DECIMAL = rb"(0|[1-9][0-9]*)"
ROW = re.compile(
    re.escape(PREFIX)
    + rb" PHASE phase=([a-z_]+) kind=([a-z_]+) elapsed_ns="
    + DECIMAL
    + rb" count="
    + DECIMAL
)


class ProfileError(ValueError):
    pass


def checked_add(left, right, ceiling, label):
    if left > ceiling - right:
        raise ProfileError(f"{label} overflow")
    return left + right


def scalar(raw, ceiling, label):
    # Bound decimal work before converting an untrusted field to a Python int.
    if len(raw) > len(str(ceiling)):
        raise ProfileError(f"{label} out of range")
    value = int(raw)
    if value > ceiling:
        raise ProfileError(f"{label} out of range")
    return value


def prefix_lines(lines):
    total = 0
    found = []
    for raw in lines:
        if len(raw) > MAX_LINE_BYTES:
            raise ProfileError("stderr line limit")
        total = checked_add(total, len(raw), MAX_STDERR_BYTES, "stderr bytes")
        if PREFIX in raw:
            if len(found) == 6:
                raise ProfileError("extra profile line or report block")
            found.append(raw.removesuffix(b"\n"))
    return found


def parse(lines, expect_off=False):
    report = prefix_lines(lines)
    if expect_off:
        if report:
            raise ProfileError("gate-disabled run emitted a profile")
        return {"profile_absent": True}
    if len(report) != 6:
        raise ProfileError("expected exactly six profile lines")
    if report[0] != PREFIX + b" BEGIN count=1":
        raise ProfileError("invalid BEGIN")
    if report[-1] != PREFIX + b" END count=1":
        raise ProfileError("invalid END")
    nanoseconds = {}
    counts = {}
    count_sum = 0
    for raw, (name, kind) in zip(report[1:-1], PHASES, strict=True):
        match = ROW.fullmatch(raw)
        if match is None:
            raise ProfileError("invalid PHASE syntax")
        phase, found_kind, elapsed_raw, count_raw = match.groups()
        if phase != name.encode("ascii") or found_kind != kind.encode("ascii"):
            raise ProfileError("unknown, duplicate or out-of-order phase/kind")
        elapsed = scalar(elapsed_raw, MAX_U128, "elapsed_ns")
        count = scalar(count_raw, MAX_U64, "count")
        count_sum = checked_add(count_sum, count, MAX_U64, "phase counts")
        if count != 1:
            raise ProfileError("unexpected phase count")
        if name == "app_refresh_succeeded" and elapsed != 0:
            raise ProfileError("success marker must have zero elapsed time")
        nanoseconds[name] = elapsed
        counts[name] = count
    children = checked_add(
        nanoseconds["semantic_leaf_apply_call"],
        nanoseconds["replay_leaf_apply_call"],
        MAX_U128,
        "disjoint leaf sum",
    )
    if children > nanoseconds["app_service_total"]:
        raise ProfileError("negative outer-minus-disjoint-leaves residual")
    return {
        "diagnostic_only": True,
        "nanoseconds": nanoseconds,
        "counts": counts,
        "phase_count_sum": count_sum,
        "other_service_cleanup_and_overhead_ns": nanoseconds["app_service_total"] - children,
        "instrumentation_overhead_qualified": False,
    }


def bounded_file_lines(source):
    while raw := source.readline(MAX_LINE_BYTES + 1):
        yield raw


def main():
    arguments = argparse.ArgumentParser()
    arguments.add_argument("stderr", type=Path)
    arguments.add_argument("--expect-off", action="store_true")
    args = arguments.parse_args()
    try:
        with args.stderr.open("rb") as source:
            result = parse(bounded_file_lines(source), expect_off=args.expect_off)
    except (OSError, ProfileError) as error:
        arguments.exit(1, f"profile rejected: {error}\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
