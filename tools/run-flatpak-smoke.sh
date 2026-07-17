#!/usr/bin/env bash
set -euo pipefail

bundle_path=${1:?usage: run-flatpak-smoke.sh BUNDLE}
app_id=dev.linguamesh.LinguaMesh
runtime_dir=$(mktemp -d)
run_status=0

cleanup() {
  flatpak uninstall --user --noninteractive "$app_id" >/dev/null 2>&1 || true
  find "$runtime_dir" -mindepth 1 -delete
  rmdir "$runtime_dir"
}
trap cleanup EXIT

chmod 700 "$runtime_dir"
flatpak install --user --noninteractive "$bundle_path"
runtime=$(flatpak info --user --show-runtime "$app_id")
if [[ "$runtime" != org.gnome.Platform/x86_64/49 ]]; then
  printf 'Unexpected Flatpak runtime: %s\n' "$runtime" >&2
  exit 1
fi

XDG_RUNTIME_DIR="$runtime_dir" GDK_BACKEND=x11 dbus-run-session -- \
  xvfb-run --auto-servernum --server-args='-screen 0 1280x800x24' \
  timeout --signal=TERM --kill-after=5s 15s \
  flatpak run --user "$app_id" || run_status=$?

if [[ "$run_status" -ne 124 ]]; then
  printf 'Flatpak sandbox launch exited unexpectedly with status %s.\n' "$run_status" >&2
  exit "$run_status"
fi

printf 'Flatpak sandbox smoke passed for %s with runtime %s.\n' "$app_id" "$runtime"
