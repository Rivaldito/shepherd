#!/usr/bin/env bash
# The native app on YOUR environment: your real HOME, so the shells in the panes have your colours, your
# PATH and your logins (`claude` stays signed in). It runs its own named session (default `native`), so your
# other sessions and servers are not touched; the session keeps running when the window closes.
#   dev/live.sh                  start the session if needed and open the window (in the current folder)
#   dev/live.sh stop             stop the session (its terminals close)
# Variables: SERVER (shepherd binary), APP (shepherd-native binary), SESSION (default: native).
set -u
HERE="$(cd "$(dirname "$0")/.." && pwd)"
SERVER="${SERVER:-$HERE/../target/debug/shepherd}"
APP="${APP:-$HERE/target/release/shepherd-native}"
SESSION="${SESSION:-native}"
[ -x "$SERVER" ] || { echo "no server binary at $SERVER (run: make native-build)"; exit 1; }
# a server started from inside a Shepherd pane must not believe it is still in that one
unset SHEPHERD_ENV SHEPHERD_SOCKET_PATH SHEPHERD_SESSION SHEPHERD_PANE_ID SHEPHERD_CLIENT_SOCKET_PATH

socket() {
  "$SERVER" session list --json 2>/dev/null | python3 -c "
import sys, json
for s in json.load(sys.stdin).get('sessions', []):
    if s['name'] == sys.argv[1] and s['running']:
        print(s['socket_path'])" "$SESSION"
}

if [ "${1:-}" = "stop" ]; then
  s="$(socket)"
  [ -n "$s" ] && SHEPHERD_SOCKET_PATH="$s" "$SERVER" server stop && echo "stopped session $SESSION" || echo "session $SESSION is not running"
  exit 0
fi

[ -x "$APP" ] || { echo "no app binary at $APP (run: make native-build)"; exit 1; }
SOCK="$(socket)"
if [ -z "$SOCK" ]; then
  setsid "$SERVER" --session "$SESSION" server >"${TMPDIR:-/tmp}/shepherd-$SESSION.log" 2>&1 &
  for _ in $(seq 1 50); do SOCK="$(socket)"; [ -n "$SOCK" ] && break; sleep 0.2; done
  [ -n "$SOCK" ] || { echo "the server did not start; see ${TMPDIR:-/tmp}/shepherd-$SESSION.log"; exit 1; }
  # a new session has no terminal yet: open one in this folder
  SHEPHERD_SOCKET_PATH="$SOCK" "$SERVER" workspace create --cwd "$PWD" --label "$(basename "$PWD")" --focus >/dev/null 2>&1
fi
export SHEPHERD_SOCKET_PATH="$SOCK" SHEPHERD_BIN="$SERVER"
[ -n "${MODE:-}" ] && export SHEPHERD_NATIVE_MODE="$MODE"
[ -n "${TAB:-}" ] && export SHEPHERD_NATIVE_TAB="$TAB"
exec "$APP"
