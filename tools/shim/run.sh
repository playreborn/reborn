#!/usr/bin/env bash
# One observation run under ADR 0044, 0045 and 0049: arm the proxy, start a
# game through the official client, wait, close it normally, disarm, and
# print which interface positions it called.
#
# Usage: tools/shim/run.sh <app id> [seconds after Steam loaded] [seconds to wait for Steam]
# Environment: NOAPP=1 arms without --app (no shadow or answer mode);
#              PIDARG=<pid> passes --pid (experiment 6 only).
#
# Limits (ADR 0045, 0052): at least 2 minutes between starts, no daily cap;
# the start log is shim-observe/starts.log in Reborn's data folder.
# Must run outside the Claude desktop app (its registry is virtualized,
# see docs/shim/tools/README.md).
set -u
APP="$1"; WAIT="${2:-40}"; CAPTURE="${3:-60}"
HERE="$(cd "$(dirname "$0")" && pwd -W 2>/dev/null || pwd)"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
D="$LOCALAPPDATA/Reborn/data/shim-observe"
STARTS="$D/starts.log"
mkdir -p "$D"
LAST=$(tail -1 "$STARTS" 2>/dev/null | cut -c1-19)
if [ -n "$LAST" ]; then
  GAP=$(( $(date +%s) - $(date -d "$LAST" +%s) ))
  if [ "$GAP" -lt 125 ]; then echo "waiting $((125 - GAP))s for the 2-minute gap"; sleep $((125 - GAP)); fi
fi
echo "$(date '+%F %T') start $APP" >> "$STARTS"
cd "$ROOT" || exit 1
ARGS=(arm --proxy "$(pwd -W 2>/dev/null || pwd)/target/debug/reborn_shim_observe.dll")
[ -z "${NOAPP:-}" ] && ARGS+=(--app "$APP")
[ -n "${PIDARG:-}" ] && ARGS+=(--pid "$PIDARG")
./target/debug/reborn-observe.exe "${ARGS[@]}" | head -3
OUT=$(powershell -NoProfile -ExecutionPolicy Bypass -File "$HERE/dbwin.ps1" -Seconds "$CAPTURE" -Launch -AppId "$APP")
echo "$OUT"
PID_=$(echo "$OUT" | grep -oE 'pid [0-9]+' | head -1 | cut -d' ' -f2)
if [ -z "$PID_" ]; then
  echo "no S_API line seen"
  DIR=$(grep -m1 '"installdir"' "/c/Program Files (x86)/Steam/steamapps/appmanifest_$APP.acf" | cut -d'"' -f4)
  powershell -NoProfile -ExecutionPolicy Bypass -File "$HERE/shot.ps1" -Out "$D/nosapi-$APP.png" >/dev/null
  PIDS=$(powershell -NoProfile -Command "Get-Process | Where-Object { \$_.Path -like '*\\steamapps\\common\\$DIR\\*' } | ForEach-Object { \$_.Id }" | tr -d '\r')
  echo "processes of the game: ${PIDS:-none}"
  for p in $PIDS; do taskkill //PID "$p" >/dev/null 2>&1; done
  sleep 10
  for p in $PIDS; do tasklist //FI "PID eq $p" //NH | grep -q "$p" && echo "pid $p STILL RUNNING; left as it is"; done
  ./target/debug/reborn-observe.exe disarm
  exit 1
fi
sleep "$WAIT"
tasklist //FI "PID eq $PID_" //NH | grep -q "$PID_" || echo "game already gone before close request"
taskkill //PID "$PID_" >/dev/null 2>&1
for i in $(seq 1 24); do tasklist //FI "PID eq $PID_" //NH | grep -q "$PID_" || { echo "exited after ~$((i*5))s"; break; }; sleep 5; done
tasklist //FI "PID eq $PID_" //NH | grep -q "$PID_" && echo "STILL RUNNING after 2 minutes; left as it is"
./target/debug/reborn-observe.exe disarm
echo "log observe-$PID_.log"
echo "$(date '+%F %T') log $APP observe-$PID_.log" >> "$STARTS"
