#!/usr/bin/env bash
set -euo pipefail

capture=$(mktemp)
monitor_pid=""

cleanup() {
  if [[ -n "$monitor_pid" ]]; then
    kill "$monitor_pid" >/dev/null 2>&1 || true
    wait "$monitor_pid" >/dev/null 2>&1 || true
  fi
  rm -f "$capture"
}
trap cleanup EXIT

dbus-monitor --session "interface='org.gtk.Notifications'" >"$capture" 2>&1 &
monitor_pid=$!
sleep 1

GDK_BACKEND=x11 xvfb-run --auto-servernum \
  --server-args='-screen 0 1280x800x24' \
  cargo test --all-targets --all-features --locked \
  gtk_buttons_explicitly_connect_select_and_translate_with_session_credential \
  -- --exact --test-threads=1

for _ in {1..20}; do
  if grep -Fq 'member=AddNotification' "$capture"; then
    break
  fi
  sleep 0.1
done

grep -Fq 'member=AddNotification' "$capture"
grep -Fq 'Translation complete' "$capture"
grep -Fq 'The translated output is ready in LinguaMesh.' "$capture"
if grep -Fq 'Hello' "$capture" || grep -Fq '你好，LinguaMesh！' "$capture"; then
  printf 'Notification payload leaked source or translated content.\n' >&2
  exit 1
fi

printf 'Notification transport fixture passed with generic payload.\n'
