# Security Policy

## Reporting a vulnerability

Do not disclose suspected vulnerabilities in a public issue. Use this repository's private GitHub security-advisory channel when it is enabled. If it is unavailable, follow the private reporting route documented by the central `linguamesh-project` security policy. Include affected revisions, reproduction details, impact, and a safe contact method. Do not include live credentials or private user documents.

## Supported versions

This repository has no released application version. The documentation-only foundation receives maintenance, but there is currently no product binary to classify as supported.

## Security requirements

- Store provider credentials with Secret Service; offer an explicit in-memory session-only secret when secure storage is unavailable, and never silently use plaintext persistence.
- Keep translation content, authorization headers, cookies, signed URLs, private keys, and packaging credentials out of source, logs, diagnostics, tests, and CI artifacts.
- Require HTTPS for remote endpoints. Loopback HTTP is allowed only under the global policy.
- Treat provider output, source documents, locale data, file paths, portal responses, and protocol messages as untrusted input.
- Use restrictive file permissions, XDG locations, desktop portals where appropriate, explicit lease lifetimes, and core compatibility checks.
- Never provide release credentials to untrusted pull-request workflows.

Security-sensitive changes require focused tests, threat-model review, and explicit evidence in `IMPLEMENTATION_STATUS.md`.
