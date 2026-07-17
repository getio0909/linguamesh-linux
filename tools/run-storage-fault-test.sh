#!/usr/bin/env bash
set -euo pipefail

# Ubuntu CI 无法使用非特权挂载时，只让根协调器挂载并立即降回调用用户执行测试。
if [[ "${1:-}" == "--root-mount-runner" ]]; then
  if [[ "$#" -ne 5 || "$EUID" -ne 0 ]]; then
    printf '%s\n' 'The root mount runner received an invalid invocation.' >&2
    exit 2
  fi
  caller_uid="$2"
  caller_gid="$3"
  test_binary="$4"
  test_name="$5"
  if [[ ! "$caller_uid" =~ ^[0-9]+$ || ! "$caller_gid" =~ ^[0-9]+$ ]]; then
    printf '%s\n' 'The root mount runner requires numeric caller IDs.' >&2
    exit 2
  fi
  if [[ "$test_binary" != /* || ! -x "$test_binary" ]]; then
    printf '%s\n' 'The root mount runner requires an absolute executable test path.' >&2
    exit 2
  fi
  for command in env mktemp mount rmdir setpriv umount; do
    if ! command -v "$command" >/dev/null 2>&1; then
      printf 'Required root mount command is unavailable: %s\n' "$command" >&2
      exit 127
    fi
  done

  fault_directory="$(mktemp -d /tmp/linguamesh-linux-runtime-fault-root.XXXXXX)"
  mounted=false
  cleanup_root_mount() {
    cleanup_status=$?
    trap - EXIT
    if [[ "$mounted" == true ]] && ! umount "$fault_directory"; then
      printf '%s\n' 'The root-managed fault filesystem could not be unmounted.' >&2
      cleanup_status=1
    fi
    if [[ -d "$fault_directory" ]] && ! rmdir "$fault_directory"; then
      printf '%s\n' 'The root-managed fault directory could not be removed.' >&2
      cleanup_status=1
    fi
    exit "$cleanup_status"
  }
  trap cleanup_root_mount EXIT
  mount \
    -t tmpfs \
    -o "mode=0700,size=8m,nosuid,nodev,noexec,uid=$caller_uid,gid=$caller_gid" \
    tmpfs \
    "$fault_directory"
  mounted=true
  setpriv \
    --reuid "$caller_uid" \
    --regid "$caller_gid" \
    --clear-groups \
    --inh-caps=-all \
    --ambient-caps=-all \
    --bounding-set=-all \
    --reset-env \
    --no-new-privs \
    --pdeathsig SIGKILL \
    -- env \
    LINGUAMESH_RUNTIME_STORAGE_FAULT_TEST=1 \
    LINGUAMESH_RUNTIME_STORAGE_FAULT_DIRECTORY="$fault_directory" \
    "$test_binary" \
    --exact "$test_name" \
    --ignored \
    --nocapture \
    --test-threads=1
  exit 0
fi

if [[ "${1:-}" == --* ]]; then
  printf 'Unknown storage write-fault runner option: %s\n' "$1" >&2
  exit 2
fi

for command in cargo env grep id mktemp mount python3 rmdir sh sync umount unshare; do
  if ! command -v "$command" >/dev/null 2>&1; then
    printf 'Required command is unavailable: %s\n' "$command" >&2
    exit 127
  fi
done

# 无论调用位置如何，都从仓库根目录编译和运行精确测试。
script_dir="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repository_dir="$(CDPATH= cd -- "$script_dir/.." && pwd)"
cd "$repository_dir"

test_name='worker::tests::runtime_storage_write_failures_degrade_to_session_mode_without_false_commits'
package_id="$(cargo pkgid)"

printf '%s\n' 'Compiling the runtime storage write-fault test.'
test_binary="$({
  cargo test \
    --features demo-provider \
    --locked \
    --lib \
    --no-run \
    --message-format=json-render-diagnostics
} | LINGUAMESH_PACKAGE_ID="$package_id" python3 -c '
import json
import os
import sys

package_id = os.environ["LINGUAMESH_PACKAGE_ID"]
executables = []
for line in sys.stdin:
    try:
        message = json.loads(line)
    except json.JSONDecodeError:
        continue
    target = message.get("target", {})
    profile = message.get("profile", {})
    executable = message.get("executable")
    if (
        message.get("reason") == "compiler-artifact"
        and message.get("package_id") == package_id
        and profile.get("test") is True
        and target.get("name") == "linguamesh_linux"
        and target.get("kind") == ["lib"]
        and executable
    ):
        executables.append(executable)

if len(executables) != 1:
    print(
        f"Expected exactly one LinguaMesh Linux library test binary, found {len(executables)}.",
        file=sys.stderr,
    )
    raise SystemExit(1)
print(executables[0])
')"

if [[ ! -x "$test_binary" ]]; then
  printf '%s\n' 'The compiled runtime storage test binary is not executable.' >&2
  exit 1
fi

printf '%s\n' 'Running the runtime storage write-fault test in a private mount namespace.'
probe_directory="$(mktemp -d /tmp/linguamesh-linux-userns-probe.XXXXXX)"
if unshare \
  --user \
  --map-root-user \
  --mount \
  --fork \
  --kill-child=SIGKILL \
  --propagation private \
  -- sh -eu -c \
  'mount -t tmpfs -o mode=0700,size=1m,nosuid,nodev,noexec tmpfs "$1"; umount "$1"' \
  sh "$probe_directory" >/dev/null 2>&1; then
  unprivileged_mount=true
else
  unprivileged_mount=false
fi
rmdir "$probe_directory"

set +e
if [[ "$unprivileged_mount" == true ]]; then
  test_output="$(
    unshare \
      --user \
      --map-root-user \
      --mount \
      --fork \
      --kill-child=SIGKILL \
      --propagation private \
      -- env -u LINGUAMESH_RUNTIME_STORAGE_FAULT_DIRECTORY \
      LINGUAMESH_RUNTIME_STORAGE_FAULT_TEST=1 \
      "$test_binary" \
      --exact "$test_name" \
      --ignored \
      --nocapture \
      --test-threads=1 2>&1
  )"
  test_status=$?
elif command -v sudo >/dev/null 2>&1 \
  && command -v setpriv >/dev/null 2>&1 \
  && sudo -n true >/dev/null 2>&1; then
  printf '%s\n' 'Unprivileged mounts are unavailable; using the controlled CI mount fallback.'
  caller_uid="$(id -u)"
  caller_gid="$(id -g)"
  test_output="$(
    sudo -n unshare \
      --mount \
      --fork \
      --kill-child=SIGKILL \
      --propagation private \
      -- bash "$script_dir/run-storage-fault-test.sh" \
      --root-mount-runner \
      "$caller_uid" \
      "$caller_gid" \
      "$test_binary" \
      "$test_name" 2>&1
  )"
  test_status=$?
else
  test_output='Neither an unprivileged mount namespace nor the controlled CI mount fallback is available.'
  test_status=1
fi
set -e
printf '%s\n' "$test_output"

if [[ "$test_status" -ne 0 ]]; then
  printf '%s\n' 'The runtime storage write-fault test failed.' >&2
  exit "$test_status"
fi

if ! grep -Fq 'test result: ok. 1 passed; 0 failed; 0 ignored;' <<<"$test_output"; then
  printf '%s\n' 'The runtime storage write-fault test did not execute exactly once.' >&2
  exit 1
fi

printf '%s\n' 'Runtime storage write-fault validation passed.'
