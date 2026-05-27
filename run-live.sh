#!/usr/bin/env bash
set -euo pipefail

cargo build
sudo setcap cap_net_raw,cap_net_admin+ep ./target/debug/albion-accountant
exec ./target/debug/albion-accountant --live "$@"
