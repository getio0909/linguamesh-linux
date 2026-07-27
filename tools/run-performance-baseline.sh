#!/usr/bin/env bash
set -euo pipefail

output_dir=${1:-performance-baseline}
mkdir -p "$output_dir"

command -v python3 >/dev/null 2>&1 || {
  printf '%s\n' 'Python 3 is required for the Linux performance baseline.' >&2
  exit 127
}

cpu_model=$(awk -F: '/^model name/ { gsub(/^[[:space:]]+/, "", $2); print $2; exit }' /proc/cpuinfo)
memory_mb=$(awk '/^MemTotal:/ { printf "%.0f", $2 / 1024; exit }' /proc/meminfo)
kernel=$(uname -sr)
rustc_version=$(rustc --version)
core_revision=${CORE_REVISION:-unrecorded}
l10n_revision=${L10N_REVISION:-unrecorded}
baseline_file="$output_dir/LINUX-PERFORMANCE-BASELINE.tsv"

{
  printf 'LinguaMesh Linux performance baseline\n'
  printf 'Evidence status: machine-specific CI baseline; not a cross-machine performance claim\n'
  printf 'Kernel: %s\n' "$kernel"
  printf 'CPU: %s\n' "${cpu_model:-unknown}"
  printf 'Memory MiB: %s\n' "${memory_mb:-unknown}"
  printf 'Rust: %s\n' "$rustc_version"
  printf 'Core revision: %s\n' "$core_revision"
  printf 'Localization revision: %s\n' "$l10n_revision"
  printf 'Tests\tFilter\tElapsed seconds\n'
} > "$baseline_file"

run_case() {
  local label=$1
  local filter=$2
  local log_file
  local elapsed_file
  local elapsed
  log_file=$(mktemp)
  elapsed_file=$(mktemp)
  # 使用精确测试过滤器，避免把编译或无关测试混入单项基线。
  if ! python3 - "$log_file" "$elapsed_file" "$filter" <<'PY'
import pathlib
import subprocess
import sys
import time

log_path = pathlib.Path(sys.argv[1])
elapsed_path = pathlib.Path(sys.argv[2])
test_filter = sys.argv[3]
command = [
    "cargo",
    "test",
    "--features",
    "demo-provider",
    "--locked",
    "--offline",
    test_filter,
    "--",
    "--exact",
    "--test-threads=1",
]
started = time.monotonic()
with log_path.open("w", encoding="utf-8") as log:
    result = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, check=False)
elapsed_path.write_text(f"{time.monotonic() - started:.3f}\n", encoding="utf-8")
raise SystemExit(result.returncode)
PY
  then
    cat "$log_file" >&2
    find "$log_file" -type f -delete 2>/dev/null || true
    find "$elapsed_file" -type f -delete 2>/dev/null || true
    exit 1
  fi
  elapsed=$(<"$elapsed_file")
  printf '%s\t%s\t%s\n' "$label" "$filter" "$elapsed" >> "$baseline_file"
  find "$log_file" -type f -delete 2>/dev/null || true
  find "$elapsed_file" -type f -delete 2>/dev/null || true
}

run_case \
  'docx-reconstruction' \
  'worker::tests::document_job_translation_reconstructs_docx_and_preserves_binary_parts'
run_case \
  'xlsx-reconstruction' \
  'worker::tests::document_job_translation_reconstructs_xlsx_and_preserves_formulas_and_numbers'
run_case \
  'routing-dispatch' \
  'worker::tests::routing_profile_selects_saved_candidate_for_ordinary_translation'

printf 'Performance baseline created: %s\n' "$baseline_file"
