"""Draft synthetic cases. This file has not been executed."""

import itertools
import unittest

import check_profile as parser


def valid(outer=100, semantic=30, replay=40):
    values = [outer, semantic, replay, 0]
    return [parser.PREFIX + b" BEGIN count=1\n"] + [
        parser.PREFIX
        + f" PHASE phase={name} kind={kind} elapsed_ns={elapsed} count=1\n".encode("ascii")
        for (name, kind), elapsed in zip(parser.PHASES, values, strict=True)
    ] + [parser.PREFIX + b" END count=1\n"]


class CurrentLeafParser(unittest.TestCase):
    def reject(self, lines):
        with self.assertRaises(parser.ProfileError):
            parser.parse(lines)

    def test_exact_success_and_checked_residual(self):
        result = parser.parse(valid())
        self.assertEqual(result["other_service_cleanup_and_overhead_ns"], 30)
        self.assertEqual(result["phase_count_sum"], 4)
        self.assertEqual(result["counts"], {name: 1 for name, _ in parser.PHASES})

    def test_nonprofile_stderr_can_surround_the_block(self):
        self.assertEqual(parser.parse([b"other log\n"] + valid() + [b"done\n"]), parser.parse(valid()))

    def test_gate_disabled_pass(self):
        self.assertEqual(parser.parse([b"ordinary stderr\n"], expect_off=True), {"profile_absent": True})
        with self.assertRaises(parser.ProfileError):
            parser.parse(valid(), expect_off=True)

    def test_missing_duplicate_extra_and_out_of_order(self):
        for lines in [valid()[:-1], valid()[1:], valid() + valid(), valid() + [valid()[1]], [], valid()[:2] + valid()[3:]]:
            self.reject(lines)
        lines = valid()
        lines[2] = lines[1]
        self.reject(lines)
        lines = valid()
        lines[1], lines[2] = lines[2], lines[1]
        self.reject(lines)

    def test_unknown_phase_and_wrong_kind(self):
        for old, new in [(b"semantic_leaf_apply_call", b"unknown"), (b"disjoint_leaf_call_in_semantics", b"inclusive_native_route")]:
            lines = valid()
            lines[2] = lines[2].replace(old, new)
            self.reject(lines)

    def test_counts_marker_and_header_must_be_exact(self):
        for index in range(6):
            for count in [b"0", b"2", str(parser.MAX_U64 + 1).encode("ascii")]:
                lines = valid()
                lines[index] = lines[index].replace(b"count=1", b"count=" + count)
                self.reject(lines)
        lines = valid()
        lines[4] = lines[4].replace(b"elapsed_ns=0", b"elapsed_ns=1")
        self.reject(lines)

    def test_containment_and_u128_overflow(self):
        self.reject(valid(outer=69))
        self.reject(valid(outer=parser.MAX_U128 + 1))
        self.reject(valid(outer=parser.MAX_U128, semantic=parser.MAX_U128, replay=1))
        self.assertEqual(parser.parse(valid(outer=parser.MAX_U128, semantic=parser.MAX_U128, replay=0))["other_service_cleanup_and_overhead_ns"], 0)

    def test_numeric_and_line_syntax(self):
        for elapsed in [b"-1", b"+1", b"1.0", b"1e3", b"", b"01", b"9" * 1000]:
            lines = valid()
            lines[1] = lines[1].replace(b"elapsed_ns=100", b"elapsed_ns=" + elapsed)
            self.reject(lines)
        for suffix in [b" trailing", b"\r", b"\x00"]:
            lines = valid()
            lines[1] = lines[1].removesuffix(b"\n") + suffix + b"\n"
            self.reject(lines)
        self.reject([b"prefix " + row for row in valid()])
        self.reject(valid() + [parser.PREFIX + b" nonsense\n"])
        self.reject([b"x" * (parser.MAX_LINE_BYTES + 1)])

    def test_checked_scalar_helpers(self):
        with self.assertRaises(parser.ProfileError):
            parser.checked_add(parser.MAX_U64, 1, parser.MAX_U64, "counts")
        with self.assertRaises(parser.ProfileError):
            parser.scalar(str(parser.MAX_U128 + 1).encode("ascii"), parser.MAX_U128, "elapsed")

    def test_total_stderr_work_is_bounded_without_a_large_allocation(self):
        chunk = b"x" * parser.MAX_LINE_BYTES
        self.reject(itertools.repeat(chunk, parser.MAX_STDERR_BYTES // len(chunk) + 1))


if __name__ == "__main__":
    unittest.main()
