"""Pure structural fixture tests. No media tools, codecs or GPU are invoked."""

from pathlib import Path
import os
import struct
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

from mp4_boxes import InspectionError, Limits, inspect_mp4


def box(kind: bytes, payload: bytes = b"", *, extended: bool = False) -> bytes:
    if extended:
        return struct.pack(">I4sQ", 1, kind, 16 + len(payload)) + payload
    return struct.pack(">I4s", 8 + len(payload), kind) + payload


def timing(kind: bytes, timescale: int, duration: int, version: int = 0) -> bytes:
    if version == 0:
        prefix = bytes((0, 0, 0, 0)) + struct.pack(">IIII", 11, 12, timescale, duration)
    else:
        prefix = bytes((version, 0, 0, 0)) + struct.pack(">QQIQ", 11, 12, timescale, duration)
    total = {b"mvhd": (100, 112), b"mdhd": (24, 36)}[kind][version]
    return box(kind, prefix + bytes(total - len(prefix)))


def edit_list(version: int = 0) -> bytes:
    entry = struct.pack(">IiHH", 25, -1, 1, 0) if version == 0 else struct.pack(">QqHH", 25, -1, 1, 0)
    return box(b"elst", bytes((version, 0, 0, 0)) + struct.pack(">I", 1) + entry)


