"""The small framed protocol used by a local model qualification worker.

This module deliberately has no model/runtime dependencies.  A worker should
keep its protocol file descriptors separate from backend logging: use
``WorkerProtocol.from_fds`` for the binary descriptors and send diagnostics to
stderr.  The protocol is a transport and validation boundary, not proof that a
candidate's media is usable; the Rust host validates the output independently.
"""

from __future__ import annotations

from dataclasses import dataclass
from fractions import Fraction
import json
import math
import os
import struct
import threading
from typing import Any, BinaryIO, Mapping


PROTOCOL_VERSION = 1
BRIDGE_PROTOCOL_VERSION = 2
MAX_FRAME_BYTES = 256 * 1024
MAX_PROTOCOL_ID_BYTES = 128
MAX_WORKSPACE_REF_BYTES = 1_024
MAX_DIAGNOSTIC_BYTES = 4_096
MAX_VIDEO_DIMENSION = 32_768
_MAX_U32 = 2**32 - 1
_MAX_U64 = 2**64 - 1
_MAX_I64 = 2**63 - 1


class ProtocolError(ValueError):
    """A malformed, unexpected, or invalid protocol message."""


class _DuplicateKey(ProtocolError):
    pass


def _reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise _DuplicateKey(f"duplicate JSON object key: {key!r}")
        result[key] = value
    return result


def _reject_constant(value: str) -> Any:
    raise ProtocolError(f"non-finite JSON number is not allowed: {value}")


def _read_exact(stream: BinaryIO, length: int) -> bytes:
    # Allocate exactly the already-bounded length once.  Keeping one object
    # per fragmented read lets a peer turn the 256 KiB frame budget into a
    # large object-count allocation with a one-byte-at-a-time stream.
    buffer = bytearray(length)
    offset = 0
    while offset < length:
        try:
            chunk = stream.read(length - offset)
        except InterruptedError:
            continue
        if not chunk:
            break
        if not isinstance(chunk, (bytes, bytearray, memoryview)):
            raise ProtocolError("protocol reader returned non-bytes")
        chunk = bytes(chunk)
        if len(chunk) > length - offset:
            raise ProtocolError("protocol reader returned more bytes than requested")
        buffer[offset : offset + len(chunk)] = chunk
        offset += len(chunk)
    return bytes(buffer[:offset])


def read_frame(stream: BinaryIO) -> dict[str, Any] | None:
    """Read one bounded big-endian length-prefixed JSON object.

    ``None`` means clean EOF before a new frame.  A truncated header or body
    is always an error.  The body is allocated only after its declared length
    passes the frame bound.
    """

    header = _read_exact(stream, 4)
    if not header:
        return None
    if len(header) != 4:
        raise ProtocolError(f"truncated frame header: read {len(header)} of 4 bytes")
    (declared,) = struct.unpack(">I", header)
    if declared > MAX_FRAME_BYTES:
        raise ProtocolError(
            f"frame declares {declared} bytes; maximum is {MAX_FRAME_BYTES}"
        )
    payload = _read_exact(stream, declared)
    if len(payload) != declared:
        raise ProtocolError(
            f"truncated frame body: read {len(payload)} of {declared} bytes"
        )
    try:
        value = json.loads(
            payload.decode("utf-8"),
            object_pairs_hook=_reject_duplicate_keys,
            parse_constant=_reject_constant,
        )
    except ProtocolError:
        raise
    except (UnicodeDecodeError, ValueError, RecursionError) as error:
        raise ProtocolError(f"malformed JSON frame: {error}") from error
    if not isinstance(value, dict):
        raise ProtocolError("protocol frame must contain a JSON object")
    return value


def _write_all(stream: BinaryIO, data: bytes) -> None:
    view = memoryview(data)
    written = 0
    while written < len(view):
        try:
            count = stream.write(view[written:])
        except InterruptedError:
            continue
        if count is None:
            # Buffered streams may legally return None only in non-blocking
            # mode.  Treat it as a failed protocol write instead of looping.
            raise ProtocolError("protocol writer returned no byte count")
        if count <= 0:
            raise ProtocolError("protocol writer made no progress")
        written += count


def write_frame(stream: BinaryIO, value: Mapping[str, Any]) -> None:
    """Serialize and write one compact UTF-8 JSON frame."""

    try:
        encoder = json.JSONEncoder(
            ensure_ascii=False,
            separators=(",", ":"),
            allow_nan=False,
        )
        payload_buffer = bytearray()
        for chunk in encoder.iterencode(value):
            # UTF-8 is never shorter than its source text.  Reject a giant
            # encoder chunk before encoding it, which keeps an untrusted large
            # string from forcing an unbounded temporary allocation.
            if len(payload_buffer) + len(chunk) > MAX_FRAME_BYTES:
                raise ProtocolError(
                    f"serialized frame exceeds {MAX_FRAME_BYTES} bytes"
                )
            encoded = chunk.encode("utf-8")
            if len(payload_buffer) + len(encoded) > MAX_FRAME_BYTES:
                raise ProtocolError(
                    f"serialized frame exceeds {MAX_FRAME_BYTES} bytes"
                )
            payload_buffer.extend(encoded)
        payload = bytes(payload_buffer)
    except ProtocolError:
        raise
    except (TypeError, ValueError, UnicodeEncodeError, RecursionError) as error:
        raise ProtocolError(f"cannot serialize protocol frame: {error}") from error
    _write_all(stream, struct.pack(">I", len(payload)))
    _write_all(stream, payload)
    flush = getattr(stream, "flush", None)
    if flush is not None:
        try:
            flush()
        except OSError as error:
            raise ProtocolError(f"protocol flush failed: {error}") from error


