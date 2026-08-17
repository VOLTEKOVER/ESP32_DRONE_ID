#!/usr/bin/env bash
# Fetches the official OpenDroneID C library at a pinned tag and replaces the
# vendored copy used by `opendroneid-sys`.
#
# Usage: update-opendroneid.sh <tag>
#
# This is the "deliberate, reviewed and verified" update path: the layout
# parity tests in `opendroneid-sys` (layout_probe.c vs the Rust FFI mirrors)
# and the byte-exact tests in `out-astm` gate the change. If the C struct
# layout or encoder behaviour changed, those tests fail and the FFI mirror in
# `external-libs/opendroneid-sys/src/lib.rs` must be updated in the same PR.
set -euo pipefail

REPO="opendroneid/opendroneid-core-c"
TAG="${1:?usage: update-opendroneid.sh <tag>}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VENDOR_DIR="${SCRIPT_DIR}/../external-libs/opendroneid-sys/vendor"

for f in opendroneid.c opendroneid.h; do
  echo "==> downloading ${REPO}@${TAG}/${f}"
  curl -fsSL "https://raw.githubusercontent.com/${REPO}/${TAG}/${f}" -o "${VENDOR_DIR}/${f}"
done

echo "==> vendored library updated to ${TAG}"
git -C "${SCRIPT_DIR}/.." diff --stat -- external-libs/opendroneid-sys
