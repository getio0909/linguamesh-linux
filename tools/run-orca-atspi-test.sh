#!/usr/bin/env bash
set -euo pipefail

workspace=$(mktemp -d)
cleanup() {
  if [[ -n "${orca_pid:-}" ]]; then
    kill "$orca_pid" >/dev/null 2>&1 || true
    wait "$orca_pid" >/dev/null 2>&1 || true
  fi
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
  printf '%s\n' 'Orca AT-SPI fixture cleanup left temporary files.' >&2
}
trap cleanup EXIT

command -v orca >/dev/null 2>&1 || {
  printf '%s\n' 'Orca is required for the Linux accessibility fixture.' >&2
  exit 127
}
command -v xvfb-run >/dev/null 2>&1 || {
  printf '%s\n' 'xvfb-run is required for the Orca AT-SPI fixture.' >&2
  exit 127
}
python3 -c 'import pyatspi' >/dev/null 2>&1 || {
  printf '%s\n' 'python3-pyatspi is required for the Orca AT-SPI fixture.' >&2
  exit 127
}

cargo build --all-features --locked --bin linguamesh-linux

LINGUAMESH_ORCA_LOG="$workspace/orca-debug.log" \
LINGUAMESH_TEST_ORCA_ATSPI=1 \
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
    mkdir -p "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME" "$XDG_CONFIG_HOME/orca"
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
      terminate_inner_process "${orca_pid:-}"
      terminate_inner_process "${app_pid:-}"
      terminate_inner_process "${wm_pid:-}"
      terminate_inner_process "${a11y_pid:-}"
    }
    trap cleanup_inner EXIT
    /usr/libexec/at-spi-bus-launcher --launch-immediately --screen-reader=1 \
      >/tmp/linguamesh-orca-atspi-launcher.log 2>&1 &
    a11y_pid=$!
    xfwm4 --compositor=off >/tmp/linguamesh-orca-atspi-xfwm4.log 2>&1 &
    wm_pid=$!
    sleep 0.5
    orca -u "$XDG_CONFIG_HOME/orca" --replace --speech-system speechdispatcherfactory \
      --debug --debug-file "$LINGUAMESH_ORCA_LOG" \
      >/tmp/linguamesh-orca.log 2>&1 &
    orca_pid=$!
    target/debug/linguamesh-linux >/tmp/linguamesh-orca-app.log 2>&1 &
    app_pid=$!
    for _ in {1..200}; do
      if xdotool search --onlyvisible --name "^LinguaMesh$" >/dev/null 2>&1; then
        break
      fi
      sleep 0.1
    done
    app_window=$(xdotool search --onlyvisible --name "^LinguaMesh$" | head -n 1 || true)
    if [[ -z "$app_window" ]]; then
      cat /tmp/linguamesh-orca-app.log >&2 || true
      cat /tmp/linguamesh-orca.log >&2 || true
      printf "%s\n" "Orca AT-SPI fixture could not find the application window." >&2
      exit 1
    fi
    xdotool windowactivate --sync "$app_window" >/dev/null 2>&1 || true
    xdotool windowfocus --sync "$app_window" >/dev/null 2>&1 || true
    inspection_output=$(python3 tools/orca-atspi-inspect.py)
    printf '%s\n' "$inspection_output"
    grep -Fq "Orca AT-SPI fixture focused expected Stop control" <<<"$inspection_output" || {
      printf "%s\n" "Orca AT-SPI fixture did not confirm the expected Stop control." >&2
      exit 1
    }
    # Arabic CI 只断言可访问树和焦点路径，避免把不稳定的语音后端输出当作语言证据。
    if [[ "${LINGUAMESH_ORCA_REQUIRE_SPEECH:-1}" != "1" ]]; then
      printf "%s\n" "Orca AT-SPI fixture passed: expected Stop control was focused; speech generation was not required for this locale."
      exit 0
    fi
    xdotool windowactivate --sync "$app_window" >/dev/null 2>&1 || true
    xdotool key --window "$app_window" --clearmodifiers KP_Enter >/dev/null 2>&1 || true
    for _ in {1..200}; do
      if [[ -s "$LINGUAMESH_ORCA_LOG" ]] \
        && grep -Fq "SPEECH GENERATOR" "$LINGUAMESH_ORCA_LOG" \
        && grep -Fq "linguamesh-linux" "$LINGUAMESH_ORCA_LOG"; then
        printf "%s\n" "Orca AT-SPI fixture passed: the named control was inspected and Orca generated speech for the Linux application tree."
        exit 0
      fi
      if ! kill -0 "$orca_pid" >/dev/null 2>&1; then
        cat /tmp/linguamesh-orca.log >&2 || true
        cat "$LINGUAMESH_ORCA_LOG" >&2 || true
        printf "%s\n" "Orca AT-SPI fixture stopped before processing the named control." >&2
        exit 1
      fi
      sleep 0.1
    done
    cat "$LINGUAMESH_ORCA_LOG" >&2 || true
    printf "%s\n" "Orca AT-SPI fixture did not observe speech generation for the named control." >&2
    exit 1
  '
