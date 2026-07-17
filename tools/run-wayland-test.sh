#!/usr/bin/env bash
set -euo pipefail

wayland_runtime_dir=""
weston_pid=""

# 仅删除本脚本创建的临时运行目录，并确保 Weston 不会遗留在后台。
cleanup() {
  local exit_status=$?

  trap - EXIT
  if [[ -n "$weston_pid" ]] && kill -0 "$weston_pid" 2>/dev/null; then
    kill "$weston_pid" 2>/dev/null || true
    for _ in {1..50}; do
      if ! kill -0 "$weston_pid" 2>/dev/null; then
        break
      fi
      sleep 0.1
    done
    if kill -0 "$weston_pid" 2>/dev/null; then
      kill -KILL "$weston_pid" 2>/dev/null || true
    fi
  fi
  if [[ -n "$weston_pid" ]]; then
    wait "$weston_pid" 2>/dev/null || true
  fi
  if [[ "$wayland_runtime_dir" == /tmp/linguamesh-wayland.* ]] &&
    [[ -d "$wayland_runtime_dir" ]]; then
    rm -rf -- "$wayland_runtime_dir"
  fi
  exit "$exit_status"
}

trap cleanup EXIT

# 将终止信号转换为稳定的失败状态，再由退出陷阱统一清理。
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

if ! command -v weston >/dev/null 2>&1; then
  printf '%s\n' 'Weston is required for the headless Wayland GTK test.' >&2
  exit 127
fi

if ! command -v cargo >/dev/null 2>&1; then
  printf '%s\n' 'Cargo is required for the headless Wayland GTK test.' >&2
  exit 127
fi

# 无论调用位置如何，都从仓库根目录运行 Cargo。
script_dir="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repository_dir="$(CDPATH= cd -- "$script_dir/.." && pwd)"
cd "$repository_dir"

wayland_runtime_dir="$(mktemp -d /tmp/linguamesh-wayland.XXXXXX)"
if [[ "$wayland_runtime_dir" != /tmp/linguamesh-wayland.* ]]; then
  printf '%s\n' 'The temporary Wayland runtime directory is outside the approved location.' >&2
  exit 1
fi
chmod 0700 "$wayland_runtime_dir"

export XDG_RUNTIME_DIR="$wayland_runtime_dir"
export WAYLAND_DISPLAY="linguamesh-wayland-test"
export GDK_BACKEND="wayland"
unset DISPLAY

weston_log="$wayland_runtime_dir/weston.log"

# 无头 Weston 只为当前测试提供隔离的 Wayland socket。
weston \
  --no-config \
  --backend=headless-backend.so \
  --renderer=pixman \
  --idle-time=0 \
  --socket="$WAYLAND_DISPLAY" \
  --log="$weston_log" &
weston_pid=$!

# 使用有限轮询等待 socket，避免启动失败时无限阻塞 CI。
wayland_socket="$wayland_runtime_dir/$WAYLAND_DISPLAY"
for _ in {1..100}; do
  if [[ -S "$wayland_socket" ]]; then
    break
  fi
  if ! kill -0 "$weston_pid" 2>/dev/null; then
    printf '%s\n' 'Weston exited before the Wayland socket became ready.' >&2
    if [[ -s "$weston_log" ]]; then
      sed -n '1,200p' "$weston_log" >&2
    fi
    exit 1
  fi
  sleep 0.1
done

if [[ ! -S "$wayland_socket" ]]; then
  printf '%s\n' 'Timed out waiting for the headless Wayland socket.' >&2
  if [[ -s "$weston_log" ]]; then
    sed -n '1,200p' "$weston_log" >&2
  fi
  exit 1
fi

printf '%s\n' 'Running the serialized GTK binary test on headless Wayland.'
cargo test --bin linguamesh-linux --all-features --locked -- --test-threads=1
