"""Bounded structural MP4 observations for a developer encoder experiment.

This is not a general media validator. It traverses known box containers and
does not interpret sample entries, compressed data, or arbitrary unknown payloads.
All offsets refer to the opened file. No payload is searched for box-like text.
"""

from __future__ import annotations

from dataclasses import dataclass
import os
from pathlib import Path
import stat


class InspectionError(ValueError):
    """Malformed input, an unsupported traversed header, or a resource limit."""


@dataclass(frozen=True)
class Limits:
    maximum_depth: int = 16
    maximum_boxes: int = 4096
    maximum_edit_entries: int = 4096
    maximum_read_bytes: int = 1024 * 1024
    maximum_file_bytes: int = 1024**4

    def __post_init__(self) -> None:
        ceilings = {
            "maximum_depth": 64,
            "maximum_boxes": 65536,
            "maximum_edit_entries": 65536,
            "maximum_read_bytes": 16 * 1024 * 1024,
            "maximum_file_bytes": (1 << 63) - 1,
        }
        for name, ceiling in ceilings.items():
            value = getattr(self, name)
            if type(value) is not int or not 1 <= value <= ceiling:
                raise InspectionError(f"{name} must be an integer in 1..{ceiling}")


# These payloads are sequences of ordinary box headers, without a prefix.
# In particular, stsd/dref and their sample entries are NOT in this set.
_CONTAINERS = frozenset(
    (b"moov", b"trak", b"mdia", b"minf", b"stbl", b"edts", b"dinf",
     b"udta", b"mvex", b"moof", b"traf", b"mfra", b"tref", b"sinf", b"schi")
)


class _Inspector:
    def __init__(self, descriptor: int, file_size: int, limits: Limits):
        self.descriptor = descriptor
        self.file_size = file_size
        self.limits = limits
        self.bytes_read = 0
        self.boxes: list[dict] = []
        self.edit_containers: list[dict] = []
        self.edit_lists: list[dict] = []
        self.edit_entry_count = 0
        self.handlers: list[dict] = []
        self.timescales: list[dict] = []
        self.moov: list[dict] = []
        self.mdat: list[dict] = []

    def read(self, offset: int, count: int) -> bytes:
        if offset < 0 or count < 0 or offset > self.file_size - count:
            raise InspectionError(f"truncated read at byte {offset}")
        if count > self.limits.maximum_read_bytes - self.bytes_read:
            raise InspectionError("box-header read budget exceeded")
        os.lseek(self.descriptor, offset, os.SEEK_SET)
        value = os.read(self.descriptor, count)
        self.bytes_read += len(value)
        if len(value) != count:
            raise InspectionError(f"truncated read at byte {offset}")
        return value

    def full_header(self, start: int, end: int, box_type: str) -> tuple[int, int]:
        if end - start < 4:
            raise InspectionError(f"truncated {box_type} full-box header at byte {start}")
        header = self.read(start, 4)
        version = header[0]
        flags = int.from_bytes(header[1:], "big")
        if flags != 0:
            raise InspectionError(f"unsupported {box_type} flags at byte {start}")
        return version, start + 4

    def timing(self, record: dict, start: int, end: int) -> None:
        name = record["type"]
        version, _ = self.full_header(start, end, name)
        if version not in (0, 1):
            raise InspectionError(f"unsupported {name} version {version} at byte {start}")
        minimum = {"mvhd": (100, 112), "mdhd": (24, 36)}[name][version]
        if end - start < minimum:
            raise InspectionError(f"truncated {name} version {version} at byte {start}")
        # Skip creation/modification times. Read only timescale and duration.
        position = start + (12 if version == 0 else 20)
        width = 4 if version == 0 else 8
        fields = self.read(position, 4 + width)
        timescale = int.from_bytes(fields[:4], "big")
        if timescale == 0:
            raise InspectionError(f"zero {name} timescale at byte {position}")
        duration = int.from_bytes(fields[4:], "big")
        unknown = duration == (1 << (8 * width)) - 1
        self.timescales.append({
            **record,
            "version": version,
            "timescale": timescale,
            "duration_ticks": None if unknown else duration,
            "duration_unknown": unknown,
        })

    def edit_list(self, record: dict, start: int, end: int) -> None:
        version, position = self.full_header(start, end, "elst")
        if version not in (0, 1):
            raise InspectionError(f"unsupported elst version {version} at byte {start}")
        if end - position < 4:
            raise InspectionError(f"truncated elst entry count at byte {position}")
        count = int.from_bytes(self.read(position, 4), "big")
        entry_size = 12 if version == 0 else 20
        # Check extent and aggregate count before reading or allocating entries.
        # The same cumulative read budget covers headers and edit metadata.
        if count * entry_size != end - position - 4:
            raise InspectionError(f"elst entry extent disagrees with box size at byte {start}")
        if count > self.limits.maximum_edit_entries - self.edit_entry_count:
            raise InspectionError("aggregate edit-list entry limit exceeded")
        data = self.read(position + 4, count * entry_size)
        width = 4 if version == 0 else 8
        entries = []
        for offset in range(0, len(data), entry_size):
            entry = data[offset:offset + entry_size]
            entries.append({
                "segment_duration": int.from_bytes(entry[:width], "big"),
                "media_time": int.from_bytes(entry[width:width * 2], "big", signed=True),
                # Keep the signed 16.16 fixed-point representation exact.
                "media_rate_integer": int.from_bytes(entry[-4:-2], "big", signed=True),
                "media_rate_fraction": int.from_bytes(entry[-2:], "big"),
            })
        self.edit_entry_count += count
        self.edit_lists.append({**record, "version": version, "entry_count": count, "entries": entries})

    def handler(self, record: dict, start: int, end: int) -> None:
        version, _ = self.full_header(start, end, "hdlr")
        if version != 0:
            raise InspectionError(f"unsupported hdlr version {version} at byte {start}")
        if end - start < 24:
            raise InspectionError(f"truncated hdlr header at byte {start}")
        # FullBox, predefined, then the four-byte handler type. Never use track
        # order to infer whether an edit belongs to audio or video.
        kind = self.read(start + 8, 4).decode("latin-1")
        self.handlers.append({**record, "handler_type": kind})

    def walk(
        self, start: int, end: int, path: tuple[str, ...] = (),
        parent_offset: int | None = None, depth: int = 1,
    ) -> None:
        offset = start
        while offset < end:
            if depth > self.limits.maximum_depth:
                raise InspectionError(f"box nesting exceeds depth limit at byte {offset}")
            if len(self.boxes) >= self.limits.maximum_boxes:
                raise InspectionError(f"box count limit exceeded at byte {offset}")
            if end - offset < 8:
                raise InspectionError(f"truncated box header at byte {offset}")
            header = self.read(offset, 8)
            size = int.from_bytes(header[:4], "big")
            raw_type = header[4:]
            name = raw_type.decode("latin-1")
            header_size = 8
            if size == 1:
                if end - offset < 16:
                    raise InspectionError(f"truncated extended box header at byte {offset}")
                size = int.from_bytes(self.read(offset + 8, 8), "big")
                header_size = 16
            elif size == 0:
                # ISO box size zero means EOF, not an arbitrary enclosing end.
                # A nested zero-sized box still has to fit within its parent.
                size = self.file_size - offset
            if raw_type == b"uuid":
                header_size += 16
            if size < header_size:
                raise InspectionError(f"box size is smaller than its header at byte {offset}")
            if size > end - offset:
                raise InspectionError(f"box extends beyond its parent at byte {offset}")
            box_end = offset + size
            record = {
                "type": name, "offset": offset, "size": size,
                "header_size": header_size, "parent_offset": parent_offset,
                "path": [*path, name],
            }
            if raw_type == b"uuid":
                record["user_type"] = self.read(offset + header_size - 16, 16).hex()
            self.boxes.append(record)
            if parent_offset is None:
                if raw_type == b"moov":
                    self.moov.append(record)
                elif raw_type == b"mdat":
                    self.mdat.append(record)
            payload = offset + header_size
            if raw_type == b"edts":
                self.edit_containers.append(record)
            if raw_type == b"elst":
                self.edit_list(record, payload, box_end)
            elif raw_type in (b"mvhd", b"mdhd"):
                self.timing(record, payload, box_end)
            elif raw_type == b"hdlr":
                self.handler(record, payload, box_end)
            elif raw_type == b"meta":
                # Support ISO FullBox meta only, not the ambiguous historical
                # QuickTime non-FullBox variant. Unsupported input fails closed.
                version, children = self.full_header(payload, box_end, "meta")
                if version != 0:
                    raise InspectionError(f"unsupported meta version {version} at byte {payload}")
                self.walk(children, box_end, (*path, name), offset, depth + 1)
            elif raw_type in _CONTAINERS:
                self.walk(payload, box_end, (*path, name), offset, depth + 1)
            offset = box_end

    def report(self) -> dict:
        before = None
        if self.moov and self.mdat:
            before = max(box["offset"] + box["size"] for box in self.moov) <= min(
                box["offset"] for box in self.mdat
            )
        return {
            "schema_version": 1,
            "file_size": self.file_size,
            "bytes_read": self.bytes_read,
            "box_count": len(self.boxes),
            "boxes": self.boxes,
            "has_edts": bool(self.edit_containers),
            "has_elst": bool(self.edit_lists),
            "edit_containers": self.edit_containers,
            "edit_lists": self.edit_lists,
            "edit_entry_count": self.edit_entry_count,
            "handlers": self.handlers,
            "timescales": self.timescales,
            "fast_start": {
                "moov_count": len(self.moov),
                "mdat_count": len(self.mdat),
                "moov_before_mdat": before,
            },
        }


