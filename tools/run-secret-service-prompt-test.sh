#!/usr/bin/env bash
set -euo pipefail

dbus-run-session -- bash -c '
  set -euo pipefail
  service_pid=""

  stop_service() {
    if [[ -n "$service_pid" ]]; then
      kill "$service_pid" >/dev/null 2>&1 || true
      wait "$service_pid" >/dev/null 2>&1 || true
      service_pid=""
    fi
  }

  start_service() {
    LINGUAMESH_SECRET_SERVICE_PROMPT_OPERATION="$1" \
      python3 tools/secret-service-prompt-fixture.py &
    service_pid=$!
    sleep 0.2
    service_ready=0
    for _ in {1..20}; do
      if ! kill -0 "$service_pid" 2>/dev/null; then
        printf "%s\n" "Secret Service prompt fixture exited before readiness." >&2
        exit 1
      fi
      if gdbus call --session \
        --dest org.freedesktop.secrets \
        --object-path /org/freedesktop/secrets \
        --method org.freedesktop.Secret.Service.ReadAlias default >/dev/null 2>&1; then
        service_ready=1
        break
      fi
      sleep 0.1
    done
    if [[ "$service_ready" -ne 1 ]]; then
      printf "%s\n" "Secret Service prompt fixture did not start." >&2
      exit 1
    fi
  }

  trap stop_service EXIT
  start_service store
  cargo test --features gui --lib secret_service::tests::secret_service_prompt_is_rejected_when_storing \
    --locked -- --ignored --exact
  stop_service
  start_service delete
  cargo test --features gui --lib secret_service::tests::secret_service_prompt_is_rejected_when_deleting \
    --locked -- --ignored --exact
'

printf '%s\n' 'Secret Service prompted-flow fixture passed for store and delete.'
