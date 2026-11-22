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


def first_steam_client(path) -> str:
    """The `SteamClient…` version (other than 017) the game asked for."""
    for raw in open(path, encoding="utf-8", errors="replace"):
        m = re.search(r'CreateInterface call \d+ .*?rcx->"(SteamClient\d+)"', raw)
        if m and m.group(1) != "SteamClient017":
            return m.group(1)
    return "?"
