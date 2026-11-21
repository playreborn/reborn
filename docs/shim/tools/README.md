# Observation tool scripts

Scripts for the stages of ADR 0038 that use public tools. They only
observe calls a game makes to the operating system; they never look at
the code of `steamclient`.

## `windbg-getprocaddress.wds` (stage 1, Windows)

Logs every call of `GetProcAddress` (`bm` on `kernel32!GetProcAddress*`
and `kernelbase!GetProcAddress*`) with the module handle and the asked
name, so the names a game asks `steamclient64.dll` for can be filtered
afterwards with the module's load address from the same log. Microsoft
documents `GetProcAddress` and the x64 calling convention: `rcx` holds
the module handle, `rdx` the name, or an ordinal if it is at most
`0xffff`. The script ends with `g`, so the game continues on its own.

In WinDbg (Microsoft), started with **File → Start debugging → Launch
executable** on the game:

```text
.logopen <repo>\fixtures\raw\windbg_<appid>.log
sxe -c "$$< <repo-with-forward-slashes>/docs/shim/tools/windbg-getprocaddress.wds" ld:steamclient64.dll
g
```

Then play to the main menu, quit the game, and run `.logclose`.

Pitfalls, found the hard way (2026-11-21):

- Inside the quotes of `sxe -c "..."`, a backslash is an escape
  character, so the script path needs forward slashes.
- `$$<` takes the rest of the command as the file name; nothing can follow
  it, which is why the script runs `g` itself.
- Comments in the script broke WinDbg's parsing, so it has none.
- Tested with `cdb.exe` on `cmd.exe /c exit` and `ld:combase.dll`: no
  errors, one `gpa` line per call.

## `reborn-observe` (stage 2, Windows)

Arms and disarms a proxy session (ADR 0039). It changes only
`HKCU\Software\Valve\Steam\ActiveProcess\SteamClientDll64`, always with
a saved copy of the old value in Reborn's data folder
(`shim-observe\`), and only when the user runs it.

```text
reborn-observe status
reborn-observe arm --proxy <built proxy dll>
reborn-observe disarm
```

Start the official client first, arm, run the game once, disarm before
closing the official client. `disarm` only writes the old value back if
the proxy's path is still there, and can be run any number of times.

The proxy is the DLL of the same crate: `cargo build -p
reborn-shim-observe` writes `target\debug\reborn_shim_observe.dll`. It
exports the five entry points from `docs/shim/entry-points.md`, forwards
every call unchanged to the real `steamclient64.dll` (the saved path) and
writes `observe-<process id>.log` into Reborn's `shim-observe` folder:
which entry point was called, when and on which thread, the raw values
of the argument registers (`rcx`, `rdx`, `r8`, `r9`, the low 64 bits of
`xmm0` to `xmm3`), and on return the raw result registers (`rax`, low 64
bits of `xmm0`); the first 50 calls of each, then every 10 000th. Where an
integer register points to readable memory with 2 to 64 printable ASCII
characters and a NUL, that text is logged too (`rcx->"..."`); memory is
checked with `VirtualQuery` first, and nothing else is read.

To see results, the stub replaces the game's return address with a
return stub and keeps the real one in a per-thread stack; arguments and
the stack the real function sees are unchanged.

Checked without a game (2026-11-21): unit tests call the five stubs with
integer, floating-point and stack arguments and a nested call, compare
what arrives and what comes back, and read text only from readable
memory; the built DLL, loaded from Python with stand-in "real" DLLs (one
with a dependency in its own folder), forwarded calls, logged the
arguments and the right results.
