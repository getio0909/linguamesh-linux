#!/usr/bin/env bash
set -euo pipefail

workspace=$(mktemp -d)
fixture="$workspace/fixture.txt"
cleanup() {
  rm -rf "$workspace"
}
trap cleanup EXIT

printf 'portal file chooser fixture content\n' >"$fixture"

LINGUAMESH_FILE_CHOOSER_FIXTURE="$fixture" xvfb-run --auto-servernum \
  --server-args='-screen 0 1280x800x24' \
  dbus-run-session -- bash -c '
    set -euo pipefail
    export XDG_CURRENT_DESKTOP=GNOME
    export GDK_BACKEND=x11
    python3 tools/file-chooser-portal-test.py
  '
