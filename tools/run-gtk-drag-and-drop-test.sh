#!/usr/bin/env bash
set -euo pipefail

workspace=$(mktemp -d)
fixture="$workspace/fixture.txt"
coordinates="$workspace/coordinates.txt"
log="$workspace/test.log"
cleanup() {
  rm -rf "$workspace"
}
trap cleanup EXIT

printf 'GTK drag-and-drop application fixture content\n' >"$fixture"
cargo build --all-features --locked --bin linguamesh-linux

LINGUAMESH_FILE_DROP_FIXTURE="$fixture" \
LINGUAMESH_FILE_DROP_COORDINATES="$coordinates" \
LINGUAMESH_FILE_DROP_LOG="$log" \
XDG_DATA_HOME="$workspace/data" \
XDG_CONFIG_HOME="$workspace/config" \
XDG_CACHE_HOME="$workspace/cache" \
  xvfb-run --auto-servernum \
  --server-args='-screen 0 1280x800x24' \
  dbus-run-session -- bash -c '
    set -euo pipefail
    export XDG_CURRENT_DESKTOP=GNOME
    export GDK_BACKEND=x11
    export LINGUAMESH_TEST_FILE_DROP=1
    mkdir -p "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME"
    target/debug/linguamesh-linux >"$LINGUAMESH_FILE_DROP_LOG" 2>&1 &
    app_pid=$!
    for _ in {1..120}; do
      if [[ -s "$LINGUAMESH_FILE_DROP_COORDINATES" ]]; then
        break
      fi
      sleep 0.1
    done
    if [[ ! -s "$LINGUAMESH_FILE_DROP_COORDINATES" ]]; then
      cat "$LINGUAMESH_FILE_DROP_LOG" >&2
      printf "%s\n" "GTK drag-and-drop fixture did not receive widget coordinates." >&2
      kill "$app_pid" >/dev/null 2>&1 || true
      wait "$app_pid" >/dev/null 2>&1 || true
      exit 1
    fi
    app_window=$(xdotool search --onlyvisible --name "LinguaMesh" | tail -n 1 || true)
    if [[ -z "$app_window" ]]; then
      cat "$LINGUAMESH_FILE_DROP_LOG" >&2
      printf "%s\n" "GTK drag-and-drop fixture could not find the application window." >&2
      kill "$app_pid" >/dev/null 2>&1 || true
      wait "$app_pid" >/dev/null 2>&1 || true
      exit 1
    fi
    read -r source_x source_y source_width source_height target_x target_y target_width target_height <"$LINGUAMESH_FILE_DROP_COORDINATES"
    printf "%s\n" "Widget coordinates: $(cat "$LINGUAMESH_FILE_DROP_COORDINATES")"
    eval "$(xdotool getwindowgeometry --shell "$app_window")"
    printf "%s\n" "Window geometry: $X $Y $WIDTH $HEIGHT"
    source_abs_x=$((X + source_x + source_width / 2))
    source_abs_y=$((Y + source_y + source_height / 2))
    target_abs_x=$((X + target_x + target_width / 2))
    target_abs_y=$((Y + target_y + target_height / 2))
    xdotool mousemove --sync "$source_abs_x" "$source_abs_y"
    xdotool getmouselocation --shell
    xdotool mousedown 1
    sleep 0.3
    xdotool mousemove --sync "$((source_abs_x + 24))" "$((source_abs_y + 24))"
    sleep 0.2
    xdotool mousemove --sync "$target_abs_x" "$target_abs_y"
    sleep 0.6
    xdotool mouseup 1
    if ! wait "$app_pid"; then
      cat "$LINGUAMESH_FILE_DROP_LOG" >&2
      exit 1
    fi
    if ! grep -q "GTK drag-and-drop application fixture passed:" "$LINGUAMESH_FILE_DROP_LOG"; then
      cat "$LINGUAMESH_FILE_DROP_LOG" >&2
      exit 1
    fi
    cat "$LINGUAMESH_FILE_DROP_LOG"
  '
