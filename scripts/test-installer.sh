#!/usr/bin/env sh
# Offline installer tests for scripts/install.sh (Linux/macOS).
# Uses a local fixture directory — no GitHub network required.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
installer="$root/install.sh"
pass=0

assert_fail() {
  label=$1
  shift
  if "$@" >/tmp/original-install-test.out 2>/tmp/original-install-test.err; then
    echo "FAIL $label (expected non-zero)" >&2
    exit 1
  fi
  echo "PASS: $label"
  pass=$((pass + 1))
}

assert_fail 'invalid version rejected' sh "$installer" '../invalid'

# Detect asset name the same way install.sh does.
case "$(uname -s)/$(uname -m)" in
  Linux/x86_64) asset=original-linux-x86_64 ;;
  Darwin/arm64) asset=original-macos-aarch64 ;;
  Darwin/x86_64) asset=original-macos-x86_64 ;;
  *)
    echo "SKIP happy-path: unsupported platform $(uname -s)/$(uname -m)"
    echo "Rejection tests passed ($pass)."
    exit 0
    ;;
esac

fixture=$(mktemp -d)
bindir=$(mktemp -d)
trap 'rm -rf -- "$fixture" "$bindir"' EXIT HUP INT TERM

payload="original-fixture-$(date +%s)-$$"
printf '%s' "$payload" >"$fixture/$asset"
if command -v sha256sum >/dev/null 2>&1; then
  (cd "$fixture" && sha256sum "$asset" >"$asset.sha256")
else
  (cd "$fixture" && shasum -a 256 "$asset" >"$asset.sha256")
fi

ORIGINAL_INSTALL_BASE=$fixture \
ORIGINAL_INSTALL_BIN=$bindir \
ORIGINAL_INSTALL_NO_PATH=1 \
  sh "$installer" v0.3.3

test -x "$bindir/original"
installed=$(cat "$bindir/original")
test "$installed" = "$payload"
echo 'PASS: local fixture installs with SHA-256 check'
pass=$((pass + 1))

# mismatch
printf '%s  %s\n' '0000000000000000000000000000000000000000000000000000000000000000' "$asset" >"$fixture/$asset.sha256"
assert_fail 'checksum mismatch rejected' \
  env ORIGINAL_INSTALL_BASE=$fixture ORIGINAL_INSTALL_BIN=$bindir ORIGINAL_INSTALL_NO_PATH=1 \
  sh "$installer"

echo "All installer tests passed ($pass)."
