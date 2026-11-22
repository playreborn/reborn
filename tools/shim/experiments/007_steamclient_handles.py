# Checks the predictions of experiment 7 in one proxy log.
import re, sys

line_re = re.compile(r"^(\d+) ms ThreadId\(\d+\) (SteamClient0\d\d)#(\d+) (call|return) (\d+) (.*)$")
reg = lambda name, text: (lambda m: int(m.group(1), 16) if m else None)(re.search(name + r"=0x([0-9a-f]+)", text))
low = lambda v: None if v is None else v & 0xffffffff

calls = []  # (time, pos, n, rest) for calls
results = {}  # (pos, n) -> rax
for raw in open(sys.argv[1], encoding="utf-8", errors="replace"):
    raw = re.sub(r" xmm\d=0x[0-9a-f]+", "", raw.rstrip("\n"))
    raw = raw.replace(" (rcx held the stand-in, now the real object)", "")
    m = line_re.match(raw)
    if not m:
        continue
    t, ver, pos, kind, n, rest = m.groups()
    if kind == "call":
        calls.append((int(t), ver, int(pos), int(n), rest))
    elif rest.startswith("rax="):
        results[(ver, int(pos), int(n))] = reg("rax", rest)

if not calls:
    print("no SteamClient020/021/023 calls"); sys.exit()
ver = calls[0][1]
calls = [c for c in calls if c[1] == ver]
bad = []
first = calls[0]
ok1 = first[2] == 0
P = low(results.get((ver, 0, first[3])))
second = calls[1]
ok2 = second[2] == 2 and low(reg("rdx", second[4])) == P
U = low(results.get((ver, 2, second[3])))
n9 = n12 = 0
for t, v, pos, n, rest in calls:
    texts = re.findall(r'(rdx|r8|r9)->"([^"]*)"', rest)
    rdx, r8 = low(reg("rdx", rest)), low(reg("r8", rest))
    if pos == 9:
        n9 += 1
        if not (rdx == P and any(r == "r8" and s.startswith("SteamUtils") for r, s in texts)):
            bad.append(("9", t, rest[:110]))
    elif any(r == "r9" for r, s in texts):
        n12 += 1
        if not (r8 == P and rdx in (U, 0)):
            bad.append((str(pos), t, rest[:110]))
print(f"{sys.argv[1].split('/')[-1]} {ver}: P={P} U={U} | 1: {'match' if ok1 else 'MISMATCH'} | 2: {'match' if ok2 else 'MISMATCH'} | 3: {n9} calls | 4: {n12} calls | mismatches: {len(bad)}")
for b in bad[:8]:
    print("   ", b)
if ver == "SteamClient020":
    tail = [(t, pos, n, rest) for t, v, pos, n, rest in calls if pos in (4, 1, 23)]
    if tail:
        seq = [p for _, p, _, _ in tail]
        ok5 = True
        for t, pos, n, rest in tail:
            rdx, r8 = low(reg("rdx", rest)), low(reg("r8", rest))
            if pos == 4 and not (rdx == P and r8 == U): ok5 = False
            if pos == 1 and rdx != P: ok5 = False
        if 1 in seq and 23 in seq and seq.index(23) < seq.index(1): ok5 = False
        print(f"    5: close calls {seq} -> {'match' if ok5 else 'MISMATCH'}")
    else:
        print("    5: no close calls (counts neither way)")
