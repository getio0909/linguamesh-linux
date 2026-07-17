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
  start_keyring() {
    printf "fixture-passphrase\\n" | gnome-keyring-daemon --foreground --components=secrets --unlock >/dev/null 2>&1 &
    keyring_pid=$!
    sleep 1
    gdbus call --session \
      --dest org.freedesktop.secrets \
      --object-path /org/freedesktop/secrets \
      --method org.freedesktop.Secret.Service.SetAlias \
      default /org/freedesktop/secrets/collection/login >/dev/null
  }

  stop_keyring() {
    kill "$keyring_pid" 2>/dev/null || true
    wait "$keyring_pid" 2>/dev/null || true
  }

  trap stop_keyring EXIT
  start_keyring
  cargo test --features gui --lib secret_service::tests::secret_service_persistent_store_for_restart \
    --locked -- --ignored --exact
  cargo test --features gui --lib worker::tests::persistent_secret_onboarding_connects_without_credential_reentry \
    --locked -- --ignored --exact
  GDK_BACKEND=x11 xvfb-run --auto-servernum \
    --server-args="-screen 0 1280x800x24" \
    cargo test --features gui --bin linguamesh-linux \
    gtk_remembered_credential_uses_secret_service_and_clears_the_form \
    --locked -- --ignored --exact
  lock_objects="[objectpath $(printf "\\047")/org/freedesktop/secrets/collection/login$(printf "\\047")]"
  gdbus call --session \
    --dest org.freedesktop.secrets \
    --object-path /org/freedesktop/secrets \
    --method org.freedesktop.Secret.Service.Lock \
    "$lock_objects" >/dev/null
  cargo test --features gui --lib secret_service::tests::secret_service_locked_item_fails_closed \
    --locked -- --ignored --exact
  stop_keyring
  start_keyring
  cargo test --features gui --lib secret_service::tests::secret_service_persistent_resolves_after_daemon_restart \
    --locked -- --ignored --exact
  cargo test --features gui --lib secret_service::tests::secret_service_round_trip_and_cleanup \
    --locked -- --ignored --exact
'
