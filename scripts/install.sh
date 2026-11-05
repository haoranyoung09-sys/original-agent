#!/usr/bin/env sh
set -eu
version=${1:-latest}
case "$version" in latest) ;; v[0-9]*.[0-9]*.[0-9]*) ;; *) echo 'Expected latest or vX.Y.Z' >&2; exit 1;; esac
case "$version" in *[!a-zA-Z0-9.-]*) exit 1;; esac
case "$(uname -s)/$(uname -m)" in
  Linux/x86_64) asset=original-linux-x86_64 ;;
  Darwin/arm64) asset=original-macos-aarch64 ;;
  Darwin/x86_64) asset=original-macos-x86_64 ;;
  *) echo 'Unsupported platform or architecture' >&2; exit 1 ;;
esac
repo=haoranyoung09-sys/original-agent
if [ "$version" = latest ]; then
  base="https://github.com/$repo/releases/latest/download"
else
  base="https://github.com/$repo/releases/download/$version"
fi
tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT HUP INT TERM
curl --fail --location --proto '=https' --tlsv1.2 "$base/$asset" -o "$tmp/$asset"
curl --fail --location --proto '=https' --tlsv1.2 "$base/$asset.sha256" -o "$tmp/$asset.sha256"
(cd "$tmp"; if command -v sha256sum >/dev/null 2>&1; then sha256sum -c "$asset.sha256"; else shasum -a 256 -c "$asset.sha256"; fi)
bin="$HOME/.local/bin"
mkdir -p "$bin"
install -m 755 "$tmp/$asset" "$bin/original"
printf '%s\n' "Installed $bin/original"
case ":$PATH:" in *":$bin:"*) ;; *) printf '%s\n' 'Add ~/.local/bin to PATH: export PATH="$HOME/.local/bin:$PATH"';; esac
