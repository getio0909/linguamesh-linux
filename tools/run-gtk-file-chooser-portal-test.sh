#!/usr/bin/env bash
set -euo pipefail

workspace=$(mktemp -d)
fixture="$workspace/fixture.txt"
log="$workspace/test.log"
cleanup() {
  rm -rf "$workspace"
}
trap cleanup EXIT

printf 'GTK portal application fixture content\n' >"$fixture"
cargo build --all-features --locked --bin linguamesh-linux

LINGUAMESH_FILE_CHOOSER_FIXTURE="$fixture" \
LINGUAMESH_FILE_CHOOSER_LOG="$log" \
XDG_DATA_HOME="$workspace/data" \
XDG_CONFIG_HOME="$workspace/config" \
XDG_CACHE_HOME="$workspace/cache" \
  xvfb-run --auto-servernum \
  --server-args='-screen 0 1280x800x24' \
  dbus-run-session -- bash -c '
    set -euo pipefail
    export XDG_CURRENT_DESKTOP=GNOME
    export GDK_BACKEND=x11
    mkdir -p "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME"
    target/debug/linguamesh-linux --test-file-dialog >"$LINGUAMESH_FILE_CHOOSER_LOG" 2>&1 &
    app_pid=$!
    chooser_window=""
    for _ in {1..200}; do
      for pattern in "Open text file" "Open File" "Select a File"; do
        chooser_window=$(xdotool search --onlyvisible --name "$pattern" 2>/dev/null | tail -n 1 || true)
        if [[ -n "$chooser_window" ]]; then
          break 2
        fi
      done
      sleep 0.1
    done
    if [[ -z "$chooser_window" ]]; then
      cat "$LINGUAMESH_FILE_CHOOSER_LOG" >&2
      printf "%s\n" "GTK file chooser application dialog did not become visible." >&2
      kill "$app_pid" >/dev/null 2>&1 || true
      wait "$app_pid" >/dev/null 2>&1 || true
      exit 1
    fi
    xdotool key --window "$chooser_window" ctrl+l
    xdotool type --window "$chooser_window" --delay 1 "$LINGUAMESH_FILE_CHOOSER_FIXTURE"
    xdotool key --window "$chooser_window" Return
    xdotool key --window "$chooser_window" Return
    if ! wait "$app_pid"; then
      cat "$LINGUAMESH_FILE_CHOOSER_LOG" >&2
      exit 1
    fi
    cat "$LINGUAMESH_FILE_CHOOSER_LOG"
  '