class BoxInspectionTests(unittest.TestCase):
    def setUp(self) -> None:
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.path = Path(self.scratch.name) / "fixture.mp4"

    def inspect(self, content: bytes, limits: Limits = Limits()) -> dict:
        self.path.write_bytes(content)
        return inspect_mp4(self.path, limits)

    def reject(self, content: bytes, message: str, limits: Limits = Limits()) -> None:
        self.path.write_bytes(content)
        with mock.patch("mp4_boxes.os.close", wraps=os.close) as close:
            with self.assertRaisesRegex(InspectionError, message):
                inspect_mp4(self.path, limits)
        close.assert_called_once()

    def test_nested_timing_and_fast_start_are_exact(self) -> None:
        movie = box(b"moov", timing(b"mvhd", 240000, 960960) + box(
            b"trak", box(b"mdia", timing(b"mdhd", 48000, 192192, 1))
        ))
        result = self.inspect(box(b"ftyp", b"isom\x00\x00\x02\x00isom") + movie + box(b"mdat", b"opaque"))
        self.assertFalse(result["has_elst"])
        self.assertFalse(result["has_edts"])
        self.assertEqual(result["fast_start"], {
            "moov_count": 1, "mdat_count": 1, "moov_before_mdat": True,
        })
        self.assertEqual([(value["type"], value["version"], value["timescale"], value["duration_ticks"])
                          for value in result["timescales"]],
                         [("mvhd", 0, 240000, 960960), ("mdhd", 1, 48000, 192192)])
        media = result["timescales"][1]
        self.assertEqual(media["path"], ["moov", "trak", "mdia", "mdhd"])
        parent = next(value for value in result["boxes"] if value["type"] == "mdia")
        self.assertEqual(media["parent_offset"], parent["offset"])

    def test_edit_list_hidden_in_track_is_detected(self) -> None:
        for version in (0, 1):
            with self.subTest(version=version):
                result = self.inspect(box(b"moov", box(b"trak", box(b"edts", edit_list(version)))) + box(b"mdat"))
                self.assertTrue(result["has_edts"])
                self.assertTrue(result["has_elst"])
                self.assertEqual(result["edit_lists"][0]["path"], ["moov", "trak", "edts", "elst"])
                self.assertEqual(result["edit_lists"][0]["entry_count"], 1)

    def test_empty_edit_container_is_distinct_from_an_edit_list(self) -> None:
        result = self.inspect(box(b"moov", box(b"trak", box(b"edts"))))
        self.assertTrue(result["has_edts"])
        self.assertFalse(result["has_elst"])
        self.assertIsNone(result["fast_start"]["moov_before_mdat"])

    def test_box_text_inside_opaque_payload_is_not_a_box(self) -> None:
        impostor = box(b"edts", edit_list())
        result = self.inspect(box(b"moov", box(b"zzzz", impostor)) + box(b"mdat", impostor))
        self.assertEqual(result["box_count"], 3)
        self.assertFalse(result["has_edts"])
        self.assertFalse(result["has_elst"])
        self.assertEqual(result["bytes_read"], 24)

    def test_media_before_movie_is_not_fast_start(self) -> None:
        result = self.inspect(box(b"mdat", b"payload") + box(b"moov"))
        self.assertFalse(result["fast_start"]["moov_before_mdat"])

    def test_later_movie_prevents_all_movies_before_media_observation(self) -> None:
        result = self.inspect(box(b"moov") + box(b"mdat") + box(b"moov"))
        self.assertEqual(result["fast_start"]["moov_count"], 2)
        self.assertFalse(result["fast_start"]["moov_before_mdat"])

    def test_extended_sizes_and_uuid_header(self) -> None:
        uuid = bytes(range(16))
        content = box(b"moov", box(b"free", b"x", extended=True), extended=True)
        content += box(b"uuid", uuid + b"opaque", extended=True)
        result = self.inspect(content)
        self.assertEqual([entry["header_size"] for entry in result["boxes"]], [16, 16, 32])
        self.assertEqual(result["boxes"][-1]["user_type"], uuid.hex())

    def test_zero_size_top_level_media_extends_to_eof(self) -> None:
        content = box(b"moov") + struct.pack(">I4s", 0, b"mdat") + b"elst edts moov"
        result = self.inspect(content)
        self.assertEqual(result["boxes"][-1]["size"], len(content) - 8)
        self.assertEqual(result["box_count"], 2)
        self.assertFalse(result["has_elst"])

    def test_zero_size_nested_box_cannot_escape_parent(self) -> None:
        content = box(b"moov", struct.pack(">I4s", 0, b"free")) + box(b"mdat")
        self.reject(content, "beyond its parent")

    def test_zero_size_nested_box_at_eof_stays_in_parent(self) -> None:
        result = self.inspect(box(b"moov", struct.pack(">I4s", 0, b"free") + b"opaque"))
        self.assertEqual(result["box_count"], 2)
        self.assertEqual(result["boxes"][1]["size"], 14)

    def test_malformed_header_sizes_fail(self) -> None:
        cases = [
            (b"", "too short"),
            (b"\x00" * 7, "too short"),
            (box(b"free") + b"tail", "truncated box header"),
            (struct.pack(">I4s", 4, b"free"), "smaller than its header"),
            (struct.pack(">I4s", 100, b"mdat"), "beyond its parent"),
            (struct.pack(">I4sI", 1, b"free", 0), "truncated extended"),
            (struct.pack(">I4sQ", 1, b"free", 8), "smaller than its header"),
            (struct.pack(">I4sQ", 1, b"free", (1 << 64) - 1), "beyond its parent"),
            (box(b"uuid", bytes(15)), "smaller than its header"),
            (box(b"moov", struct.pack(">I4s", 9, b"free")), "beyond its parent"),
            (box(b"moov", b"tail"), "truncated box header"),
        ]
        for content, message in cases:
            with self.subTest(content=content, message=message):
                self.reject(content, message)

    def test_timing_headers_validate_version_size_and_timescale(self) -> None:
        self.reject(box(b"mvhd", bytes(19)), "truncated mvhd")
        self.reject(box(b"mdhd", b"\x02\x00\x00\x00" + bytes(40)), "unsupported mdhd version")
        self.reject(timing(b"mdhd", 0, 10), "zero mdhd timescale")
        self.reject(box(b"mvhd", b"\x00\x00\x00\x01" + bytes(96)), "unsupported mvhd flags")
        for version, duration in ((0, (1 << 32) - 1), (1, (1 << 64) - 1)):
            with self.subTest(version=version):
                value = self.inspect(timing(b"mdhd", 48000, duration, version))["timescales"][0]
                self.assertTrue(value["duration_unknown"])
                self.assertIsNone(value["duration_ticks"])

    def test_movie_version_one_preserves_a_wide_known_duration(self) -> None:
        duration = (1 << 40) + 123
        result = self.inspect(box(b"moov", timing(b"mvhd", 240000, duration, 1)))
        movie = result["timescales"][0]
        self.assertEqual(movie["version"], 1)
        self.assertEqual(movie["timescale"], 240000)
        self.assertEqual(movie["duration_ticks"], duration)
        self.assertFalse(movie["duration_unknown"])

    def test_truncated_version_one_timing_tails_fail_and_close(self) -> None:
        for kind in (b"mvhd", b"mdhd"):
            with self.subTest(kind=kind):
                complete = timing(kind, 48000, (1 << 40) + 1, 1)
                # Keep the outer extent truthful so rejection proves that the
                # required version-one tail was checked, not just box bounds.
                truncated = box(kind, complete[8:-1])
                self.reject(truncated, f"truncated {kind.decode('ascii')} version 1")

    def test_edit_list_entry_extent_is_checked_without_entry_allocation(self) -> None:
        self.reject(box(b"elst"), "truncated elst full-box")
        self.reject(box(b"elst", bytes(4)), "truncated elst entry count")
        self.reject(box(b"elst", bytes(4) + struct.pack(">I", (1 << 32) - 1)), "entry extent")
        self.reject(box(b"elst", bytes(8) + b"extra"), "entry extent")
        result = self.inspect(box(b"elst", bytes(8)))
        self.assertTrue(result["has_elst"])
        self.assertEqual(result["edit_lists"][0]["entry_count"], 0)

    def test_iso_meta_prefix_is_not_mistaken_for_a_child_header(self) -> None:
        result = self.inspect(box(b"moov", box(b"meta", bytes(4) + box(b"free"))))
        self.assertEqual(result["boxes"][-1]["path"], ["moov", "meta", "free"])
        self.reject(box(b"meta", b"\x01\x00\x00\x00"), "unsupported meta version")
        self.reject(box(b"meta", bytes(3)), "truncated meta full-box")

    def test_box_count_depth_and_read_limits_are_enforced(self) -> None:
        self.reject(box(b"free") * 2, "box count limit", Limits(maximum_boxes=1))
        content = box(b"free")
        for _ in range(4):
            content = box(b"moov", content)
        self.reject(content, "depth limit", Limits(maximum_depth=4))
        self.reject(content, "box count limit", Limits(maximum_boxes=4))
        result = self.inspect(content, Limits(maximum_depth=5))
        self.assertEqual(result["box_count"], 5)
        self.reject(box(b"free") * 2, "read budget", Limits(maximum_read_bytes=15))
        self.assertEqual(self.inspect(box(b"free") * 2, Limits(maximum_read_bytes=16))["bytes_read"], 16)
        self.reject(box(b"free"), "file-size limit", Limits(maximum_file_bytes=7))

    def test_large_media_payload_is_skipped(self) -> None:
        media_size = 16 * 1024 * 1024
        with self.path.open("wb") as stream:
            stream.write(struct.pack(">I4s", media_size, b"mdat"))
            stream.seek(media_size - 1)
            stream.write(b"\x00")
            stream.write(box(b"moov"))
        result = inspect_mp4(self.path, Limits(maximum_read_bytes=16))
        self.assertEqual(result["bytes_read"], 16)
        self.assertEqual(result["file_size"], media_size + 8)
        self.assertEqual(result["box_count"], 2)

    def test_limits_reject_invalid_or_unbounded_values(self) -> None:
        for values in ({"maximum_depth": 0}, {"maximum_depth": 65},
                       {"maximum_boxes": True}, {"maximum_boxes": 65537},
                       {"maximum_read_bytes": -1}, {"maximum_file_bytes": 1 << 64}):
            with self.subTest(values=values), self.assertRaises(InspectionError):
                Limits(**values)

    def test_nonregular_and_missing_inputs_fail_explicitly(self) -> None:
        with self.assertRaises(InspectionError):
            inspect_mp4(self.path)
        with self.assertRaises(InspectionError):
            inspect_mp4(self.scratch.name)

    def test_descriptor_is_closed_once_after_success_and_parse_failure(self) -> None:
        for content, valid in ((box(b"free"), True), (box(b"moov", b"tail"), False)):
            with self.subTest(valid=valid):
                self.path.write_bytes(content)
                with mock.patch("mp4_boxes.os.close", wraps=os.close) as close:
                    if valid:
                        inspect_mp4(self.path)
                    else:
                        with self.assertRaises(InspectionError):
                            inspect_mp4(self.path)
                close.assert_called_once()

    def test_short_header_and_timing_reads_fail_and_close(self) -> None:
        original_read = os.read
        for short_at in (1, 3):
            with self.subTest(short_read_number=short_at):
                self.path.write_bytes(timing(b"mvhd", 240000, (1 << 40) + 123, 1))
                calls = 0

                def short_read(descriptor: int, count: int) -> bytes:
                    nonlocal calls
                    calls += 1
                    data = original_read(descriptor, count)
                    return data[:-1] if calls == short_at else data

                with mock.patch("mp4_boxes.os.read", side_effect=short_read) as read:
                    with mock.patch("mp4_boxes.os.close", wraps=os.close) as close:
                        with self.assertRaisesRegex(InspectionError, "truncated read at byte"):
                            inspect_mp4(self.path)
                self.assertEqual(read.call_count, short_at)
                close.assert_called_once()

    def test_changed_final_stat_fails_and_closes(self) -> None:
        self.path.write_bytes(box(b"free"))
        original = self.path.stat()
        changed = SimpleNamespace(
            st_dev=original.st_dev, st_ino=original.st_ino,
            st_size=original.st_size, st_mtime_ns=original.st_mtime_ns + 1,
            st_ctime_ns=original.st_ctime_ns,
        )
        with mock.patch("mp4_boxes.os.fstat", side_effect=[original, changed]) as metadata:
            with mock.patch("mp4_boxes.os.close", wraps=os.close) as close:
                with self.assertRaisesRegex(InspectionError, "input changed during inspection"):
                    inspect_mp4(self.path)
        close.assert_called_once()
        descriptor = close.call_args.args[0]
        self.assertEqual(metadata.call_args_list, [mock.call(descriptor), mock.call(descriptor)])


if __name__ == "__main__":
    unittest.main()
