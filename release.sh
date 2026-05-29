#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: ./release.sh [--skip-tests]

Builds a Linux release archive for albion-accountant in ./dist.

Options:
  --skip-tests   Build the release without running cargo test first.
USAGE
}

skip_tests=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-tests)
      skip_tests=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "ERROR: unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$repo_root"

package_name="$(awk -F '"' '/^name = / { print $2; exit }' Cargo.toml)"
version="$(awk -F '"' '/^version = / { print $2; exit }' Cargo.toml)"
target_triple="$(rustc -vV | awk '/^host:/ { print $2 }')"
release_name="${package_name}-v${version}-${target_triple}"
dist_dir="$repo_root/dist"
stage_dir="$dist_dir/$release_name"
archive_path="$dist_dir/$release_name.tar.gz"
checksum_path="$archive_path.sha256"

if [[ "$target_triple" != *linux* ]]; then
  echo "ERROR: this release script packages the Linux build; current host is $target_triple" >&2
  exit 1
fi

echo "==> Preparing release $release_name"
rm -rf "$stage_dir" "$archive_path" "$checksum_path"
mkdir -p "$stage_dir"

if [[ "$skip_tests" -eq 0 ]]; then
  echo "==> Running tests"
  cargo test
fi

echo "==> Building web app"
if [[ ! -d webapp/node_modules ]]; then
  (cd webapp && npm install)
fi
(cd webapp && npm run build)

echo "==> Building optimized binary"
cargo build --release

echo "==> Staging files"
install -m 755 "target/release/$package_name" "$stage_dir/$package_name"
install -m 644 README.md "$stage_dir/README.md"
install -m 644 hosts.txt "$stage_dir/hosts.txt"
install -m 644 .env.example "$stage_dir/.env.example"

cat > "$stage_dir/INSTALL.md" <<'INSTALL'
# Albion Accountant Release Install

This Linux build requires packet-capture privileges and Ubuntu tray dependencies:

```bash
sudo apt install libgtk-3-0 libayatana-appindicator3-1
```

Allow the binary to capture packets without running as root:

```bash
sudo setcap cap_net_raw,cap_net_admin+ep ./albion-accountant
```

Run from this directory so `hosts.txt` can be found:

```bash
./albion-accountant
```

Copy `.env.example` to `.env` and fill in the Google Sheets settings when Sheets upload is wanted.
INSTALL

echo "==> Creating archive"
tar -C "$dist_dir" -czf "$archive_path" "$release_name"
sha256sum "$archive_path" > "$checksum_path"

echo "==> Release complete"
echo "Archive:  $archive_path"
echo "Checksum: $checksum_path"
