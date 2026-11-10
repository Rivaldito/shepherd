#!/bin/sh
# Offline test of install.sh: serves fake release assets from a local http server and checks what gets
# installed. No network and no real binary needed.
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
T="$(mktemp -d)"; trap 'kill $SRV 2>/dev/null; rm -rf "$T"' EXIT
case "$(uname -s)" in Linux) os=linux ;; Darwin) os=macos ;; *) echo "skip: unsupported OS"; exit 0 ;; esac
case "$(uname -m)" in x86_64|amd64) arch=x86_64 ;; aarch64|arm64) arch=aarch64 ;; *) echo "skip: unsupported arch"; exit 0 ;; esac
core="shepherd-$os-$arch"; gui="shepherd-native-$os-$arch"
mkdir -p "$T/rel" "$T/home"

sum_of() { if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | awk '{print $1}'; else shasum -a 256 "$1" | awk '{print $1}'; fi; }
publish() { # <asset> <script body>
    printf '#!/bin/sh\necho %s\n' "$2" > "$T/rel/$1"
    printf '%s  %s\n' "$(sum_of "$T/rel/$1")" "$1" > "$T/rel/$1.sha256"
}
publish "$core" core-binary
publish "$gui" native-binary

PORT="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')"
(cd "$T/rel" && python3 -m http.server "$PORT" --bind 127.0.0.1 >/dev/null 2>&1) & SRV=$!
sleep 1
export HOME="$T/home" SHEPHERD_INSTALL_DIR="$T/home/bin" SHEPHERD_RELEASE_BASE="http://127.0.0.1:$PORT"
fresh() { rm -rf "$T/home/bin"; }
fail() { echo "FAIL: $1"; exit 1; }

# 1. installs both
fresh; sh "$ROOT/install.sh" >/dev/null 2>&1
[ "$("$T/home/bin/shepherd")" = "core-binary" ] || fail "the core does not run"
[ "$("$T/home/bin/shepherd-native")" = "native-binary" ] || fail "the desktop app does not run"
echo "ok: installs the core and the desktop app"

# 2. SHEPHERD_NO_GUI=1 skips the app
fresh; SHEPHERD_NO_GUI=1 sh "$ROOT/install.sh" >/dev/null 2>&1
[ -x "$T/home/bin/shepherd" ] || fail "no core with SHEPHERD_NO_GUI=1"
[ ! -e "$T/home/bin/shepherd-native" ] || fail "installed the app despite SHEPHERD_NO_GUI=1"
echo "ok: SHEPHERD_NO_GUI=1 installs only the core"

# 3. a release without the app still installs the core and succeeds
mv "$T/rel/$gui" "$T/gui.bak"; mv "$T/rel/$gui.sha256" "$T/gui.sha.bak"
fresh; sh "$ROOT/install.sh" >/dev/null 2>&1 || fail "a release without the app should still install"
[ -x "$T/home/bin/shepherd" ] || fail "no core when the app is missing"
[ ! -e "$T/home/bin/shepherd-native" ] || fail "installed an app that is not in the release"
echo "ok: a release without the app installs the core"
mv "$T/gui.bak" "$T/rel/$gui"; mv "$T/gui.sha.bak" "$T/rel/$gui.sha256"

# 4. a tampered app is not installed, but the core is
printf 'tampered\n' >> "$T/rel/$gui"
fresh; sh "$ROOT/install.sh" >/dev/null 2>&1 || fail "a bad app checksum should not fail the whole install"
[ -x "$T/home/bin/shepherd" ] || fail "no core when the app is corrupted"
[ ! -e "$T/home/bin/shepherd-native" ] || fail "installed a corrupted app"
echo "ok: refuses an app that does not match its checksum"
publish "$gui" native-binary

# 5. a tampered core installs nothing and fails
printf 'tampered\n' >> "$T/rel/$core"
fresh
if sh "$ROOT/install.sh" >/dev/null 2>&1; then fail "installed a corrupted core"; fi
[ ! -e "$T/home/bin/shepherd" ] || fail "left a corrupted core behind"
echo "ok: refuses a core that does not match its checksum"
