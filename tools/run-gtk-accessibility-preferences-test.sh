#!/usr/bin/env bash
set -euo pipefail

# 将 GTK 无障碍偏好测试放到隔离显示与 DBus 会话，避免修改开发者桌面设置。
workspace=$(mktemp -d "${TMPDIR:-/tmp}/linguamesh-gtk-a11y.XXXXXX")
cleanup() {
  find "$workspace" -depth -delete >/dev/null 2>&1 || true
}
trap cleanup EXIT

XDG_DATA_HOME="$workspace/data" \
XDG_CONFIG_HOME="$workspace/config" \
XDG_CACHE_HOME="$workspace/cache" \
  xvfb-run --auto-servernum \
  --server-args='-screen 0 1280x800x24' \
  dbus-run-session -- bash -c '
    set -euo pipefail
    export GDK_BACKEND=x11
    export GTK_A11Y=none
    mkdir -p "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME"
    cargo test --all-features --locked \
      gtk_accessibility_preferences_follow_desktop_settings \
      -- --ignored --exact --nocapture
  '

printf '%s\n' 'GTK accessibility preference fixture passed: high contrast and reduced motion follow desktop settings.'
