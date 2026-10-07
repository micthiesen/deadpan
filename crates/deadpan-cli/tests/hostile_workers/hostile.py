"""Hostile worker behaviours shared by the host fixtures.

Loaded with runpy by each protocol fixture after it has read the host's
request. A mode is `hostile:<name>:<record directory>`; the record directory
belongs to the test, which reads the process identities written here and
kills anything this fixture leaves behind. Every behaviour is bounded: at most
CHILDREN sleeping descendants, sleeps of at most LIFETIME seconds and a fixed
amount of stderr. Nothing here renders, tracks, transcribes or generates.
"""

import os
from pathlib import Path
import struct
import sys
import time

CHILDREN = 24
LIFETIME = 60
STDERR_BYTES = 16 * 1024 * 1024


def parse(mode):
    """Return (name, record directory) for a hostile mode, otherwise None."""
    if not mode.startswith("hostile:"):
        return None
    _, name, directory = mode.split(":", 2)
    return name, Path(directory)


def record(directory, name, value):
    """Publish one value atomically so the test never reads a partial file."""
    temporary = directory / (name + ".partial")
    temporary.write_text(str(value))
    os.replace(temporary, directory / name)


def record_identity(directory):
    record(directory, "leader", os.getpid())
    record(directory, "group", os.getpgid(0))
    with open(directory / "launches", "a") as launches:
        launches.write("%d\n" % os.getpid())


def sleeping_child(after=None):
    """Fork a descendant that sleeps and never runs the parent's code again."""
    pid = os.fork()
    if pid == 0:
        try:
            if after is not None:
                after()
            time.sleep(LIFETIME)
        finally:
            os._exit(0)
    return pid


def fork_spam(directory):
    """A bounded burst of sleeping descendants inside the worker's group."""
    children = [sleeping_child() for _ in range(CHILDREN)]
    record(directory, "children", " ".join(map(str, children)))


def escape(directory, quiet=False):
    """A descendant that leaves the group with setsid.

    It keeps the inherited stdout/stderr pipes unless `quiet`. A quiet escapee
    waits for the test to say the host returned, then proves it is still alive
    by writing a marker and tries to replace the worker's output. The test owns
    and kills it; the supervisor does not.
    """
    def detach():
        os.setsid()
        if quiet:
            # Keep actual worker-output descriptors across the host's workspace
            # cleanup. Otherwise a rewrite after host return could touch no
            # file at all and would not test private-snapshot independence.
            retained = [candidate.open("r+b") for candidate in Path("output").rglob("*")
                        if candidate.is_file() and not candidate.is_symlink()]
            for descriptor in (0, 1, 2):
                os.close(descriptor)
            record(directory, "escaped-ready", len(retained))
            deadline = time.monotonic() + LIFETIME
            while time.monotonic() < deadline:
                if (directory / "host-returned").exists():
                    for candidate in retained:
                        candidate.seek(0)
                        candidate.write(b"replaced after admission")
                        candidate.truncate()
                        candidate.flush()
                        candidate.close()
                    record(directory, "escaped-rewritten", len(retained))
                    record(directory, "escaped-alive", os.getpid())
                    break
                time.sleep(0.01)
    escaped = sleeping_child(detach)
    record(directory, "escaped", escaped)
    if quiet:
        deadline = time.monotonic() + 5
        while not (directory / "escaped-ready").exists():
            if time.monotonic() >= deadline:
                raise RuntimeError("escapee did not retain its output")
            time.sleep(0.005)


def generic(mode):
    """Run a protocol-independent behaviour; returns only for other modes."""
    parsed = parse(mode)
    if parsed is None:
        return
    name, directory = parsed
    record_identity(directory)
    out = sys.stdout.buffer
    if name in {"malformed", "invalid_utf8", "zero_length"}:
        payload = {"malformed": b"{not-json}", "invalid_utf8": b'"\xff"',
                   "zero_length": b""}[name]
        out.write(struct.pack(">I", len(payload)) + payload)
        out.flush()
        time.sleep(LIFETIME)
    elif name == "truncated":
        out.write(struct.pack(">I", 4096) + b"{")
        out.flush()
        raise SystemExit(0)
    elif name == "oversized":
        # A declared length far above every protocol's 256 KiB frame bound,
        # followed by a held pipe: the host must refuse on the header alone.
        out.write(struct.pack(">I", 0xFFFFFFFF) + b"{" * 64)
        out.flush()
        time.sleep(LIFETIME)
    elif name == "just_over":
        out.write(struct.pack(">I", 256 * 1024 + 1) + b"{")
        out.flush()
        time.sleep(LIFETIME)
    elif name == "slow_loris":
        # A legal declared length dribbled one byte at a time. Each byte arrives
        # well inside any per-read stall, but the frame would take minutes.
        out.write(struct.pack(">I", 4096))
        out.flush()
        while True:
            out.write(b" ")
            out.flush()
            time.sleep(0.05)
    elif name == "fork_spam":
        fork_spam(directory)
        time.sleep(LIFETIME)
    elif name == "fork_spam_exit":
        fork_spam(directory)
        raise SystemExit(0)
    elif name == "escape":
        escape(directory)
        raise SystemExit(0)
    elif name == "stderr_flood":
        chunk = b"e" * 65536
        for _ in range(STDERR_BYTES // len(chunk)):
            sys.stderr.buffer.write(chunk)
        sys.stderr.buffer.flush()
        raise SystemExit(3)
    else:
        return
    raise SystemExit(0)


def artifact(mode, path, payload):
    """Place `path` for an artifact-claim mode and return the bytes to declare.

    Returns None for modes this helper does not own. `outside` is a test-owned
    file holding exactly `payload`, so the declared hash and length match a
    real file the worker must not be able to hand to the host.
    """
    parsed = parse(mode)
    if parsed is None:
        return None
    name, directory = parsed
    outside = directory / "outside.bin"
    path = Path(path)
    if name == "symlink_outside":
        path.symlink_to(outside)
    elif name == "hardlink_outside":
        os.link(outside, path)
    elif name == "symlinked_scope":
        # Replace the host-created output directory with a link to an outside
        # directory that holds a matching artifact.
        scope = Path(path.parts[0])
        os.rename(scope, scope.with_name(scope.name + ".original"))
        target = directory / "outside-scope"
        (target / path.relative_to(scope)).parent.mkdir(parents=True, exist_ok=True)
        (target / path.relative_to(scope)).write_bytes(payload)
        scope.symlink_to(target)
    elif name == "fifo":
        os.mkfifo(path)
    elif name == "sparse":
        # A 64 GiB hole costs no disk; the host must refuse it from metadata
        # instead of reading it.
        with open(path, "xb") as output:
            output.write(payload)
            output.truncate(64 * 1024 * 1024 * 1024)
    elif name == "directory":
        path.mkdir()
    elif name in {"valid", "escape_quiet", "fork_spam_valid", "absolute", "parent",
                  "wrong_attempt"}:
        with open(path, "xb") as output:
            output.write(payload)
        if name == "escape_quiet":
            escape(directory, quiet=True)
        elif name == "fork_spam_valid":
            fork_spam(directory)
    else:
        return None
    return payload


def reference(mode, normal):
    """The artifact reference to declare: hostile for absolute/parent modes."""
    parsed = parse(mode)
    if parsed is None:
        return normal
    name, directory = parsed
    if name == "absolute":
        return str((directory / "outside.bin").resolve())
    if name == "parent":
        return normal.split("/", 1)[0] + "/../../outside.bin"
    return normal
