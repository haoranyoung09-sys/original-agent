#!/usr/bin/env sh
# Install prebuilt `original` from GitHub Releases (no Rust required).
# Usage:
#   curl -fsSL .../install.sh | sh
#   sh install.sh v0.3.5
# Overrides (for tests / mirrors):
#   ORIGINAL_INSTALL_BASE  — HTTPS base URL, or a local directory with assets
#   ORIGINAL_INSTALL_BIN   — install dir (default: ~/.local/bin)
#   ORIGINAL_INSTALL_NO_PATH=1 — skip PATH hint

set -eu
version=${1:-latest}
case "$version" in
  latest) ;;
  v[0-9]*.[0-9]*.[0-9]*) ;;
  *) echo 'Expected latest or vX.Y.Z' >&2; exit 1 ;;
esac
case "$version" in
  *[!a-zA-Z0-9.-]*) echo 'Invalid version characters' >&2; exit 1 ;;
esac

case "$(uname -s)/$(uname -m)" in
  Linux/x86_64) asset=original-linux-x86_64 ;;
  Darwin/arm64) asset=original-macos-aarch64 ;;
  Darwin/x86_64) asset=original-macos-x86_64 ;;
  *) echo 'Unsupported platform or architecture' >&2; exit 1 ;;
esac

repo=haoranyoung09-sys/original-agent
if [ -n "${ORIGINAL_INSTALL_BASE:-}" ]; then
  base=${ORIGINAL_INSTALL_BASE%/}
else
  if [ "$version" = latest ]; then
    base="https://github.com/$repo/releases/latest/download"
  else
    base="https://github.com/$repo/releases/download/$version"
  fi
fi

bin=${ORIGINAL_INSTALL_BIN:-"$HOME/.local/bin"}
mkdir -p "$bin"

tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT HUP INT TERM

missing_release_help() {
  url=$1
  cat >&2 <<MSG
Release asset not found (HTTP 404): $url

GitHub has no usable prebuilt assets yet (empty Releases or missing $asset).
The no-Rust curl|sh installer cannot succeed until a Release with attachments is published.

Options:
  1) Push tag matching Cargo.toml (e.g. v0.3.5), wait for Actions \"Release binaries\", retry
  2) With a local cargo build: copy target/release/original to ~/.local/bin/original
  3) With Rust: cargo install --locked --path . --force

See: https://github.com/$repo/releases
MSG
}

fetch() {
  src_name=$1
  dest=$2
  if [ -d "$base" ]; then
    if [ ! -f "$base/$src_name" ]; then
      echo "Local release asset not found: $base/$src_name" >&2
      exit 1
    fi
    cp "$base/$src_name" "$dest"
  else
    if ! curl --fail --location --retry 2 --retry-all-errors --retry-max-time 90 \
      --connect-timeout 10 --max-time 120 \
      --proto '=https' --proto-redir '=https' --tlsv1.2 \
      "$base/$src_name" -o "$dest"; then
      missing_release_help "$base/$src_name"
      exit 1
    fi
  fi
}

fetch "$asset" "$tmp/$asset"
fetch "$asset.sha256" "$tmp/$asset.sha256"

checksum_line=$(tr -d '\r' <"$tmp/$asset.sha256" | head -n 1)
hash=${checksum_line%% *}
rest=${checksum_line#"$hash"}
if [ -z "$rest" ] || [ "$checksum_line" = "$hash" ]; then
  printf '%s  %s\n' "$hash" "$asset" >"$tmp/$asset.sha256"
fi

(
  cd "$tmp"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum -c "$asset.sha256"
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 -c "$asset.sha256"
  else
    echo 'Need sha256sum or shasum to verify the download' >&2
    exit 1
  fi
)

install -m 755 "$tmp/$asset" "$bin/original"
printf '%s\n' "Installed $bin/original"

if [ "${ORIGINAL_INSTALL_NO_PATH:-}" = 1 ]; then
  printf '%s\n' 'PATH hint skipped (ORIGINAL_INSTALL_NO_PATH=1).'
else
  case ":$PATH:" in
    *":$bin:"*) ;;
    *)
      printf '%s\n' "Add $bin to PATH, for example:"
      printf '%s\n' "  export PATH=\"$bin:\$PATH\""
      ;;
  esac
fi
