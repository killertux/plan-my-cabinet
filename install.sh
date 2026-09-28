#!/bin/sh
# Install Plan My Cabinet from the latest GitHub release.
#
#   curl -fsSL https://raw.githubusercontent.com/killertux/plan-my-cabinet/master/install.sh | sh
#
# Environment:
#   PMC_VERSION  release tag to install (default: latest), e.g. v0.1.0
#   PMC_PREFIX   Linux install prefix (default: ~/.local)
#   PMC_APPDIR   macOS folder for the app (default: ~/Applications)
set -eu

REPO="killertux/plan-my-cabinet"
APP="Plan My Cabinet"
NAME="plan-my-cabinet"

say() { printf '%s\n' "$*"; }
fail() { printf 'install: %s\n' "$*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || fail "'$1' is required"; }

need curl
need tar
need uname

case "$(uname -s)" in
    Darwin) os=macos ;;
    Linux) os=linux ;;
    MINGW* | MSYS* | CYGWIN*)
        fail "on Windows, run in PowerShell: irm https://raw.githubusercontent.com/$REPO/master/install.ps1 | iex" ;;
    *) fail "unsupported system: $(uname -s)" ;;
esac

case "$(uname -m)" in
    arm64 | aarch64) arch=arm64 ;;
    x86_64 | amd64) arch=x86_64 ;;
    *) fail "unsupported processor: $(uname -m)" ;;
esac

# A Mac running this shell under Rosetta still gets the native build.
if [ "$os" = macos ] && [ "$arch" = x86_64 ] &&
    [ "$(sysctl -in sysctl.proc_translated 2>/dev/null)" = 1 ]; then
    arch=arm64
fi
if [ "$os" = linux ] && [ "$arch" != x86_64 ]; then
    fail "only x86_64 Linux builds are published; build from source (see README)"
fi

version="${PMC_VERSION:-}"
if [ -z "$version" ]; then
    # The latest-release page redirects to .../releases/tag/<tag>.
    url=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest") ||
        fail "could not reach GitHub"
    version="${url##*/}"
    case "$version" in
        v*) ;;
        *) fail "no published release found" ;;
    esac
fi

asset="$NAME-${version#v}-$os-$arch.tar.gz"
base="https://github.com/$REPO/releases/download/$version"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

say "Downloading $APP $version for $os $arch..."
curl -fL --progress-bar -o "$tmp/$asset" "$base/$asset" || fail "download failed: $base/$asset"

if curl -fsSL -o "$tmp/SHA256SUMS" "$base/SHA256SUMS"; then
    expected=$(awk -v f="$asset" '$2 == f || $2 == "*" f { print $1 }' "$tmp/SHA256SUMS")
    if command -v sha256sum >/dev/null 2>&1; then
        actual=$(sha256sum "$tmp/$asset" | awk '{ print $1 }')
    else
        actual=$(shasum -a 256 "$tmp/$asset" | awk '{ print $1 }')
    fi
    [ -n "$expected" ] && [ "$expected" = "$actual" ] || fail "checksum mismatch for $asset"
else
    say "warning: no SHA256SUMS in the release; skipping checksum"
fi

tar -xzf "$tmp/$asset" -C "$tmp"

if [ "$os" = macos ]; then
    dest="${PMC_APPDIR:-$HOME/Applications}"
    mkdir -p "$dest"
    rm -rf "$dest/$APP.app"
    mv "$tmp/$APP.app" "$dest/"
    # Downloaded by curl, so normally not quarantined; clear it just in case.
    xattr -dr com.apple.quarantine "$dest/$APP.app" 2>/dev/null || true
    say "Installed $dest/$APP.app"
    say "Open it from Launchpad or Finder, or run: open \"$dest/$APP.app\""
else
    prefix="${PMC_PREFIX:-$HOME/.local}"
    src="$tmp/$NAME-${version#v}-linux-x86_64"
    mkdir -p "$prefix/bin" "$prefix/share"
    install -m 755 "$src/bin/$NAME" "$prefix/bin/$NAME"
    cp -R "$src/share/." "$prefix/share/"
    command -v update-desktop-database >/dev/null 2>&1 &&
        update-desktop-database "$prefix/share/applications" 2>/dev/null || true
    say "Installed $prefix/bin/$NAME"
    case ":$PATH:" in
        *":$prefix/bin:"*) say "Run: $NAME" ;;
        *) say "Add $prefix/bin to your PATH, or run: $prefix/bin/$NAME" ;;
    esac
fi
