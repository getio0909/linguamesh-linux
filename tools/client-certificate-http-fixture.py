#!/usr/bin/env python3
"""Serve a bounded HTTPS model-discovery response that requires a client certificate."""

import argparse
import pathlib
import socket
import ssl


def response(status: str, content_type: str, body: bytes) -> bytes:
    return (
        f"HTTP/1.1 {status}\r\n"
        f"Content-Type: {content_type}\r\n"
        f"Content-Length: {len(body)}\r\n"
        "Connection: close\r\n\r\n"
    ).encode("ascii") + body


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--certificate", required=True)
    parser.add_argument("--private-key", required=True)
    parser.add_argument("--client-ca", required=True)
    parser.add_argument("--port-file", required=True)
    args = parser.parse_args()

    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(args.certificate, args.private_key)
    context.load_verify_locations(cafile=args.client_ca)
    context.verify_mode = ssl.CERT_REQUIRED

    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind(("127.0.0.1", 0))
    listener.listen(4)
    listener.settimeout(0.5)
    pathlib.Path(args.port_file).write_text(str(listener.getsockname()[1]), encoding="ascii")

    try:
        while True:
            try:
                connection, _ = listener.accept()
            except TimeoutError:
                continue
            with connection:
                try:
                    with context.wrap_socket(connection, server_side=True) as secure:
                        request = b""
                        while b"\r\n\r\n" not in request and len(request) <= 64 * 1024:
                            chunk = secure.recv(4096)
                            if not chunk:
                                break
                            request += chunk
                        request_line = request.split(b"\r\n", 1)[0].split()
                        path = request_line[1].decode("ascii", "replace") if len(request_line) > 1 else ""
                        if path == "/v1/models":
                            body = b'{"data":[{"id":"client-cert-model"}]}'
                            secure.sendall(response("200 OK", "application/json", body))
                        else:
                            secure.sendall(response("404 Not Found", "text/plain", b"not found\n"))
                except ssl.SSLError:
                    continue
    except KeyboardInterrupt:
        return 0
    finally:
        listener.close()


if __name__ == "__main__":
    raise SystemExit(main())
