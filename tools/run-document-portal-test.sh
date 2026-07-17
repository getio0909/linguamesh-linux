#!/usr/bin/env bash
set -euo pipefail

workspace=$(mktemp -d)
data_home="$workspace/data"
fixture="$workspace/input.txt"

cleanup() {
  find "$workspace" -mindepth 1 -delete
  rmdir "$workspace"
}
trap cleanup EXIT

chmod 700 "$workspace"
mkdir -m 700 "$data_home"
printf 'portal lease fixture content\n' >"$fixture"

XDG_CURRENT_DESKTOP=GNOME XDG_DATA_HOME="$data_home" dbus-run-session -- \
  python3 tools/document-portal-test.py "$fixture"
