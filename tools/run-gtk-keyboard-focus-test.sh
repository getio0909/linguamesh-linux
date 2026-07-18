#!/usr/bin/env bash
set -euo pipefail

workspace=$(mktemp -d)
focus_log="$workspace/focus.log"
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
    xdotool key --window "$app_window" alt+p
    sleep 0.1
    for _ in {1..8}; do
      xdotool key --window "$app_window" Shift+Tab
      sleep 0.04
    done
    for _ in {1..80}; do
      xdotool key --window "$app_window" Tab
      sleep 0.04
    done
    for _ in {1..12}; do
      xdotool key --window "$app_window" ctrl+Tab
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
    required_widgets=(
      provider_name
      provider_endpoint
      provider_credential
      remember_profile
      connect
      source_locale
      target_locale
      glossary
      incognito
      history_enabled
      memory_enabled
      theme
      locale
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
    printf "%s\n" "GTK keyboard focus fixture passed: Tab traversal reached onboarding and workspace controls."
    cat "$LINGUAMESH_KEYBOARD_FOCUS_LOG"
  '
