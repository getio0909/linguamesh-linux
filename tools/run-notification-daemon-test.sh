#!/usr/bin/env bash
set -euo pipefail

capture=$(mktemp)
server_log=$(mktemp)
monitor_pid=""

cleanup() {
  if [[ -n "$monitor_pid" ]]; then
    kill "$monitor_pid" >/dev/null 2>&1 || true
    wait "$monitor_pid" >/dev/null 2>&1 || true
  fi
  rm -f "$capture" "$server_log"
}
trap cleanup EXIT

dbus-monitor --session "interface='org.freedesktop.Notifications',member='Notify'" >"$capture" 2>&1 &
monitor_pid=$!

LINGUAMESH_NOTIFICATION_SERVER_LOG="$server_log" xvfb-run --auto-servernum \
  --server-args='-screen 0 1280x800x24' bash -c '
    set -euo pipefail
    dunst -conf /dev/null >"$LINGUAMESH_NOTIFICATION_SERVER_LOG" 2>&1 &
    server_pid=$!
    cleanup_server() {
      kill "$server_pid" >/dev/null 2>&1 || true
      wait "$server_pid" >/dev/null 2>&1 || true
    }
    trap cleanup_server EXIT

    server_ready=0
    for _ in {1..30}; do
      if gdbus call --session \
        --dest org.freedesktop.Notifications \
        --object-path /org/freedesktop/Notifications \
        --method org.freedesktop.Notifications.GetCapabilities >/dev/null 2>&1; then
        server_ready=1
        break
      fi
      sleep 0.1
    done
    if [[ "$server_ready" -ne 1 ]]; then
      cat "$LINGUAMESH_NOTIFICATION_SERVER_LOG" >&2
      printf "Notification daemon did not acquire the D-Bus service.\n" >&2
      exit 1
    fi

    GDK_BACKEND=x11 cargo test --all-targets --all-features --locked \
      "tests::gtk_buttons_explicitly_connect_select_and_translate_with_session_credential" \
      -- --exact --test-threads=1
  '

for _ in {1..20}; do
  if grep -Fq 'member=Notify' "$capture"; then
    break
  fi
  sleep 0.1
done

grep -Fq 'member=Notify' "$capture"
grep -Fq 'Translation complete' "$capture"
grep -Fq 'The translated output is ready in LinguaMesh.' "$capture"
if grep -Fq 'Hello' "$capture" || grep -Fq '你好，LinguaMesh！' "$capture"; then
  printf 'Notification daemon payload leaked source or translated content.\n' >&2
  exit 1
fi

printf 'Notification daemon delivery fixture passed with generic payload.\n'