def _identity(info: os.stat_result) -> tuple[int, int, int, int, int]:
    return info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns, info.st_ctime_ns


def inspect_mp4(path: str | os.PathLike[str], limits: Limits = Limits()) -> dict:
    """Return bounded JSON-ready structural observations or raise InspectionError.

    Only regular files are admitted. Large mdat and unknown payloads are skipped
    using seek. Output size is bounded by the box/depth/edit-entry caps; compressed
    media never enters the report. Fast-start is an ordering observation, not file validation.
    The caller must separately require its needed boxes, tracks and semantics.
    """
    if not isinstance(limits, Limits):
        raise InspectionError("limits must be a Limits value")
    descriptor: int | None = None
    try:
        # Avoid waiting to open a FIFO before learning that it is not regular.
        flags = os.O_RDONLY | getattr(os, "O_NONBLOCK", 0) | getattr(os, "O_CLOEXEC", 0)
        descriptor = os.open(Path(path), flags)
        before = os.fstat(descriptor)
        if not stat.S_ISREG(before.st_mode):
            raise InspectionError("input must be a regular file")
        if before.st_size < 8:
            raise InspectionError("input is too short for a box header")
        if before.st_size > limits.maximum_file_bytes:
            raise InspectionError("input exceeds file-size limit")
        inspector = _Inspector(descriptor, before.st_size, limits)
        inspector.walk(0, before.st_size)
        if _identity(before) != _identity(os.fstat(descriptor)):
            raise InspectionError("input changed during inspection")
        return inspector.report()
    except OSError as error:
        raise InspectionError(f"cannot inspect input: {error.strerror or type(error).__name__}") from error
    finally:
        if descriptor is not None:
            os.close(descriptor)
