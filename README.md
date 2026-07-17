# LinguaMesh for Linux

LinguaMesh for Linux is the native Rust, GTK 4, and libadwaita client for the LinguaMesh
translation suite. The current Core `0.1.0-alpha.2` vertical slice starts disconnected, connects
only after an explicit user action, requires an explicit model selection, streams translated text,
and supports cancellation with partial-output retention. It also displays typed errors, switches
appearance, records locale preference, and exposes redacted diagnostics.

## Project authority

- [`GLOBAL_GOAL.md`](GLOBAL_GOAL.md) pins the global specification revision.
- [`REPOSITORY_ROLE.md`](REPOSITORY_ROLE.md) defines this repository's ownership boundaries.
- [`IMPLEMENTATION_STATUS.md`](IMPLEMENTATION_STATUS.md) records what is actually implemented and verified.

The authoritative specification lives in the sibling `linguamesh-project` repository. Product
work must remain compatible with LinguaMesh Core and the central release train. Native CI pins the
reviewed Core functional revision `c9a96da52e10554c8458f4d49600ec9336ea651b`.

## Native stack

The client uses stable Rust, GTK 4 through gtk-rs, GLib/GIO, and libadwaita. Shared provider,
streaming, cancellation, compatibility, and secret-broker behavior comes through Core's public
Rust application layer. The Linux layer owns native state reduction, host-secret responses,
background scheduling, and widgets.

## Build and run

On Debian or Ubuntu, install native development headers:

```sh
sudo apt-get install libgtk-4-dev libadwaita-1-dev gettext pkg-config
cargo run --features gui
```

The development worker starts a loopback-only fake service and places its endpoint in the form when
the default endpoint is still untouched. Readiness does not connect it: click **Connect**, wait for
model discovery, and deliberately choose a model before translating. A user-supplied
OpenAI-compatible base endpoint such as `http://127.0.0.1:11434/v1/` follows the same flow.

The credential field is optional and session-only. Its value is copied into Core's secret-aware
`SecretValue`, the widget is cleared immediately, the temporary GTK string is dropped, and a
`session:` `SecretRef` lets the bounded typed host-secret broker provide it once during connection.
Neither the credential nor the profile is persisted. Secret Service and persistent provider
profiles are not implemented; persistent intent or a persistent secret reference fails closed with
a typed error instead of falling back to plaintext. Connection and translation can both be
cancelled. A failed provider switch preserves the previously confirmed provider and model.

The tested external-provider path uses the LinguaMesh fake provider on loopback. It is not evidence
of interoperability with Ollama or any other third-party server. Full validation commands, the
header-free local path, and the GTK/Xvfb CI gate are documented in
[`docs/testing.md`](docs/testing.md). No release or packaging artifact is implemented yet.

At worker startup, the client requires exact Core `0.1.0-alpha.2`, ABI 1, protocol 1, provider
catalog `0.1.0`, and the reviewed feature subset. The native workflow checks out the exact
functional revision above; an arbitrary default branch is not compatibility evidence.

Canonical PO catalogs are synchronized from immutable l10n revision
`52e73ea2a6cc7e6e7409b2b6eb0d02db35576a49` and validated with `msgfmt`. The locale selector
currently records `en` or `zh-CN`, but English remains the explicit runtime fallback until the
GTK gettext adapter is implemented.

## Documentation

- [Architecture](docs/architecture.md)
- [Testing](docs/testing.md)
- [Releasing](docs/releasing.md)
- [Contributing](CONTRIBUTING.md)
- [Security policy](SECURITY.md)
