#!/usr/bin/env bash
set -euo pipefail

workspace=$(mktemp -d)
cleanup() {
  if [[ -n "${app_pid:-}" ]]; then
    kill "$app_pid" >/dev/null 2>&1 || true
    wait "$app_pid" >/dev/null 2>&1 || true
  fi
  if [[ -n "${wm_pid:-}" ]]; then
    kill "$wm_pid" >/dev/null 2>&1 || true
    wait "$wm_pid" >/dev/null 2>&1 || true
  fi
  if [[ -n "${a11y_pid:-}" ]]; then
    kill "$a11y_pid" >/dev/null 2>&1 || true
    wait "$a11y_pid" >/dev/null 2>&1 || true
  fi
  # 清理可能由异步桌面服务延迟创建的临时文件。
  for _ in {1..20}; do
    if [[ ! -e "$workspace" ]]; then
      return
    fi
    find "$workspace" -depth -delete >/dev/null 2>&1 || true
    [[ ! -e "$workspace" ]] && return
    sleep 0.1
  done
  printf '%s\n' 'GTK AT-SPI fixture cleanup left temporary files.' >&2
}
trap cleanup EXIT

command -v python3 >/dev/null 2>&1 || {
  printf '%s\n' 'Python 3 is required for the GTK AT-SPI fixture.' >&2
  exit 127
}
python3 -c 'import pyatspi' >/dev/null 2>&1 || {
  printf '%s\n' 'python3-pyatspi is required for the GTK AT-SPI fixture.' >&2
  exit 127
}

cargo build --all-features --locked --bin linguamesh-linux

XDG_DATA_HOME="$workspace/data" \
XDG_CONFIG_HOME="$workspace/config" \
XDG_CACHE_HOME="$workspace/cache" \
  xvfb-run --auto-servernum \
  --server-args='-screen 0 1280x800x24' \
  dbus-run-session -- bash -c '
    set -euo pipefail
    export XDG_CURRENT_DESKTOP=GNOME
    export GDK_BACKEND=x11
    export GTK_A11Y=atspi
    mkdir -p "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME"
    terminate_inner_process() {
      local pid="${1:-}"
      if [[ -z "$pid" ]]; then
        return
      fi
      kill "$pid" >/dev/null 2>&1 || true
      for _ in {1..50}; do
        if ! kill -0 "$pid" >/dev/null 2>&1; then
          break
        fi
        sleep 0.1
      done
      kill -KILL "$pid" >/dev/null 2>&1 || true
      wait "$pid" >/dev/null 2>&1 || true
    }
    cleanup_inner() {
      terminate_inner_process "${app_pid:-}"
      terminate_inner_process "${wm_pid:-}"
      terminate_inner_process "${a11y_pid:-}"
    }
    trap cleanup_inner EXIT
    /usr/libexec/at-spi-bus-launcher --launch-immediately --screen-reader=1 \
      >/tmp/linguamesh-atspi-launcher.log 2>&1 &
    a11y_pid=$!
    xfwm4 --compositor=off >/tmp/linguamesh-atspi-xfwm4.log 2>&1 &
    wm_pid=$!
    sleep 0.5
    target/debug/linguamesh-linux >/tmp/linguamesh-atspi-app.log 2>&1 &
    app_pid=$!
    app_window=""
    for _ in {1..200}; do
      app_window=$(xdotool search --onlyvisible --pid "$app_pid" 2>/dev/null | head -n 1 || true)
      if [[ -n "$app_window" ]]; then
        break
      fi
      sleep 0.1
    done
    if [[ -z "$app_window" ]]; then
      cat /tmp/linguamesh-atspi-app.log >&2 || true
      printf "%s\n" "GTK AT-SPI fixture could not find the application window." >&2
      exit 1
    fi
    python3 tools/gtk-atspi-inspect.py
  '
