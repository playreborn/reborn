# Security Policy

Reborn handles Steam login tokens and writes game files to disk, so security
reports are taken seriously.

## Reporting a vulnerability

**Do not open a public issue.** Report vulnerabilities privately through
GitHub: *Security* tab → *Report a vulnerability* in
[playreborn/reborn](https://github.com/playreborn/reborn/security/advisories/new).

Please include what is affected, how to reproduce it and what an attacker
could achieve. Never include real tokens, passwords or account data.

We aim to acknowledge reports within 7 days and will keep you updated until a
fix is released. Credit is given in the advisory unless you prefer otherwise.

## In scope

- Leaking or insecurely storing tokens or credentials
- Path traversal or arbitrary file writes from manifests, servers or IPC
- Local privilege escalation or unauthorized access to the daemon's IPC socket
- Anything that could let a third party act on a user's Steam account

## Supported versions

Reborn is pre-alpha. Only the latest commit on `main` receives fixes until
the first release.
