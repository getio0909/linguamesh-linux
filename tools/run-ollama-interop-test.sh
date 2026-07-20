#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
ollama_bin=${OLLAMA_BIN:-ollama}
endpoint=${LINGUAMESH_OLLAMA_ENDPOINT:-http://127.0.0.1:11434/api/}
model=${LINGUAMESH_OLLAMA_MODEL:-}
pull_model=${LINGUAMESH_OLLAMA_PULL:-0}

if ! command -v "$ollama_bin" >/dev/null 2>&1; then
    printf '%s\n' 'ollama is required for the third-party interoperability test.' >&2
    exit 2
fi
if [[ -z "$model" ]]; then
    printf '%s\n' 'LINGUAMESH_OLLAMA_MODEL must name an installed model.' >&2
    exit 2
fi

cleanup_dir=''
ollama_pid=''
cleanup() {
    if [[ -n "$ollama_pid" ]]; then
        kill "$ollama_pid" 2>/dev/null || true
        wait "$ollama_pid" 2>/dev/null || true
    fi
    if [[ -n "$cleanup_dir" ]]; then
        rm -rf "$cleanup_dir"
    fi
}
trap cleanup EXIT

base_endpoint=${endpoint%/api/}
if ! curl --fail --silent --show-error --max-time 5 "$base_endpoint/api/tags" >/dev/null 2>&1; then
    cleanup_dir=$(mktemp -d)
    OLLAMA_HOST=127.0.0.1:11434 OLLAMA_MODELS="$cleanup_dir/models" "$ollama_bin" serve \
        >"$cleanup_dir/ollama.log" 2>&1 &
    ollama_pid=$!
    for _ in $(seq 1 60); do
        if curl --fail --silent --show-error --max-time 2 "$base_endpoint/api/tags" >/dev/null 2>&1; then
            break
        fi
        sleep 1
    done
    curl --fail --silent --show-error --max-time 5 "$base_endpoint/api/tags" >/dev/null
fi

if [[ "$pull_model" == '1' ]]; then
    printf '%s\n' "Pulling Ollama model: $model"
    curl --fail --silent --show-error --max-time 1800 \
        -X POST "$base_endpoint/api/pull" \
        -H 'Content-Type: application/json' \
        -d "{\"name\":\"$model\",\"stream\":false}" >/dev/null
fi

printf '%s\n' "Running third-party Ollama interoperability test for model: $model"
cd "$repo_root"
LINGUAMESH_OLLAMA_ENDPOINT="$endpoint" \
LINGUAMESH_OLLAMA_MODEL="$model" \
cargo test --features demo-provider --offline \
    worker::tests::running_third_party_ollama_provider_translates_without_secret \
    -- --ignored --exact --nocapture
