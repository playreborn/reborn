"""Experiment 10 (docs/shim/meanings.md): SteamUtils009/010 position 14,
candidate GetCurrentBatteryPower. Prediction: on a computer on AC power,
every call returns 255 in its lowest byte.

Usage: python tools/shim/experiments/010_battery_power.py <log>...
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import observelog  # noqa: E402

overall = True
for path in sys.argv[1:]:
    hits = [c for c in observelog.calls(path)
            if c.interface in ("SteamUtils009", "SteamUtils010") and c.position == 14]
    bad = [c for c in hits if c.rax is None or c.rax & 0xFF != 255]
    ok = bool(hits) and not bad
    overall &= ok or not hits
    verdict = "match" if ok else ("no calls (counts neither way)" if not hits else "MISMATCH")
    print(f"{Path(path).name}: {len(hits)} logged calls, {len(bad)} not 255 -> {verdict}")
    for c in bad[:3]:
        print("   ", c.ms, c.interface, c.position, hex(c.rax) if c.rax is not None else None)
print("experiment 10:", "match" if overall else "MISMATCH")
