#!/usr/bin/env bash
set -euo pipefail

workspace=$(mktemp -d)
fixture="$workspace/fixture.txt"
log="$workspace/test.log"
test_pid=""
cleanup() {
  if [[ -n "$test_pid" ]]; then
    kill "$test_pid" >/dev/null 2>&1 || true
    wait "$test_pid" >/dev/null 2>&1 || true
  fi
  rm -rf "$workspace"
}
trap cleanup EXIT

printf 'GTK portal file chooser fixture content\n' >"$fixture"

LINGUAMESH_FILE_CHOOSER_FIXTURE="$fixture" LINGUAMESH_FILE_CHOOSER_LOG="$log" \
  xvfb-run --auto-servernum \
  --server-args='-screen 0 1280x800x24' \
  dbus-run-session -- bash -c '
    set -euo pipefail
    export XDG_CURRENT_DESKTOP=GNOME
    export GDK_BACKEND=x11
    cargo test --all-targets --all-features --locked --ignored \
      "tests::gtk_file_dialog_uses_portal_and_loads_selected_fixture" \
      -- --exact --test-threads=1 >"$LINGUAMESH_FILE_CHOOSER_LOG" 2>&1 &
    test_pid=$!
    chooser_window=""
    for _ in {1..150}; do
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
      printf "GTK file chooser dialog did not become visible.\n" >&2
      exit 1
    fi
    xdotool key --window "$chooser_window" ctrl+l
    xdotool type --window "$chooser_window" --delay 1 "$LINGUAMESH_FILE_CHOOSER_FIXTURE"
    xdotool key --window "$chooser_window" Return
    xdotool key --window "$chooser_window" Return
    if ! wait "$test_pid"; then
      cat "$LINGUAMESH_FILE_CHOOSER_LOG" >&2
      exit 1
    fi
    cat "$LINGUAMESH_FILE_CHOOSER_LOG"
  '
