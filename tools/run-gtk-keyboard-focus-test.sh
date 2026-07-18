#!/usr/bin/env bash
set -euo pipefail

workspace=$(mktemp -d)
focus_log="$workspace/focus.log"
focus_start="$workspace/focus-start"
focus_coordinates="$workspace/focus-coordinates"
app_log="$workspace/app.log"
cleanup() {
  if [[ -n "${app_pid:-}" ]]; then
    kill "$app_pid" >/dev/null 2>&1 || true
    wait "$app_pid" >/dev/null 2>&1 || true
  fi
  rm -rf "$workspace"
}
trap cleanup EXIT

cargo build --all-features --locked --bin linguamesh-linux

LINGUAMESH_KEYBOARD_FOCUS_LOG="$focus_log" \
LINGUAMESH_KEYBOARD_FOCUS_START="$focus_start" \
LINGUAMESH_KEYBOARD_FOCUS_COORDINATES="$focus_coordinates" \
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
    xfwm4 --compositor=off >/tmp/linguamesh-xfwm4.log 2>&1 &
    wm_pid=$!
    sleep 0.5
    target/debug/linguamesh-linux >"$LINGUAMESH_KEYBOARD_FOCUS_LOG.app" 2>&1 &
    app_pid=$!
    app_window=""
    for _ in {1..120}; do
      app_window=$(xdotool search --onlyvisible --name "^LinguaMesh$" | head -n 1 || true)
      if [[ -n "$app_window" ]]; then
        break
      fi
      sleep 0.1
    done
    if [[ -z "$app_window" ]]; then
      cat "$LINGUAMESH_KEYBOARD_FOCUS_LOG.app" >&2
      printf "%s\n" "GTK keyboard fixture could not find the application window." >&2
      exit 1
    fi
    xdotool windowactivate --sync "$app_window" >/dev/null 2>&1 || true
    xdotool windowfocus --sync "$app_window" >/dev/null 2>&1 || true
    for _ in {1..240}; do
      if grep -Fxq "__ready__" "$LINGUAMESH_KEYBOARD_FOCUS_LOG"; then
        break
      fi
      sleep 0.1
    done
    if ! grep -Fxq "__ready__" "$LINGUAMESH_KEYBOARD_FOCUS_LOG"; then
      cat "$LINGUAMESH_KEYBOARD_FOCUS_LOG.app" >&2
      printf "%s\n" "GTK keyboard fixture did not reach the enabled provider form." >&2
      exit 1
    fi
    for _ in {1..50}; do
      if [[ -s "$LINGUAMESH_KEYBOARD_FOCUS_COORDINATES" ]]; then
        break
      fi
      sleep 0.1
    done
    xdotool windowactivate --sync "$app_window" >/dev/null 2>&1 || true
    xdotool windowfocus --sync "$app_window" >/dev/null 2>&1 || true
    read -r focus_x focus_y focus_width focus_height <"$LINGUAMESH_KEYBOARD_FOCUS_COORDINATES"
    window_geometry=$(xdotool getwindowgeometry --shell "$app_window")
    window_x=$(printf "%s\n" "$window_geometry" | grep "^X=" | cut -d= -f2)
    window_y=$(printf "%s\n" "$window_geometry" | grep "^Y=" | cut -d= -f2)
    focus_abs_x=$((window_x + focus_x + focus_width / 2))
    focus_abs_y=$((window_y + focus_y + focus_height / 2))
    printf "GTK keyboard fixture clicking provider field at %s,%s.\n" "$focus_abs_x" "$focus_abs_y"
    xdotool mousemove --sync "$focus_abs_x" "$focus_abs_y"
    xdotool click 1
    : >"$LINGUAMESH_KEYBOARD_FOCUS_START"
    sleep 0.1
    xdotool key --clearmodifiers alt+p
    sleep 0.1
    for _ in {1..8}; do
      xdotool key --clearmodifiers Shift+Tab
      sleep 0.04
    done
    for _ in {1..80}; do
      xdotool key --clearmodifiers Tab
      sleep 0.04
    done
    for _ in {1..24}; do
      xdotool key --clearmodifiers ctrl+Tab
      sleep 0.04
    done
    for _ in {1..240}; do
      if [[ -s "$LINGUAMESH_KEYBOARD_FOCUS_LOG" ]]; then
        break
      fi
      sleep 0.1
    done
    if [[ ! -s "$LINGUAMESH_KEYBOARD_FOCUS_LOG" ]]; then
      cat "$LINGUAMESH_KEYBOARD_FOCUS_LOG.app" >&2
      printf "%s\n" "GTK keyboard fixture did not observe a focused control after Tab input." >&2
      exit 1
    fi
    sleep 0.2
    kill "$app_pid" >/dev/null 2>&1 || true
    wait "$app_pid" >/dev/null 2>&1 || true
    kill "$wm_pid" >/dev/null 2>&1 || true
    wait "$wm_pid" >/dev/null 2>&1 || true
    required_widgets=(
      provider_name
      provider_endpoint
      provider_credential
      remember_profile
      connect
      source_editor
      output_editor
      open_source
      document_jobs
    )
    for widget in "${required_widgets[@]}"; do
      if ! grep -Fxq "$widget" "$LINGUAMESH_KEYBOARD_FOCUS_LOG"; then
        cat "$LINGUAMESH_KEYBOARD_FOCUS_LOG" >&2
        printf "GTK keyboard fixture never focused %s.\n" "$widget" >&2
        exit 1
      fi
    done
    printf "%s\n" "GTK keyboard focus fixture passed: Tab traversal reached the tested onboarding and workspace controls."
    cat "$LINGUAMESH_KEYBOARD_FOCUS_LOG"
  '
