#!/usr/bin/env bash
set -euo pipefail

capture=$(mktemp)
monitor_pid=""
service_pid=""

cleanup() {
  if [[ -n "$monitor_pid" ]]; then
    kill "$monitor_pid" >/dev/null 2>&1 || true
    wait "$monitor_pid" >/dev/null 2>&1 || true
  fi
  if [[ -n "$service_pid" ]]; then
    kill "$service_pid" >/dev/null 2>&1 || true
    wait "$service_pid" >/dev/null 2>&1 || true
  fi
  rm -f "$capture" "$capture.payload"
}
trap cleanup EXIT

dbus-monitor --session "interface='org.freedesktop.Notifications',member='Notify'" >"$capture" 2>&1 &
monitor_pid=$!
LINGUAMESH_NOTIFICATION_CAPTURE="$capture.payload" python3 tools/notification-service.py &
service_pid=$!
service_ready=0
for _ in {1..20}; do
  if gdbus call --session \
    --dest org.freedesktop.Notifications \
    --object-path /org/freedesktop/Notifications \
    --method org.freedesktop.Notifications.GetCapabilities >/dev/null 2>&1; then
    service_ready=1
    break
  fi
  sleep 0.1
done
if [[ "$service_ready" -ne 1 ]]; then
  printf 'Notification fixture service did not start.\n' >&2
  exit 1
fi

GDK_BACKEND=x11 xvfb-run --auto-servernum \
  --server-args='-screen 0 1280x800x24' \
  cargo test --all-targets --all-features --locked \
  'tests::gtk_buttons_explicitly_connect_select_and_translate_with_session_credential' \
  -- --exact --test-threads=1

for _ in {1..20}; do
  if grep -Fq 'member=AddNotification' "$capture"; then
    break
  fi
  sleep 0.1
done

grep -Fq 'member=Notify' "$capture"
grep -Fq 'summary=Translation complete' "$capture.payload"
grep -Fq 'body=The translated output is ready in LinguaMesh.' "$capture.payload"
if grep -Fq 'Hello' "$capture.payload" || grep -Fq '你好，LinguaMesh！' "$capture.payload"; then
  printf 'Notification payload leaked source or translated content.\n' >&2
  exit 1
fi

printf 'Notification transport fixture passed with generic payload.\n'
