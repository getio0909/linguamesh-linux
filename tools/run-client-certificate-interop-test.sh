#!/usr/bin/env bash
set -euo pipefail

script_dir="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repository_dir="$(CDPATH= cd -- "$script_dir/.." && pwd)"
fixture_dir="$(mktemp -d /tmp/linguamesh-client-certificate.XXXXXX)"
server_pid=""
untrusted_server_pid=""
hostname_server_pid=""
client_auth_server_pid=""

cleanup() {
  local exit_status=$?
  if [[ -n "$server_pid" ]]; then
    kill "$server_pid" >/dev/null 2>&1 || true
    wait "$server_pid" >/dev/null 2>&1 || true
  fi
  if [[ -n "$untrusted_server_pid" ]]; then
    kill "$untrusted_server_pid" >/dev/null 2>&1 || true
    wait "$untrusted_server_pid" >/dev/null 2>&1 || true
  fi
  if [[ -n "$hostname_server_pid" ]]; then
    kill "$hostname_server_pid" >/dev/null 2>&1 || true
    wait "$hostname_server_pid" >/dev/null 2>&1 || true
  fi
  if [[ -n "$client_auth_server_pid" ]]; then
    kill "$client_auth_server_pid" >/dev/null 2>&1 || true
    wait "$client_auth_server_pid" >/dev/null 2>&1 || true
  fi
  if [[ "$fixture_dir" == /tmp/linguamesh-client-certificate.* ]]; then
    rm -rf -- "$fixture_dir"
  fi
  exit "$exit_status"
}
trap cleanup EXIT

for command in cargo openssl python3; do
  if ! command -v "$command" >/dev/null 2>&1; then
    printf 'Required client-certificate fixture command is unavailable: %s\n' "$command" >&2
    exit 127
  fi
done

cd "$fixture_dir"
openssl req -x509 -newkey rsa:2048 -nodes -days 1 \
  -keyout ca.key -out ca.pem -subj '/CN=LinguaMesh test CA' \
  -addext 'basicConstraints=critical,CA:TRUE' \
  -addext 'keyUsage=critical,keyCertSign,cRLSign' >/dev/null 2>&1
openssl req -newkey rsa:2048 -nodes \
  -keyout server.key -out server.csr -subj '/CN=127.0.0.1' >/dev/null 2>&1
printf '%s\n' \
  'basicConstraints=critical,CA:FALSE' \
  'keyUsage=critical,digitalSignature,keyEncipherment' \
  'extendedKeyUsage=serverAuth' \
  'subjectAltName=IP:127.0.0.1' > server.ext
openssl x509 -req -days 1 -in server.csr -CA ca.pem -CAkey ca.key -CAcreateserial \
  -out server.pem -extfile server.ext >/dev/null 2>&1
openssl req -newkey rsa:2048 -nodes \
  -keyout client.key -out client.csr -subj '/CN=LinguaMesh test client' >/dev/null 2>&1
printf '%s\n' \
  'basicConstraints=critical,CA:FALSE' \
  'keyUsage=critical,digitalSignature,keyEncipherment' \
  'extendedKeyUsage=clientAuth' > client.ext
openssl x509 -req -days 1 -in client.csr -CA ca.pem -CAkey ca.key -CAserial ca.srl \
  -out client.pem -extfile client.ext >/dev/null 2>&1
cat client.pem client.key > client-identity.pem

openssl req -x509 -newkey rsa:2048 -nodes -days 1 \
  -keyout untrusted-ca.key -out untrusted-ca.pem -subj '/CN=LinguaMesh untrusted test CA' \
  -addext 'basicConstraints=critical,CA:TRUE' \
  -addext 'keyUsage=critical,keyCertSign,cRLSign' >/dev/null 2>&1
openssl req -newkey rsa:2048 -nodes \
  -keyout untrusted-server.key -out untrusted-server.csr -subj '/CN=127.0.0.1' >/dev/null 2>&1
cp server.ext untrusted-server.ext
openssl x509 -req -days 1 -in untrusted-server.csr \
  -CA untrusted-ca.pem -CAkey untrusted-ca.key -CAcreateserial \
  -out untrusted-server.pem -extfile untrusted-server.ext >/dev/null 2>&1

openssl req -newkey rsa:2048 -nodes \
  -keyout hostname-server.key -out hostname-server.csr -subj '/CN=wrong.linguamesh.test' >/dev/null 2>&1
printf '%s\n' \
  'basicConstraints=critical,CA:FALSE' \
  'keyUsage=critical,digitalSignature,keyEncipherment' \
  'extendedKeyUsage=serverAuth' \
  'subjectAltName=DNS:wrong.linguamesh.test' > hostname-server.ext
openssl x509 -req -days 1 -in hostname-server.csr \
  -CA ca.pem -CAkey ca.key -CAserial ca.srl \
  -out hostname-server.pem -extfile hostname-server.ext >/dev/null 2>&1

openssl req -newkey rsa:2048 -nodes \
  -keyout client-auth-server.key -out client-auth-server.csr -subj '/CN=127.0.0.1' >/dev/null 2>&1
cp server.ext client-auth-server.ext
openssl x509 -req -days 1 -in client-auth-server.csr \
  -CA ca.pem -CAkey ca.key -CAserial ca.srl \
  -out client-auth-server.pem -extfile client-auth-server.ext >/dev/null 2>&1

port_file="$fixture_dir/port"
python3 "$repository_dir/tools/client-certificate-http-fixture.py" \
  --certificate "$fixture_dir/server.pem" \
  --private-key "$fixture_dir/server.key" \
  --client-ca "$fixture_dir/ca.pem" \
  --port-file "$port_file" &
