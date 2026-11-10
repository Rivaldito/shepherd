#!/bin/sh
# Shepherd installer: downloads the prebuilt binaries for your system from a GitHub release, checks their
# SHA-256 and installs them into ~/.local/bin:
#   shepherd         the server, the cli and the terminal ui
#   shepherd-native  the native desktop app (skipped with SHEPHERD_NO_GUI=1)
#
#   curl -fsSL https://raw.githubusercontent.com/Rivaldito/shepherd/main/install.sh | sh
#
# Variables:
#   SHEPHERD_VERSION      release tag to install (default: the newest release, including betas), e.g. v0.2.0-beta
#   SHEPHERD_INSTALL_DIR  where to put the binaries (default: ~/.local/bin)
#   SHEPHERD_NO_GUI=1     install only `shepherd`, not the desktop app (for servers without a screen)
#   SHEPHERD_RELEASE_BASE base URL holding the assets, for mirrors and tests (skips the GitHub lookup)
set -eu

REPO="Rivaldito/shepherd"
INSTALL_DIR="${SHEPHERD_INSTALL_DIR:-$HOME/.local/bin}"

main() {
    echo ""
    echo "  shepherd installer"
    echo "  https://shepherd.rivaldito.com"
    echo ""

    case "$(uname -s)" in
        Linux)  os="linux" ;;
        Darwin) os="macos" ;;
        *)      err "unsupported OS: $(uname -s). Shepherd has binaries for Linux and macOS." ;;
    esac
    case "$(uname -m)" in
        x86_64|amd64)  arch="x86_64" ;;
        aarch64|arm64) arch="aarch64" ;;
        *)             err "unsupported architecture: $(uname -m)" ;;
    esac
    log "detected ${os}/${arch}"

    need curl
    need awk
    if command -v sha256sum >/dev/null 2>&1; then
        SHA_TOOL="sha256sum"
    elif command -v shasum >/dev/null 2>&1; then
        SHA_TOOL="shasum"
    elif command -v openssl >/dev/null 2>&1; then
        SHA_TOOL="openssl"
    else
        err "checking the downloads needs sha256sum, shasum or openssl"
    fi

    if [ -n "${SHEPHERD_RELEASE_BASE:-}" ]; then
        BASE="${SHEPHERD_RELEASE_BASE%/}"
        TAG="custom"
    else
        TAG="${SHEPHERD_VERSION:-}"
        if [ -z "$TAG" ]; then
            log "looking for the newest release..."
            # /releases (not /releases/latest): the latest release may be a beta, which /latest ignores
            TAG="$(curl -fsSL --retry 3 --connect-timeout 10 --max-time 20 \
                "https://api.github.com/repos/${REPO}/releases?per_page=1" \
                | awk -F '"' '/"tag_name"[[:space:]]*:/ { print $4; exit }')" \
                || err "can't reach github.com. Please try again later."
            [ -n "$TAG" ] || err "no release found for ${REPO}"
        fi
        BASE="https://github.com/${REPO}/releases/download/${TAG}"
    fi
    log "installing ${TAG}"

    TMP="$(mktemp -d)"
    trap 'rm -rf "$TMP"' EXIT
    mkdir -p "$INSTALL_DIR"

    # the core is required
    fetch "shepherd-${os}-${arch}" "shepherd" required

    # the desktop app is optional: a release without it (or a failed download) does not stop the install
    GUI=0
    if [ "${SHEPHERD_NO_GUI:-0}" = "1" ]; then
        log "skipping the desktop app (SHEPHERD_NO_GUI=1)"
    elif fetch "shepherd-native-${os}-${arch}" "shepherd-native" optional; then
        GUI=1
    else
        warn "the desktop app was not installed; the terminal ui in 'shepherd' works without it"
    fi

    case ":${PATH}:" in
        *":${INSTALL_DIR}:"*) ;;
        *)
            echo ""
            warn "${INSTALL_DIR} is not in your PATH. Add it to your shell config:"
            echo ""
            echo "    export PATH=\"${INSTALL_DIR}:\$PATH\""
            ;;
    esac

    echo ""
    log "next: enable the orchestrator once, then start it in a git repository"
    echo "    mkdir -p ~/.config/shepherd && printf '[shepherd]\\nenabled = true\\n' > ~/.config/shepherd/config.toml"
    echo "    cd ~/your-repo && shepherd"
    if [ "$GUI" = "1" ]; then
        echo "    shepherd-native      # the desktop app, in a second terminal while shepherd runs"
    fi
    echo ""
}

# fetch <asset> <installed name> <required|optional>: download, verify the SHA-256 and install one binary.
fetch() {
    asset="$1"; name="$2"; mode="$3"
    fail() { if [ "$mode" = "required" ]; then err "$1"; else warn "$1"; return 1; fi; }

    curl -fsSL --retry 3 --connect-timeout 10 --max-time 180 "${BASE}/${asset}" -o "${TMP}/${asset}" \
        || { fail "download failed: ${BASE}/${asset} (does release ${TAG} have it for this system?)"; return 1; }
    # each binary has a "<asset>.sha256" file next to it (the output of sha256sum / shasum -a 256)
    curl -fsSL --retry 3 --connect-timeout 10 --max-time 20 "${BASE}/${asset}.sha256" -o "${TMP}/${asset}.sha256" \
        || { fail "download failed: ${BASE}/${asset}.sha256"; return 1; }

    expected="$(awk '{ print tolower($1); exit }' "${TMP}/${asset}.sha256")"
    [ "${#expected}" -eq 64 ] || { fail "${asset}.sha256 does not hold a valid checksum"; return 1; }
    case "$SHA_TOOL" in
        sha256sum) actual="$(sha256sum < "${TMP}/${asset}" | awk '{ print $1 }')" ;;
        shasum)    actual="$(shasum -a 256 < "${TMP}/${asset}" | awk '{ print $1 }')" ;;
        openssl)   actual="$(openssl dgst -sha256 < "${TMP}/${asset}" | awk '{ print $NF }')" ;;
    esac
    [ "$actual" = "$expected" ] || { fail "${asset} does not match its checksum; it was not installed"; return 1; }

    mv "${TMP}/${asset}" "${INSTALL_DIR}/${name}"
    chmod +x "${INSTALL_DIR}/${name}"
    log "installed ${INSTALL_DIR}/${name} (checksum ok)"
}

log()  { printf '  \033[32m>\033[0m %s\n' "$1"; }
warn() { printf '  \033[33m!\033[0m %s\n' "$1" >&2; }
err()  { printf '  \033[31m✗\033[0m %s\n' "$1" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || err "requires '$1'; install it first"; }

main "$@"
