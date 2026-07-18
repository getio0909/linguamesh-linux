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
  rm -rf -- "$workspace"
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
    cleanup_inner() {
      kill "${app_pid:-}" "${wm_pid:-}" "${a11y_pid:-}" >/dev/null 2>&1 || true
      wait "${app_pid:-}" "${wm_pid:-}" "${a11y_pid:-}" >/dev/null 2>&1 || true
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
    for _ in {1..200}; do
      if xdotool search --onlyvisible --name "^LinguaMesh$" >/dev/null 2>&1; then
        break
      fi
      sleep 0.1
    done
    if ! xdotool search --onlyvisible --name "^LinguaMesh$" >/dev/null 2>&1; then
      cat /tmp/linguamesh-atspi-app.log >&2 || true
      printf "%s\n" "GTK AT-SPI fixture could not find the application window." >&2
      exit 1
    fi
    python3 tools/gtk-atspi-inspect.py
  '
