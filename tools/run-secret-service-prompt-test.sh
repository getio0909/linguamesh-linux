#!/usr/bin/env bash
set -euo pipefail

dbus-run-session -- bash -c '
  set -euo pipefail
  service_pid=""
  ready_file=""

  stop_service() {
    if [[ -n "$service_pid" ]]; then
      kill "$service_pid" >/dev/null 2>&1 || true
      wait "$service_pid" >/dev/null 2>&1 || true
      service_pid=""
    fi
    if [[ -n "$ready_file" ]]; then
      rm -f "$ready_file"
      ready_file=""
    fi
  }

  start_service() {
    ready_file=$(mktemp "${TMPDIR:-/tmp}/linguamesh-secret-service-ready.XXXXXX")
    rm -f "$ready_file"
    LINGUAMESH_SECRET_SERVICE_PROMPT_OPERATION="$1" \
      LINGUAMESH_SECRET_SERVICE_PROMPT_DISMISSED="${2:-0}" \
      LINGUAMESH_SECRET_SERVICE_PROMPT_READY_FILE="$ready_file" \
      python3 tools/secret-service-prompt-fixture.py &
    service_pid=$!
    service_ready=0
    for _ in {1..50}; do
      if ! kill -0 "$service_pid" 2>/dev/null; then
        printf "%s\n" "Secret Service prompt fixture exited before readiness." >&2
        exit 1
      fi
      if [[ -f "$ready_file" ]] && gdbus call --session \
        --dest org.freedesktop.secrets \
        --object-path /org/freedesktop/secrets \
        --method com.linguamesh.SecretServicePromptFixture.Ping >/dev/null 2>&1; then
        service_ready=1
        break
      fi
      sleep 0.2
    done
    if [[ "$service_ready" -ne 1 ]]; then
      printf "%s\n" "Secret Service prompt fixture did not start." >&2
      exit 1
    fi
  }

  trap stop_service EXIT
  start_service store 0
  cargo test --features gui --lib secret_service::tests::secret_service_prompt_is_accepted_when_storing \
    --locked -- --ignored --exact
  stop_service
  start_service store 1
  cargo test --features gui --lib secret_service::tests::secret_service_prompt_is_rejected_when_storing \
    --locked -- --ignored --exact
  stop_service
  start_service delete 0
  cargo test --features gui --lib secret_service::tests::secret_service_prompt_is_accepted_when_deleting \
    --locked -- --ignored --exact
  stop_service
  start_service delete 1
  cargo test --features gui --lib secret_service::tests::secret_service_prompt_is_rejected_when_deleting \
    --locked -- --ignored --exact
'

printf '%s\n' 'Secret Service prompted-flow fixture passed for store and delete.'