def _object(value: Any, keys: set[str], where: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ProtocolError(f"{where} must be an object")
    actual = set(value)
    missing = keys - actual
    extra = actual - keys
    if missing:
        raise ProtocolError(f"{where} is missing field(s): {sorted(missing)}")
    if extra:
        raise ProtocolError(f"{where} has unknown field(s): {sorted(extra)}")
    return value


def _string(value: Any, where: str) -> str:
    if type(value) is not str:
        raise ProtocolError(f"{where} must be a string")
    try:
        value.encode("utf-8")
    except UnicodeEncodeError as error:
        raise ProtocolError(f"{where} is not valid UTF-8") from error
    return value


def _integer(value: Any, where: str, *, minimum: int = 0, maximum: int) -> int:
    # ``bool`` is an ``int`` subclass, but serde_json does not deserialize a
    # JSON boolean into a Rust integer.
    if type(value) is not int or not minimum <= value <= maximum:
        raise ProtocolError(f"{where} must be an integer in [{minimum}, {maximum}]")
    return value


def _identifier(value: Any, where: str, *, punctuation: str) -> str:
    value = _string(value, where)
    encoded = value.encode("utf-8")
    if not 1 <= len(encoded) <= MAX_PROTOCOL_ID_BYTES or any(
        not (char.isascii() and (char.isalnum() or char in punctuation))
        for char in value
    ):
        raise ProtocolError(f"invalid {where}")
    return value


def _protocol_identifier(value: Any, where: str) -> str:
    return _identifier(value, where, punctuation="-_.+")


def _core_identifier(value: Any, where: str) -> str:
    return _identifier(value, where, punctuation="-_")


def _workspace_ref(value: Any, where: str = "workspace reference") -> str:
    value = _string(value, where)
    if (
        not value
        or len(value.encode("utf-8")) > MAX_WORKSPACE_REF_BYTES
        or value.startswith("/")
        or "\x00" in value
        or "\\" in value
    ):
        raise ProtocolError(f"invalid {where}")
    components = value.split("/")
    if any(not component or component in {".", ".."} for component in components):
        raise ProtocolError(f"invalid {where}")
    return value


def _sha256(value: Any, where: str = "SHA-256") -> str:
    value = _string(value, where)
    if len(value) != 64 or any(char not in "0123456789abcdef" for char in value):
        raise ProtocolError(f"invalid {where}")
    return value


def _diagnostic(value: Any, where: str = "diagnostic") -> str:
    value = _string(value, where)
    if not value or len(value.encode("utf-8")) > MAX_DIAGNOSTIC_BYTES or "\x00" in value:
        raise ProtocolError(f"invalid {where}")
    return value


@dataclass(frozen=True, slots=True)
class MessageIdentity:
    request_id: str
    attempt_id: str

    def __post_init__(self) -> None:
        _protocol_identifier(self.request_id, "request ID")
        _protocol_identifier(self.attempt_id, "attempt ID")

    def _wire(self) -> dict[str, str]:
        return {"request_id": self.request_id, "attempt_id": self.attempt_id}


@dataclass(frozen=True, slots=True)
class FrameRate:
    numerator: int
    denominator: int

    def __post_init__(self) -> None:
        numerator = _integer(self.numerator, "frame-rate numerator", minimum=1, maximum=_MAX_U32)
        denominator = _integer(
            self.denominator, "frame-rate denominator", minimum=1, maximum=_MAX_U32
        )
        divisor = math.gcd(numerator, denominator)
        object.__setattr__(self, "numerator", numerator // divisor)
        object.__setattr__(self, "denominator", denominator // divisor)

    def _wire(self) -> dict[str, int]:
        return {"numerator": self.numerator, "denominator": self.denominator}


@dataclass(frozen=True, slots=True)
class VideoSpec:
    frames: int
    frame_rate: FrameRate
    width: int
    height: int

    def __post_init__(self) -> None:
        _integer(self.frames, "video frames", minimum=1, maximum=_MAX_I64)
        if not isinstance(self.frame_rate, FrameRate):
            raise ProtocolError("video frame_rate must be a FrameRate")
        _integer(self.width, "video width", minimum=1, maximum=MAX_VIDEO_DIMENSION)
        _integer(self.height, "video height", minimum=1, maximum=MAX_VIDEO_DIMENSION)

    def _wire(self) -> dict[str, Any]:
        return {
            "frames": self.frames,
            "frame_rate": self.frame_rate._wire(),
            "width": self.width,
            "height": self.height,
        }


@dataclass(frozen=True, slots=True)
class HoldTarget:
    hold_id: str
    request_version: int

    def __post_init__(self) -> None:
        _core_identifier(self.hold_id, "hold ID")
        _integer(self.request_version, "request version", minimum=1, maximum=_MAX_U64)

    def _wire(self) -> dict[str, Any]:
        return {"hold_id": self.hold_id, "request_version": self.request_version}


@dataclass(frozen=True, slots=True)
class ContextArtifact:
    manifest: str
    sha256: str

    def __post_init__(self) -> None:
        _workspace_ref(self.manifest, "input manifest")
        _sha256(self.sha256)

    def _wire(self) -> dict[str, str]:
        return {"manifest": self.manifest, "sha256": self.sha256}


@dataclass(frozen=True, slots=True)
class HoldConstraints:
    video: VideoSpec
    conditioning: str
    motion: str

    def __post_init__(self) -> None:
        if not isinstance(self.video, VideoSpec):
            raise ProtocolError("constraints video must be a VideoSpec")
        if type(self.conditioning) is not str or self.conditioning not in {
            "bridge",
            "extend_from_left",
            "extend_from_right",
        }:
            raise ProtocolError("invalid conditioning mode")
        if type(self.motion) is not str or self.motion not in {"still", "subtle", "moderate"}:
            raise ProtocolError("invalid motion amount")

    def _wire(self) -> dict[str, Any]:
        return {
            "video": self.video._wire(),
            "conditioning": self.conditioning,
            "motion": self.motion,
        }


@dataclass(frozen=True, slots=True)
class ProviderSelection:
    pack_id: str
    pack_version: str
    runtime_id: str
    runtime_version: str
    seed: int

    def __post_init__(self) -> None:
        _protocol_identifier(self.pack_id, "provider pack ID")
        _protocol_identifier(self.pack_version, "provider pack version")
        _protocol_identifier(self.runtime_id, "runtime ID")
        _protocol_identifier(self.runtime_version, "runtime version")
        _integer(self.seed, "provider seed", minimum=0, maximum=_MAX_U64)

    def _wire(self) -> dict[str, Any]:
        return {
            "pack_id": self.pack_id,
            "pack_version": self.pack_version,
            "runtime_id": self.runtime_id,
            "runtime_version": self.runtime_version,
            "seed": self.seed,
        }


@dataclass(frozen=True, slots=True)
class WorkspaceArtifact:
    reference: str
    sha256: str
    byte_length: int

    def __post_init__(self) -> None:
        _workspace_ref(self.reference, "artifact reference")
        _sha256(self.sha256)
        _integer(self.byte_length, "artifact byte length", minimum=1, maximum=_MAX_U64)

    def _wire(self) -> dict[str, Any]:
        return {
            "reference": self.reference,
            "sha256": self.sha256,
            "byte_length": self.byte_length,
        }


@dataclass(frozen=True, slots=True)
class CandidateManifest:
    media: WorkspaceArtifact
    video: VideoSpec
    provider: ProviderSelection

    def __post_init__(self) -> None:
        if not isinstance(self.media, WorkspaceArtifact):
            raise ProtocolError("candidate media must be a WorkspaceArtifact")
        if not isinstance(self.video, VideoSpec):
            raise ProtocolError("candidate video must be a VideoSpec")
        if not isinstance(self.provider, ProviderSelection):
            raise ProtocolError("candidate provider must be a ProviderSelection")

    def _wire(self) -> dict[str, Any]:
        return {
            "media": self.media._wire(),
            "video": self.video._wire(),
            "provider": self.provider._wire(),
        }

    def to_wire(self) -> dict[str, Any]:
        """Return the Rust-compatible candidate object for host adapters."""

        return self._wire()


@dataclass(frozen=True, slots=True)
class NativeCandidateManifest:
    native: WorkspaceArtifact
    provenance: WorkspaceArtifact
    video: VideoSpec
    provider: ProviderSelection

    def __post_init__(self) -> None:
        if not isinstance(self.native, WorkspaceArtifact):
            raise ProtocolError("native candidate must be a WorkspaceArtifact")
        if not isinstance(self.provenance, WorkspaceArtifact):
            raise ProtocolError("candidate provenance must be a WorkspaceArtifact")
        if self.native.reference == self.provenance.reference:
            raise ProtocolError("native and provenance references must be distinct")
        if not isinstance(self.video, VideoSpec):
            raise ProtocolError("native candidate video must be a VideoSpec")
        if not isinstance(self.provider, ProviderSelection):
            raise ProtocolError("native candidate provider must be a ProviderSelection")

    def _wire(self) -> dict[str, Any]:
        return {
            "native": self.native._wire(),
            "provenance": self.provenance._wire(),
            "video": self.video._wire(),
            "provider": self.provider._wire(),
        }

    def to_wire(self) -> dict[str, Any]:
        return self._wire()


@dataclass(frozen=True, slots=True)
class GenerateHoldRequest:
    protocol: int
    identity: MessageIdentity
    cancellation_token: str
    project_id: str
    revision_id: str
    target: HoldTarget
    input: ContextArtifact
    output_workspace: str
    constraints: HoldConstraints
    provider: ProviderSelection

    def __post_init__(self) -> None:
        _integer(self.protocol, "protocol", minimum=PROTOCOL_VERSION, maximum=PROTOCOL_VERSION)
        if not isinstance(self.identity, MessageIdentity):
            raise ProtocolError("identity must be a MessageIdentity")
        _protocol_identifier(self.cancellation_token, "cancellation token")
        _core_identifier(self.project_id, "project ID")
        _core_identifier(self.revision_id, "revision ID")
        if not isinstance(self.target, HoldTarget):
            raise ProtocolError("target must be a HoldTarget")
        if not isinstance(self.input, ContextArtifact):
            raise ProtocolError("input must be a ContextArtifact")
        _workspace_ref(self.output_workspace, "output workspace")
        if not isinstance(self.constraints, HoldConstraints):
            raise ProtocolError("constraints must be HoldConstraints")
        if not isinstance(self.provider, ProviderSelection):
            raise ProtocolError("provider must be a ProviderSelection")

    def _wire(self) -> dict[str, Any]:
        return {
            "operation": "generate_hold",
            "protocol": self.protocol,
            "identity": self.identity._wire(),
            "cancellation_token": self.cancellation_token,
            "project_id": self.project_id,
            "revision_id": self.revision_id,
            "target": self.target._wire(),
            "input": self.input._wire(),
            "output_workspace": self.output_workspace,
            "constraints": self.constraints._wire(),
            "provider": self.provider._wire(),
        }

    def to_wire(self) -> dict[str, Any]:
        """Return the complete Rust-compatible generate request object."""

        return self._wire()


@dataclass(frozen=True, slots=True)
class GenerateBridgeRequest:
    protocol: int
    identity: MessageIdentity
    cancellation_token: str
    project_id: str
    revision_id: str
    target: HoldTarget
    input: ContextArtifact
    output_workspace: str
    constraints: HoldConstraints
    provider: ProviderSelection
    plan: dict[str, Any]

    def __post_init__(self) -> None:
        _integer(
            self.protocol,
            "protocol",
            minimum=BRIDGE_PROTOCOL_VERSION,
            maximum=BRIDGE_PROTOCOL_VERSION,
        )
        if not isinstance(self.identity, MessageIdentity):
            raise ProtocolError("identity must be a MessageIdentity")
        _protocol_identifier(self.cancellation_token, "cancellation token")
        _core_identifier(self.project_id, "project ID")
        _core_identifier(self.revision_id, "revision ID")
        if not isinstance(self.target, HoldTarget):
            raise ProtocolError("target must be a HoldTarget")
        if not isinstance(self.input, ContextArtifact):
            raise ProtocolError("input must be a ContextArtifact")
        _workspace_ref(self.output_workspace, "output workspace")
        if not isinstance(self.constraints, HoldConstraints):
            raise ProtocolError("constraints must be HoldConstraints")
        if not isinstance(self.provider, ProviderSelection):
            raise ProtocolError("provider must be a ProviderSelection")
        if not isinstance(self.plan, dict):
            raise ProtocolError("bridge plan must be an object")

    def _wire(self) -> dict[str, Any]:
        return {
            "operation": "generate_bridge",
            "protocol": self.protocol,
            "identity": self.identity._wire(),
            "cancellation_token": self.cancellation_token,
            "project_id": self.project_id,
            "revision_id": self.revision_id,
            "target": self.target._wire(),
            "input": self.input._wire(),
            "output_workspace": self.output_workspace,
            "constraints": self.constraints._wire(),
            "provider": self.provider._wire(),
            "plan": self.plan,
        }

    def to_wire(self) -> dict[str, Any]:
        return self._wire()


@dataclass(frozen=True, slots=True)
class CancelRequest:
    protocol: int
    identity: MessageIdentity
    cancellation_token: str

    def __post_init__(self) -> None:
        _integer(
            self.protocol,
            "protocol",
            minimum=PROTOCOL_VERSION,
            maximum=BRIDGE_PROTOCOL_VERSION,
        )
        if not isinstance(self.identity, MessageIdentity):
            raise ProtocolError("identity must be a MessageIdentity")
        _protocol_identifier(self.cancellation_token, "cancellation token")

    def _wire(self) -> dict[str, Any]:
        return {
            "operation": "cancel",
            "protocol": self.protocol,
            "identity": self.identity._wire(),
            "cancellation_token": self.cancellation_token,
        }

    def to_wire(self) -> dict[str, Any]:
        """Return the complete Rust-compatible cancellation object."""

        return self._wire()


@dataclass(frozen=True, slots=True)
class StageProgress:
    completed: int
    total: int

    def __post_init__(self) -> None:
        total = _integer(self.total, "progress total", minimum=1, maximum=_MAX_U64)
        _integer(self.completed, "progress completed", minimum=0, maximum=total)

    def _wire(self) -> dict[str, int]:
        return {"completed": self.completed, "total": self.total}


@dataclass(frozen=True, slots=True)
class WorkerFailure:
    code: str
    detail: str

    def __post_init__(self) -> None:
        if type(self.code) is not str or self.code not in {
            "unsupported_request",
            "invalid_input",
            "missing_artifact",
            "hash_mismatch",
            "resource_exhausted",
            "backend_failure",
            "output_validation_failed",
            "internal",
        }:
            raise ProtocolError("invalid worker failure code")
        _diagnostic(self.detail)

    def _wire(self) -> dict[str, str]:
        return {"code": self.code, "detail": self.detail}


_STAGES = {
    "preflight",
    "runtime_loading",
    "model_loading",
    "conditioning",
    "inference",
    "decoding",
    "encoding",
    "worker_validation",
}


def _parse_protocol(value: Any, *, expected: int | None = None) -> int:
    if expected is not None:
        return _integer(value, "protocol", minimum=expected, maximum=expected)
    return _integer(value, "protocol", minimum=PROTOCOL_VERSION, maximum=BRIDGE_PROTOCOL_VERSION)


def _parse_identity(value: Any) -> MessageIdentity:
    value = _object(value, {"request_id", "attempt_id"}, "identity")
    return MessageIdentity(
        _protocol_identifier(value["request_id"], "request ID"),
        _protocol_identifier(value["attempt_id"], "attempt ID"),
    )


def _parse_frame_rate(value: Any) -> FrameRate:
    value = _object(value, {"numerator", "denominator"}, "frame_rate")
    return FrameRate(
        _integer(value["numerator"], "frame-rate numerator", minimum=1, maximum=_MAX_U32),
        _integer(value["denominator"], "frame-rate denominator", minimum=1, maximum=_MAX_U32),
    )


def _parse_video(value: Any) -> VideoSpec:
    value = _object(value, {"frames", "frame_rate", "width", "height"}, "video")
    return VideoSpec(
        _integer(value["frames"], "video frames", minimum=1, maximum=_MAX_I64),
        _parse_frame_rate(value["frame_rate"]),
        _integer(value["width"], "video width", minimum=1, maximum=MAX_VIDEO_DIMENSION),
        _integer(value["height"], "video height", minimum=1, maximum=MAX_VIDEO_DIMENSION),
    )


def _parse_provider(value: Any) -> ProviderSelection:
    value = _object(
        value,
        {"pack_id", "pack_version", "runtime_id", "runtime_version", "seed"},
        "provider",
    )
    return ProviderSelection(
        _protocol_identifier(value["pack_id"], "provider pack ID"),
        _protocol_identifier(value["pack_version"], "provider pack version"),
        _protocol_identifier(value["runtime_id"], "runtime ID"),
        _protocol_identifier(value["runtime_version"], "runtime version"),
        _integer(value["seed"], "provider seed", minimum=0, maximum=_MAX_U64),
    )


def _parse_artifact(value: Any) -> WorkspaceArtifact:
    value = _object(value, {"reference", "sha256", "byte_length"}, "artifact")
    return WorkspaceArtifact(
        _workspace_ref(value["reference"], "artifact reference"),
        _sha256(value["sha256"]),
        _integer(value["byte_length"], "artifact byte length", minimum=1, maximum=_MAX_U64),
    )


def _parse_candidate(value: Any) -> CandidateManifest:
    value = _object(value, {"media", "video", "provider"}, "candidate")
    return CandidateManifest(
        _parse_artifact(value["media"]),
        _parse_video(value["video"]),
        _parse_provider(value["provider"]),
    )


def _parse_native_candidate(value: Any) -> NativeCandidateManifest:
    value = _object(value, {"native", "provenance", "video", "provider"}, "native candidate")
    return NativeCandidateManifest(
        _parse_artifact(value["native"]),
        _parse_artifact(value["provenance"]),
        _parse_video(value["video"]),
        _parse_provider(value["provider"]),
    )


def _parse_exact_ratio(value: Any, where: str) -> Fraction:
    value = _object(value, {"numerator", "denominator"}, where)
    numerator = _string(value["numerator"], f"{where}.numerator")
    denominator = _string(value["denominator"], f"{where}.denominator")
    if not numerator or not denominator:
        raise ProtocolError(f"{where} must contain non-empty integer strings")
    for name, text in (("numerator", numerator), ("denominator", denominator)):
        digits = text[1:] if text.startswith("-") else text
        if not digits or any(char not in "0123456789" for char in digits):
            raise ProtocolError(f"{where}.{name} must be a base-10 integer string")
    try:
        numerator_value = int(numerator, 10)
        denominator_value = int(denominator, 10)
    except ValueError as error:
        raise ProtocolError(f"{where} must contain base-10 integer strings") from error
    if not (-(2**127) <= numerator_value < 2**127) or not (
        0 < denominator_value < 2**127
    ):
        raise ProtocolError(f"{where} exceeds the exact-ratio bounds")
    return Fraction(numerator_value, denominator_value)


def _parse_plan(value: Any) -> dict[str, Any]:
    value = _object(
        value,
        {"schema_version", "operation", "interpolation", "project", "native", "timing", "sampling"},
        "bridge plan",
    )
    if _integer(value["schema_version"], "plan schema version", minimum=1, maximum=1) != 1:
        raise ProtocolError("unsupported bridge plan schema")
    if _string(value["operation"], "plan operation") != "bridge":
        raise ProtocolError("bridge plan operation must be bridge")
    if _string(value["interpolation"], "plan interpolation") != "linear":
        raise ProtocolError("bridge plan interpolation must be linear")

    project = _object(value["project"], {"interior_frames", "frame_rate"}, "plan project")
    project_frames = _integer(
        project["interior_frames"], "plan project interior frames", minimum=1, maximum=_MAX_I64
    )
    project_rate = _parse_frame_rate(project["frame_rate"])

    native = _object(
        value["native"], {"frame_count", "frame_rate", "width", "height"}, "plan native"
    )
    native_frames = _integer(
        native["frame_count"], "plan native frame count", minimum=2, maximum=_MAX_U32
    )
    native_rate = _parse_frame_rate(native["frame_rate"])
    width = _integer(native["width"], "plan native width", minimum=1, maximum=MAX_VIDEO_DIMENSION)
    height = _integer(native["height"], "plan native height", minimum=1, maximum=MAX_VIDEO_DIMENSION)

    timing = _object(
        value["timing"],
        {"requested_boundary_duration", "actual_boundary_duration", "retime_deviation"},
        "plan timing",
    )
    requested = _parse_exact_ratio(
        timing["requested_boundary_duration"], "requested boundary duration"
    )
    actual = _parse_exact_ratio(timing["actual_boundary_duration"], "actual boundary duration")
    deviation = _parse_exact_ratio(timing["retime_deviation"], "retime deviation")
    expected_requested = Fraction(project_frames + 1, 1) / Fraction(
        project_rate.numerator, project_rate.denominator
    )
    expected_actual = Fraction(native_frames - 1, 1) / Fraction(
        native_rate.numerator, native_rate.denominator
    )
    if (requested, actual, deviation) != (
        expected_requested,
        expected_actual,
        expected_actual - expected_requested,
    ):
        raise ProtocolError("bridge plan timing does not match its counts and rates")

    sampling = _object(value["sampling"], {"endpoint_policy"}, "plan sampling")
    if _string(sampling["endpoint_policy"], "plan endpoint policy") != "interior_only":
        raise ProtocolError("bridge plan endpoint policy must be interior_only")
    first = Fraction(native_frames - 1, project_frames + 1)
    last = Fraction(project_frames, project_frames + 1) * (native_frames - 1)
    if not (first > 0 and last < native_frames - 1):
        raise ProtocolError("bridge plan sampling must exclude both endpoints")
    # Touch all parsed fields so malformed values are rejected before the plan
    # reaches the worker; provider-specific legal counts and dimensions remain
    # the responsibility of worker_media.validate_plan.
    del width, height
    return value


def parse_host_message(
    value: Mapping[str, Any],
) -> GenerateHoldRequest | GenerateBridgeRequest | CancelRequest:
    """Validate one decoded host object and return its typed request."""

    if not isinstance(value, dict):
        raise ProtocolError("host message must be an object")
    operation = value.get("operation")
    if operation == "generate_hold":
        expected = {
            "operation",
            "protocol",
            "identity",
            "cancellation_token",
            "project_id",
            "revision_id",
            "target",
            "input",
            "output_workspace",
            "constraints",
            "provider",
        }
        value = _object(value, expected, "generate_hold")
        target = _object(value["target"], {"hold_id", "request_version"}, "target")
        input_value = _object(value["input"], {"manifest", "sha256"}, "input")
        constraints = _object(
            value["constraints"], {"video", "conditioning", "motion"}, "constraints"
        )
        return GenerateHoldRequest(
            _parse_protocol(value["protocol"], expected=PROTOCOL_VERSION),
            _parse_identity(value["identity"]),
            _protocol_identifier(value["cancellation_token"], "cancellation token"),
            _core_identifier(value["project_id"], "project ID"),
            _core_identifier(value["revision_id"], "revision ID"),
            HoldTarget(
                _core_identifier(target["hold_id"], "hold ID"),
                _integer(target["request_version"], "request version", minimum=1, maximum=_MAX_U64),
            ),
            ContextArtifact(
                _workspace_ref(input_value["manifest"], "input manifest"),
                _sha256(input_value["sha256"]),
            ),
            _workspace_ref(value["output_workspace"], "output workspace"),
            HoldConstraints(
                _parse_video(constraints["video"]),
                _string(constraints["conditioning"], "conditioning"),
                _string(constraints["motion"], "motion"),
            ),
            _parse_provider(value["provider"]),
        )
    if operation == "generate_bridge":
        expected = {
            "operation",
            "protocol",
            "identity",
            "cancellation_token",
            "project_id",
            "revision_id",
            "target",
            "input",
            "output_workspace",
            "constraints",
            "provider",
            "plan",
        }
        value = _object(value, expected, "generate_bridge")
        target = _object(value["target"], {"hold_id", "request_version"}, "target")
        input_value = _object(value["input"], {"manifest", "sha256"}, "input")
        constraints = _object(
            value["constraints"], {"video", "conditioning", "motion"}, "constraints"
        )
        return GenerateBridgeRequest(
            _parse_protocol(value["protocol"], expected=BRIDGE_PROTOCOL_VERSION),
            _parse_identity(value["identity"]),
            _protocol_identifier(value["cancellation_token"], "cancellation token"),
            _core_identifier(value["project_id"], "project ID"),
            _core_identifier(value["revision_id"], "revision ID"),
            HoldTarget(
                _core_identifier(target["hold_id"], "hold ID"),
                _integer(target["request_version"], "request version", minimum=1, maximum=_MAX_U64),
            ),
            ContextArtifact(
                _workspace_ref(input_value["manifest"], "input manifest"),
                _sha256(input_value["sha256"]),
            ),
            _workspace_ref(value["output_workspace"], "output workspace"),
            HoldConstraints(
                _parse_video(constraints["video"]),
                _string(constraints["conditioning"], "conditioning"),
                _string(constraints["motion"], "motion"),
            ),
            _parse_provider(value["provider"]),
            _parse_plan(value["plan"]),
        )
    if operation == "cancel":
        value = _object(
            value,
            {"operation", "protocol", "identity", "cancellation_token"},
            "cancel",
        )
        return CancelRequest(
            _parse_protocol(value["protocol"]),
            _parse_identity(value["identity"]),
            _protocol_identifier(value["cancellation_token"], "cancellation token"),
        )
    raise ProtocolError("host message operation must be generate_hold, generate_bridge, or cancel")


@dataclass(frozen=True, slots=True)
class StageEvent:
    protocol: int
    identity: MessageIdentity
    stage: str

    def _wire(self) -> dict[str, Any]:
        return {
            "event": "stage",
            "protocol": self.protocol,
            "identity": self.identity._wire(),
            "stage": self.stage,
        }


@dataclass(frozen=True, slots=True)
class ProgressEvent:
    protocol: int
    identity: MessageIdentity
    stage: str
    progress: StageProgress

    def _wire(self) -> dict[str, Any]:
        return {
            "event": "progress",
            "protocol": self.protocol,
            "identity": self.identity._wire(),
            "stage": self.stage,
            "progress": self.progress._wire(),
        }


@dataclass(frozen=True, slots=True)
class CompletedEvent:
    protocol: int
    identity: MessageIdentity
    candidate: CandidateManifest

    def _wire(self) -> dict[str, Any]:
        return {
            "event": "completed",
            "protocol": self.protocol,
            "identity": self.identity._wire(),
            "candidate": self.candidate._wire(),
        }


@dataclass(frozen=True, slots=True)
class CompletedBridgeEvent:
    protocol: int
    identity: MessageIdentity
    candidate: NativeCandidateManifest

    def _wire(self) -> dict[str, Any]:
        return {
            "event": "completed_bridge",
            "protocol": self.protocol,
            "identity": self.identity._wire(),
            "candidate": self.candidate._wire(),
        }


@dataclass(frozen=True, slots=True)
class FailedEvent:
    protocol: int
    identity: MessageIdentity
    failure: WorkerFailure

    def _wire(self) -> dict[str, Any]:
        return {
            "event": "failed",
            "protocol": self.protocol,
            "identity": self.identity._wire(),
            "failure": self.failure._wire(),
        }


@dataclass(frozen=True, slots=True)
class CancelledEvent:
    protocol: int
    identity: MessageIdentity

    def _wire(self) -> dict[str, Any]:
        return {
            "event": "cancelled",
            "protocol": self.protocol,
            "identity": self.identity._wire(),
        }


WorkerEvent = (
    StageEvent
    | ProgressEvent
    | CompletedEvent
    | CompletedBridgeEvent
    | FailedEvent
    | CancelledEvent
)


def _parse_stage(value: Any) -> str:
    value = _string(value, "stage")
    if value not in _STAGES:
        raise ProtocolError("invalid worker stage")
    return value


def parse_worker_message(value: Mapping[str, Any]) -> WorkerEvent:
    """Strictly validate a decoded worker event (useful for host-side tests)."""

    if not isinstance(value, dict):
        raise ProtocolError("worker message must be an object")
    event = value.get("event")
    common = {"event", "protocol", "identity"}
    protocol = _parse_protocol(value.get("protocol"))
    identity = _parse_identity(value.get("identity"))
    if event == "stage":
        value = _object(value, common | {"stage"}, "stage")
        return StageEvent(protocol, identity, _parse_stage(value["stage"]))
    if event == "progress":
        value = _object(value, common | {"stage", "progress"}, "progress")
        progress = _object(value["progress"], {"completed", "total"}, "progress")
        return ProgressEvent(
            protocol,
            identity,
            _parse_stage(value["stage"]),
            StageProgress(
                _integer(progress["completed"], "progress completed", minimum=0, maximum=_MAX_U64),
                _integer(progress["total"], "progress total", minimum=1, maximum=_MAX_U64),
            ),
        )
    if event == "completed":
        if protocol != PROTOCOL_VERSION:
            raise ProtocolError("completed is only valid for protocol 1")
        value = _object(value, common | {"candidate"}, "completed")
        return CompletedEvent(protocol, identity, _parse_candidate(value["candidate"]))
    if event == "completed_bridge":
        if protocol != BRIDGE_PROTOCOL_VERSION:
            raise ProtocolError("completed_bridge requires protocol 2")
        value = _object(value, common | {"candidate"}, "completed_bridge")
        return CompletedBridgeEvent(protocol, identity, _parse_native_candidate(value["candidate"]))
    if event == "failed":
        value = _object(value, common | {"failure"}, "failed")
        failure = _object(value["failure"], {"code", "detail"}, "failure")
        return FailedEvent(
            protocol,
            identity,
            WorkerFailure(_string(failure["code"], "failure code"), _diagnostic(failure["detail"])),
        )
    if event == "cancelled":
        value = _object(value, common, "cancelled")
        return CancelledEvent(protocol, identity)
    raise ProtocolError("worker message event is invalid")


def _wire_value(value: Any) -> Mapping[str, Any]:
    if hasattr(value, "_wire"):
        return value._wire()
    if isinstance(value, Mapping):
        return value
    raise ProtocolError("protocol value is not a message or mapping")


def write_host_message(
    stream: BinaryIO,
    message: GenerateHoldRequest | GenerateBridgeRequest | CancelRequest,
) -> None:
    write_frame(stream, _wire_value(message))


def write_worker_message(stream: BinaryIO, message: WorkerEvent) -> None:
    write_frame(stream, _wire_value(message))


def read_host_message(
    stream: BinaryIO,
) -> GenerateHoldRequest | GenerateBridgeRequest | CancelRequest | None:
    value = read_frame(stream)
    return None if value is None else parse_host_message(value)


def read_worker_message(stream: BinaryIO) -> WorkerEvent | None:
    value = read_frame(stream)
    return None if value is None else parse_worker_message(value)


class WorkerProtocol:
    """Stateful worker-side protocol endpoint.

    ``read_request`` must be called once before any other operation.  A
    cancellation reader may call ``read_cancel`` in its own thread while the
    inference thread uses the emit methods; writes are serialized and a
    matching identity plus token are required before cancellation is exposed.
    """

    def __init__(self, reader: BinaryIO, writer: BinaryIO):
        self._reader = reader
        self._writer = writer
        self._lock = threading.Lock()
        self._request: GenerateHoldRequest | GenerateBridgeRequest | None = None
        self._cancel_read = False
        self._cancel_requested = threading.Event()
        self._terminal = False

    @classmethod
    def from_fds(cls, input_fd: int = 0, output_fd: int = 1) -> "WorkerProtocol":
        """Duplicate protocol descriptors so backend stdout cannot corrupt them."""

        reader = os.fdopen(os.dup(input_fd), "rb", buffering=0)
        writer = os.fdopen(os.dup(output_fd), "wb", buffering=0)
        return cls(reader, writer)

    @property
    def request(self) -> GenerateHoldRequest | GenerateBridgeRequest:
        if self._request is None:
            raise ProtocolError("worker request has not been read")
        return self._request

    @property
    def cancel_requested(self) -> bool:
        return self._cancel_requested.is_set()

    def wait_for_cancel(self, timeout: float | None = None) -> bool:
        return self._cancel_requested.wait(timeout)

    def read_request(self) -> GenerateHoldRequest | GenerateBridgeRequest:
        if self._request is not None:
            raise ProtocolError("worker accepts only one generation request")
        message = read_host_message(self._reader)
        if message is None:
            raise ProtocolError("worker input ended before generation request")
        if not isinstance(message, (GenerateHoldRequest, GenerateBridgeRequest)):
            raise ProtocolError("first host message must be generate_hold or generate_bridge")
        self._request = message
        return message

    def read_cancel(self) -> bool:
        """Read the one optional cancellation frame, returning false on EOF."""

        self.request
        if self._cancel_read:
            raise ProtocolError("worker received more than one cancellation frame")
        message = read_host_message(self._reader)
        if message is None:
            return False
        self._cancel_read = True
        if not isinstance(message, CancelRequest):
            raise ProtocolError("only a matching cancel may follow a generation request")
        if message.protocol != self.request.protocol:
            raise ProtocolError("cancel protocol does not match generation request")
        if message.identity != self.request.identity:
            raise ProtocolError("cancel identity does not match generate_hold")
        if message.cancellation_token != self.request.cancellation_token:
            raise ProtocolError("cancel token does not match generate_hold")
        self._cancel_requested.set()
        return True

    def _emit(self, value: WorkerEvent, *, terminal: bool = False) -> None:
        with self._lock:
            if self._request is None:
                raise ProtocolError("cannot emit before a generation request")
            if self._terminal:
                raise ProtocolError("cannot emit after a terminal worker event")
            if value.identity != self._request.identity or value.protocol != self._request.protocol:
                raise ProtocolError("worker event identity or protocol does not match request")
            write_worker_message(self._writer, value)
            if terminal:
                self._terminal = True

    def emit_stage(self, stage: str) -> None:
        self._emit(StageEvent(self.request.protocol, self.request.identity, _parse_stage(stage)))

    def emit_progress(self, stage: str, completed: int, total: int) -> None:
        self._emit(
            ProgressEvent(
                self.request.protocol,
                self.request.identity,
                _parse_stage(stage),
                StageProgress(completed, total),
            )
        )

    def emit_completed(self, candidate: CandidateManifest) -> None:
        if self.request.protocol != PROTOCOL_VERSION:
            raise ProtocolError("emit_completed is only valid for protocol 1")
        if not isinstance(candidate, CandidateManifest):
            raise ProtocolError("completed candidate must be a CandidateManifest")
        self._emit(CompletedEvent(self.request.protocol, self.request.identity, candidate), terminal=True)

    def emit_completed_bridge(self, candidate: NativeCandidateManifest) -> None:
        if self.request.protocol != BRIDGE_PROTOCOL_VERSION:
            raise ProtocolError("emit_completed_bridge requires protocol 2")
        if not isinstance(candidate, NativeCandidateManifest):
            raise ProtocolError("completed_bridge candidate must be a NativeCandidateManifest")
        self._emit(
            CompletedBridgeEvent(self.request.protocol, self.request.identity, candidate),
            terminal=True,
        )

    def emit_failed(self, code: str, detail: str) -> None:
        self._emit(
            FailedEvent(
                self.request.protocol,
                self.request.identity,
                WorkerFailure(code, detail),
            ),
            terminal=True,
        )

    def emit_cancelled(self) -> None:
        if not self.cancel_requested:
            raise ProtocolError("cancelled requires a matching cancel request")
        self._emit(CancelledEvent(self.request.protocol, self.request.identity), terminal=True)


__all__ = [
    "PROTOCOL_VERSION",
    "BRIDGE_PROTOCOL_VERSION",
    "MAX_FRAME_BYTES",
    "MAX_PROTOCOL_ID_BYTES",
    "MAX_WORKSPACE_REF_BYTES",
    "MAX_DIAGNOSTIC_BYTES",
    "MAX_VIDEO_DIMENSION",
    "ProtocolError",
    "MessageIdentity",
    "FrameRate",
    "VideoSpec",
    "HoldTarget",
    "ContextArtifact",
    "HoldConstraints",
    "ProviderSelection",
    "WorkspaceArtifact",
    "CandidateManifest",
    "NativeCandidateManifest",
    "GenerateHoldRequest",
    "GenerateBridgeRequest",
    "CancelRequest",
    "StageProgress",
    "WorkerFailure",
    "StageEvent",
    "ProgressEvent",
    "CompletedEvent",
    "CompletedBridgeEvent",
    "FailedEvent",
    "CancelledEvent",
    "WorkerEvent",
    "parse_host_message",
    "parse_worker_message",
    "read_frame",
    "write_frame",
    "read_host_message",
    "write_host_message",
    "read_worker_message",
    "write_worker_message",
    "WorkerProtocol",
]
