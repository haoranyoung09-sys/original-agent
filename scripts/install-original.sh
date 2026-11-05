#!/usr/bin/env sh
set -eu
project_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cargo install --locked --path "$project_root" --force
printf '%s\n' 'Installed original. Ensure ~/.cargo/bin is on PATH.'