server_pid=$!
for _ in {1..100}; do
  if [[ -s "$port_file" ]]; then
    break
  fi
  if ! kill -0 "$server_pid" >/dev/null 2>&1; then
    printf '%s\n' 'The client-certificate HTTPS fixture exited before publishing its port.' >&2
    exit 1
  fi
  sleep 0.05
done
if [[ ! -s "$port_file" ]]; then
  printf '%s\n' 'Timed out waiting for the client-certificate HTTPS fixture.' >&2
  exit 1
fi

endpoint="https://127.0.0.1:$(<"$port_file")/v1/"
untrusted_port_file="$fixture_dir/untrusted-port"
python3 "$repository_dir/tools/client-certificate-http-fixture.py" \
  --certificate "$fixture_dir/untrusted-server.pem" \
  --private-key "$fixture_dir/untrusted-server.key" \
  --client-ca "$fixture_dir/ca.pem" \
  --port-file "$untrusted_port_file" &
untrusted_server_pid=$!
for _ in {1..100}; do
  if [[ -s "$untrusted_port_file" ]]; then
    break
  fi
  if ! kill -0 "$untrusted_server_pid" >/dev/null 2>&1; then
    printf '%s\n' 'The untrusted client-certificate HTTPS fixture exited before publishing its port.' >&2
    exit 1
  fi
  sleep 0.05
done
if [[ ! -s "$untrusted_port_file" ]]; then
  printf '%s\n' 'Timed out waiting for the untrusted client-certificate HTTPS fixture.' >&2
  exit 1
fi

untrusted_endpoint="https://127.0.0.1:$(<"$untrusted_port_file")/v1/"
hostname_port_file="$fixture_dir/hostname-port"
python3 "$repository_dir/tools/client-certificate-http-fixture.py" \
  --certificate "$fixture_dir/hostname-server.pem" \
  --private-key "$fixture_dir/hostname-server.key" \
  --client-ca "$fixture_dir/ca.pem" \
  --port-file "$hostname_port_file" &
hostname_server_pid=$!
for _ in {1..100}; do
  if [[ -s "$hostname_port_file" ]]; then
    break
  fi
  if ! kill -0 "$hostname_server_pid" >/dev/null 2>&1; then
    printf '%s\n' 'The hostname-mismatch HTTPS fixture exited before publishing its port.' >&2
    exit 1
  fi
  sleep 0.05
done
if [[ ! -s "$hostname_port_file" ]]; then
  printf '%s\n' 'Timed out waiting for the hostname-mismatch HTTPS fixture.' >&2
  exit 1
fi

hostname_endpoint="https://127.0.0.1:$(<"$hostname_port_file")/v1/"
client_auth_port_file="$fixture_dir/client-auth-port"
python3 "$repository_dir/tools/client-certificate-http-fixture.py" \
  --certificate "$fixture_dir/client-auth-server.pem" \
  --private-key "$fixture_dir/client-auth-server.key" \
  --client-ca "$fixture_dir/untrusted-ca.pem" \
  --port-file "$client_auth_port_file" &
client_auth_server_pid=$!
for _ in {1..100}; do
  if [[ -s "$client_auth_port_file" ]]; then
    break
  fi
  if ! kill -0 "$client_auth_server_pid" >/dev/null 2>&1; then
    printf '%s\n' 'The client-authentication rejection fixture exited before publishing its port.' >&2
    exit 1
  fi
  sleep 0.05
done
if [[ ! -s "$client_auth_port_file" ]]; then
  printf '%s\n' 'Timed out waiting for the client-authentication rejection fixture.' >&2
  exit 1
fi

client_auth_endpoint="https://127.0.0.1:$(<"$client_auth_port_file")/v1/"
printf '%s\n' 'Running the client-certificate HTTPS interoperability tests.'
cd "$repository_dir"
LINGUAMESH_CLIENT_CERT_ENDPOINT="$endpoint" \
LINGUAMESH_CLIENT_CERT_IDENTITY_PATH="$fixture_dir/client-identity.pem" \
LINGUAMESH_CLIENT_CERT_CA_PATH="$fixture_dir/ca.pem" \
  cargo test --features demo-provider --locked \
  worker::tests::running_client_certificate_provider_connects \
  -- --ignored --exact --nocapture
LINGUAMESH_CLIENT_CERT_UNTRUSTED_ENDPOINT="$untrusted_endpoint" \
LINGUAMESH_CLIENT_CERT_IDENTITY_PATH="$fixture_dir/client-identity.pem" \
LINGUAMESH_CLIENT_CERT_CA_PATH="$fixture_dir/ca.pem" \
  cargo test --features demo-provider --locked \
  worker::tests::running_client_certificate_provider_rejects_untrusted_server \
  -- --ignored --exact --nocapture
LINGUAMESH_CLIENT_CERT_HOSTNAME_ENDPOINT="$hostname_endpoint" \
LINGUAMESH_CLIENT_CERT_IDENTITY_PATH="$fixture_dir/client-identity.pem" \
LINGUAMESH_CLIENT_CERT_CA_PATH="$fixture_dir/ca.pem" \
  cargo test --features demo-provider --locked \
  worker::tests::running_client_certificate_provider_rejects_hostname_mismatch \
  -- --ignored --exact --nocapture
LINGUAMESH_CLIENT_CERT_UNTRUSTED_CLIENT_ENDPOINT="$client_auth_endpoint" \
LINGUAMESH_CLIENT_CERT_IDENTITY_PATH="$fixture_dir/client-identity.pem" \
LINGUAMESH_CLIENT_CERT_CA_PATH="$fixture_dir/ca.pem" \
  cargo test --features demo-provider --locked \
  worker::tests::running_client_certificate_provider_rejects_untrusted_client \
  -- --ignored --exact --nocapture
