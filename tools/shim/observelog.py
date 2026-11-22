"""Reading `observe-<pid>.log` files of the observation proxy (ADR 0038).

Used by `coverage.py`, `run.py` and the experiment scripts in
`experiments/`. Floating-point registers and the `rcx` note are dropped,
as in `docs/shim/` evidence.
"""
import os
import re
from dataclasses import dataclass, field
from pathlib import Path

LINE = re.compile(r"^(\d+) ms ThreadId\(\d+\) (\S+?)#(\d+) (call|return) (\d+) (.*)$")
TEXT = re.compile(r'(rcx|rdx|r8|r9)->"([^"]*)"')


def log_dir() -> Path:
    """Where the proxy writes its logs (Reborn's data folder)."""
    base = os.environ.get("LOCALAPPDATA")
    if not base:
        raise SystemExit("LOCALAPPDATA is not set (the proxy is Windows-only so far)")
    return Path(base) / "Reborn" / "data" / "shim-observe"


def clean(line: str) -> str:
    """A log line as docs quote it: no xmm registers, no rcx note."""
    line = re.sub(r" xmm\d=0x[0-9a-f]+", "", line.rstrip("\n"))
    return line.replace(" (rcx held the stand-in, now the real object)", "")


def register(name: str, text: str):
    """The value of register `name` in a call or return line, or None."""
    m = re.search(name + r"=0x([0-9a-f]+)", text)
    return int(m.group(1), 16) if m else None


def low32(value):
    return None if value is None else value & 0xFFFFFFFF


@dataclass
class Call:
    """One call through a stand-in, with its result if it returned."""

    ms: int
    interface: str
    position: int
    number: int
    rest: str
    texts: dict = field(default_factory=dict)
    rax: int | None = None
    extra: list = field(default_factory=list)  # result text, bytes, shadow lines

    def reg(self, name):
        return register(name, self.rest)


def calls(path) -> list:
    """All method calls (`Interface#n`) of a log, in order, with results."""
    out, open_calls = [], {}
    for raw in open(path, encoding="utf-8", errors="replace"):
        line = clean(raw)
        m = LINE.match(line)
        if not m:
            continue
        ms, interface, pos, kind, number, rest = m.groups()
        key = (interface, int(pos), int(number))
        if kind == "call":
            if "answered by Reborn" in rest:
                continue
            c = Call(int(ms), interface, int(pos), int(number), rest,
                     dict((r, t) for r, t in TEXT.findall(rest)))
            open_calls[key] = c
            out.append(c)
        elif key in open_calls:
            c = open_calls[key]
            if rest.startswith("rax="):
                c.rax = register("rax", rest)
            else:
                c.extra.append(rest)
    return out


def after_u32(call, register):
    """The first four bytes the proxy read at `register` after `call`
    returned (`after <register>->[...]`, ADR 0043), as a little-endian
    number, or None."""
    for extra in call.extra:
        m = re.search(r"after " + register + r"->\[([0-9a-f]{8})", extra)
        if m:
            return int.from_bytes(bytes.fromhex(m.group(1)), "little")
    return None


def app_of(path):
    """The app ID `run.sh` recorded for this log in `starts.log`, or None.
    The last matching line wins (a log name is reused when a process ID
    is)."""
    name = Path(path).name
    try:
        lines = open(log_dir() / "starts.log", encoding="utf-8").read().splitlines()
    except OSError:
        return None
    for line in reversed(lines):
        parts = line.split()
        if len(parts) == 5 and parts[2] == "log" and parts[4] == name:
            return int(parts[3])
    return None


def first_steam_client(path) -> str:
    """The `SteamClient…` version (other than 017) the game asked for."""
    for raw in open(path, encoding="utf-8", errors="replace"):
        m = re.search(r'CreateInterface call \d+ .*?rcx->"(SteamClient\d+)"', raw)
        if m and m.group(1) != "SteamClient017":
            return m.group(1)
    return "?"


EXPORT = re.compile(r"^(\d+) ms ThreadId\((\d+)\) (Steam_\w+) (call|return) (\d+) (.*)$")
HEADER = re.compile(r"wrote rdx->\[([0-9a-f ]+)\]")
PAYLOAD_SIZE = re.compile(r"payload: size (\d+) sha256 [0-9a-f]{64}(?: first \[([0-9a-f ]+)\])?")


@dataclass
class Export:
    """One line of a `Steam_…` entry point (`call`, `return` or, after a
    `Steam_BGetCallback` that returned 1, the `wrote` note of step 4b/4d).
    Only call numbers up to 50 (and every 10000th) are logged; `wrote`
    notes are logged for every such call (`crates/shim-observe`)."""

    ms: int
    thread: int
    name: str
    kind: str  # "call", "return" or "wrote"
    number: int
    rest: str

    def reg(self, name):
        return register(name, self.rest)


def exports(path) -> list:
    """All `Steam_…` entry point lines of a log, in order."""
    out = []
    for raw in open(path, encoding="utf-8", errors="replace"):
        m = EXPORT.match(clean(raw))
        if not m:
            continue
        ms, thread, name, kind, number, rest = m.groups()
        if kind == "return" and rest.startswith("wrote "):
            kind = "wrote"
        out.append(Export(int(ms), int(thread), name, kind, int(number), rest))
    return out


@dataclass
class Header:
    """The bytes at `rdx` after `Steam_BGetCallback` returned 1, as words
    (little endian), and the payload note of step 4d (ADR 0050)."""

    raw: bytes
    payload_size: int | None  # size from the `payload: size N sha256` note
    payload_first: bytes | None  # first bytes, only for listed numbers

    def u32(self, at):
        return int.from_bytes(self.raw[at:at + 4], "little") if len(self.raw) >= at + 4 else None

    def u64(self, at):
        return int.from_bytes(self.raw[at:at + 8], "little") if len(self.raw) >= at + 8 else None


def header(export):
    """The callback header of a `wrote` line, or None."""
    m = HEADER.search(export.rest)
    if not m:
        return None
    p = PAYLOAD_SIZE.search(export.rest)
    first = bytes.fromhex(p.group(2).replace(" ", "")) if p and p.group(2) else None
    return Header(bytes.fromhex(m.group(1).replace(" ", "")),
                  int(p.group(1)) if p else None, first)
