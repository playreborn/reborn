# 0001: Interoperability through reverse engineering, and red lines

- **Status:** Accepted
- **Date:** 2026-11-19

## Context

Valve's official Web API offers almost nothing a client needs: no depot keys,
no manifests, no licenses, no presence. Everything a Steam client does runs
over the undocumented CM protocol. Projects such as SteamKit2,
node-steam-user and DepotDownloader have implemented it for over a decade.

## Problem

Do we restrict ourselves to documented interfaces, or implement the
undocumented protocol to reach the project's goal?

## Alternatives considered

1. **Documented interfaces only.** Legally simplest, but Reborn would be a
   launcher that depends on the official client. The project goal is unreachable.
2. **Interoperability through reverse engineering** of the protocol, based on
   public reference implementations and observation of our own client's traffic.

## Decision

Option 2, bound by the red lines in `AGENTS.md` § 2: no DRM bypass, no
Steam Guard bypass, no fabricated ownership, no achievement manipulation, no
abuse automation, no Valve code or assets, no telemetry without opt-in, and
gentle server usage.

## Consequences

- We depend on Valve's continued tolerance of third-party clients. The red
  lines are what keeps that tolerance plausible.
- Protocol changes can break us at any time; generated protobufs and replay
  fixtures make breakage visible quickly.
- Users are warned that Reborn is unofficial. Developers use secondary accounts.
- In the EU, § 69d(3) and § 69e UrhG (Art. 5(3), 6 of Directive 2009/24/EC)
  support observation and interoperability. This is not legal advice; a
  proper review happens before 1.0.
