#!/usr/bin/env bash
set -euo pipefail

# 使用隔离的临时主目录，避免测试触碰开发者的桌面密钥环。
test_home=$(mktemp -d "${TMPDIR:-/tmp}/linguamesh-secret-service.XXXXXX")

cleanup() {
  find "$test_home" -type f -delete
  find "$test_home" -depth -type d -empty -delete
}
trap cleanup EXIT

export XDG_CONFIG_HOME="$test_home/.config"
export XDG_DATA_HOME="$test_home/.local/share"
unset GNOME_KEYRING_CONTROL

dbus-run-session -- bash -c '
  set -euo pipefail
  gnome-keyring-daemon --foreground --components=secrets >/dev/null 2>&1 &
  keyring_pid=$!
  trap "kill \"$keyring_pid\" 2>/dev/null || true" EXIT
  sleep 1
  gdbus call --session \
    --dest org.freedesktop.secrets \
    --object-path /org/freedesktop/secrets \
    --method org.freedesktop.Secret.Service.SetAlias \
    default /org/freedesktop/secrets/collection/session >/dev/null
  cargo test --features gui --lib secret_service::tests::secret_service_round_trip_and_cleanup \
    --locked -- --ignored --exact
'
