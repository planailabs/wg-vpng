#!/usr/bin/env bash
# Warm the xzar.plan.ai binary cache with wg-vpng's heavy nix outputs (devshell,
# server package, OCI image, CI image) so later CI jobs — and deployments —
# substitute instead of rebuilding. No-op when XZAR_TOKEN is absent (e.g. on
# unprotected pipelines). Run via: nix develop -c bash xzar.sh
set -euo pipefail

if [ -z "${XZAR_TOKEN:-}" ]; then
  echo "XZAR_TOKEN is not set; skipping cache prebuild/upload."
  echo "This is expected for pipelines without protected CI variables."
  exit 0
fi

# ── Configure the xzar plan.ai cache ────────────────────────────────
xzar config add-server planai https://xzar.plan.ai "$XZAR_TOKEN"

upload() {
  # Retry: the cache can briefly 5xx under load.
  while ! xzar --server planai upload --pin "$1" --desc "$(readlink -f "$2")" --leave-after-abandon 1m "$2"; do
    sleep 2
  done
}

build_and_upload() {
  local attr="$1" pin="$2"
  nix build ".#${attr}" -o "result-${attr//\//-}" -L
  upload "$pin" "result-${attr//\//-}"
  rm -f "result-${attr//\//-}"
}

# devShell (biggest single win for CI), the server package, and both images.
nix build ".#devShells.x86_64-linux.default" -o result-devshell -L
upload "wg-vpng/devshell" result-devshell
rm -f result-devshell

build_and_upload "wg-vpng-server" "wg-vpng/server"
build_and_upload "docker"         "wg-vpng/docker"
build_and_upload "image"          "wg-vpng/image"
