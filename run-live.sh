#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
binary="$repo_root/target/debug/albion"

cargo build --manifest-path "$repo_root/Cargo.toml"
sudo setcap cap_net_raw,cap_net_admin+ep "$binary"
exec "$binary" --live "$@"
