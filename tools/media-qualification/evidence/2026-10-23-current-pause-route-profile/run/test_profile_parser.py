from __future__ import annotations

import unittest

from check_profile import MAX_U64, PHASES, PREFIX, ProfileError, parse_profile


def make_block(values=None, *, end=None, begin="count=1", phase_count="count=1",
               end_fields="count=1 elapsed_ns={elapsed} committed=1 refreshed=1 published=1",
               phase_names=None):
    values = list(values if values is not None else [1] * len(PHASES))
    names = list(phase_names if phase_names is not None else PHASES)
    total = sum(values) if end is None else end
    rows = [f"{PREFIX} BEGIN {begin}"]
    rows.extend(
        f"{PREFIX} PHASE name={name} elapsed_ns={value} {phase_count}"
        for name, value in zip(names, values)
    )
    rows.append(f"{PREFIX} END {end_fields.format(elapsed=total)}")
    return "\n".join(rows)


class ProfileParserTests(unittest.TestCase):
    def test_accepts_complete_block_and_preserves_phase_order(self):
        parsed = parse_profile("worker: ready\n" + make_block() + "\nworker: done")
        self.assertEqual(parsed["block_count"], 1)
        self.assertEqual(parsed["selected_block_index"], 0)
        self.assertEqual([row["name"] for row in parsed["selected"]["phases"]], list(PHASES))
        self.assertEqual(parsed["selected"]["phase_sum_ns"], 13)

    def test_accepts_zero_durations_and_decimal_leading_zeroes(self):
        parsed = parse_profile(make_block(values=[0] * len(PHASES), end=0))
        self.assertEqual(parsed["selected"]["elapsed_ns"], 0)
        rows = make_block(values=[0] * len(PHASES), end=0).splitlines()
        rows[1] = rows[1].replace("elapsed_ns=0", "elapsed_ns=000")
        parsed = parse_profile("\n".join(rows))
        self.assertEqual(parsed["selected"]["phases"][0]["elapsed_ns"], 0)

    def test_accepts_unrelated_stderr_between_trace_rows(self):
        block = make_block().splitlines()
        parsed = parse_profile("\n".join([block[0], "debug: unrelated", *block[1:]]))
        self.assertEqual(parsed["block_count"], 1)

    def test_accepts_multiple_blocks_and_selects_first_in_stream_order(self):
        first = make_block(values=[0] * len(PHASES), end=0)
        second = make_block(values=[2] * len(PHASES), end=26)
        parsed = parse_profile(second + "\nnoise\n" + first)
        self.assertEqual(parsed["block_count"], 2)
        self.assertEqual(parsed["selected_block_index"], 0)
        self.assertEqual(parsed["selected"]["elapsed_ns"], 26)

    def test_rejects_no_block(self):
        with self.assertRaises(ProfileError):
            parse_profile("ordinary stderr only")

    def test_rejects_wrong_or_malformed_prefix(self):
        block = make_block().replace(PREFIX, "DEADPAN_PAUSE_ROUTE_PROFILE_V2", 1)
        with self.assertRaises(ProfileError):
            parse_profile(block)
        with self.assertRaises(ProfileError):
            parse_profile("DEADPAN_PAUSE_ROUTE_PROFILE_V1 BEGIN count=1 extra=1")

    def test_rejects_malformed_begin_or_end(self):
        for bad in (
            make_block().replace("BEGIN count=1", "BEGIN count=2", 1),
            make_block().replace("END count=1", "END count=2", 1),
            make_block().replace("committed=1", "committed=0"),
            make_block().replace("refreshed=1", "refreshed=0"),
            make_block().replace("published=1", "published=0"),
        ):
            with self.subTest(bad=bad[:80]), self.assertRaises(ProfileError):
                parse_profile(bad)

    def test_rejects_missing_or_truncated_phase(self):
        rows = make_block().splitlines()
        for cut in (rows[:1], rows[:-1], rows[:7] + rows[-1:]):
            with self.subTest(length=len(cut)), self.assertRaises(ProfileError):
                parse_profile("\n".join(cut))

    def test_rejects_duplicate_or_extra_phase(self):
        rows = make_block().splitlines()
        duplicate = rows[:3] + [rows[2]] + rows[3:]
        extra = rows[:-1] + [rows[-2], rows[-1]]
        for candidate in (duplicate, extra):
            with self.subTest(candidate=len(candidate)), self.assertRaises(ProfileError):
                parse_profile("\n".join(candidate))

    def test_rejects_out_of_order_or_unknown_phase(self):
        names = list(PHASES)
        names[2], names[3] = names[3], names[2]
        with self.assertRaises(ProfileError):
            parse_profile(make_block(phase_names=names))
        rows = make_block().splitlines()
        rows[1] = rows[1].replace(PHASES[0], "invented_phase")
        with self.assertRaises(ProfileError):
            parse_profile("\n".join(rows))

    def test_rejects_nondecimal_or_malformed_phase_fields(self):
        for bad_value in ("-1", "+1", "1.0", "1e2", "", " 1"):
            rows = make_block().splitlines()
            rows[1] = f"{PREFIX} PHASE name={PHASES[0]} elapsed_ns={bad_value} count=1"
            with self.subTest(value=bad_value), self.assertRaises(ProfileError):
                parse_profile("\n".join(rows))
        rows = make_block().splitlines()
        rows[1] = f"{PREFIX} PHASE elapsed_ns=1 name={PHASES[0]} count=1"
        with self.assertRaises(ProfileError):
            parse_profile("\n".join(rows))

    def test_rejects_wrong_phase_count(self):
        rows = make_block().splitlines()
        rows[1] = rows[1].replace("count=1", "count=2")
        with self.assertRaises(ProfileError):
            parse_profile("\n".join(rows))

    def test_rejects_u64_overflow_in_phase_and_end(self):
        too_large = str(MAX_U64 + 1)
        rows = make_block().splitlines()
        rows[1] = rows[1].replace("elapsed_ns=1", f"elapsed_ns={too_large}")
        with self.assertRaises(ProfileError):
            parse_profile("\n".join(rows))
        rows = make_block().splitlines()
        rows[-1] = rows[-1].replace("elapsed_ns=13", f"elapsed_ns={too_large}")
        with self.assertRaises(ProfileError):
            parse_profile("\n".join(rows))

    def test_rejects_phase_sum_overflow(self):
        rows = make_block(values=[MAX_U64] + [1] * (len(PHASES) - 1), end=0).splitlines()
        with self.assertRaises(ProfileError):
            parse_profile("\n".join(rows))

    def test_rejects_conservation_mismatch(self):
        rows = make_block().splitlines()
        rows[-1] = rows[-1].replace("elapsed_ns=13", "elapsed_ns=14")
        with self.assertRaises(ProfileError):
            parse_profile("\n".join(rows))

    def test_rejects_nested_or_duplicate_begin(self):
        rows = make_block().splitlines()
        with self.assertRaises(ProfileError):
            parse_profile("\n".join([rows[0], rows[0], *rows[1:]]))

    def test_rejects_unmatched_end_and_extra_trace_line(self):
        with self.assertRaises(ProfileError):
            parse_profile(f"{PREFIX} END count=1 elapsed_ns=0 committed=1 refreshed=1 published=1")
        rows = make_block().splitlines()
        with self.assertRaises(ProfileError):
            parse_profile("\n".join([*rows, f"{PREFIX} PHASE name={PHASES[0]} elapsed_ns=0 count=1"]))

    def test_rejects_extra_nonprefixed_event_line(self):
        rows = make_block().splitlines()
        rows.insert(2, "PHASE name=native_admission elapsed_ns=0 count=1")
        with self.assertRaises(ProfileError):
            parse_profile("\n".join(rows))


if __name__ == "__main__":
    unittest.main()
