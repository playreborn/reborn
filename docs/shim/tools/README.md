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
