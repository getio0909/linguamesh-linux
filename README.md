# LinguaMesh for Linux

LinguaMesh for Linux is the native Rust, GTK 4, and libadwaita client for the LinguaMesh
translation suite. The current vertical slice starts with the shared core's built-in loopback fake
provider and can connect to a user-supplied, credential-free OpenAI-compatible endpoint for the
current process. It discovers and selects models, streams translated text, supports cancellation
with partial-output retention, displays typed errors, switches appearance, records locale
preference, and exposes redacted diagnostics.

## Project authority

- [`GLOBAL_GOAL.md`](GLOBAL_GOAL.md) pins the global specification revision.
- [`REPOSITORY_ROLE.md`](REPOSITORY_ROLE.md) defines this repository's ownership boundaries.
- [`IMPLEMENTATION_STATUS.md`](IMPLEMENTATION_STATUS.md) records what is actually implemented and verified.

The authoritative specification lives in the sibling `linguamesh-project` repository. Product
work must remain compatible with LinguaMesh Core and the central release train. Native CI pins the
reviewed Core revision `873b6da45447f73e4be4e2f1127c3c8d0f188cf2`.

## Native stack

The client uses stable Rust, GTK 4 through gtk-rs, GLib/GIO, and libadwaita. Shared domain,
provider, streaming, cancellation, and protocol behavior comes directly from sibling
`linguamesh-core` crates. The Linux layer owns only application state, background scheduling, and
native widgets.

## Build and run

On Debian or Ubuntu, install native development headers:

```sh
sudo apt-get install libgtk-4-dev libadwaita-1-dev gettext pkg-config
cargo run --features gui
```

The app starts a loopback-only fake provider and requires no commercial credential. To exercise a
local OpenAI-compatible server, run the app and enter a session name plus a base endpoint such as
`http://127.0.0.1:11434/v1/`. The form does not accept credentials, and its values are never
persisted. Models and the active provider change only after discovery succeeds; a failed connection
leaves the previous provider and model usable.

The tested external-provider path uses the LinguaMesh fake provider on loopback. It is not evidence
of interoperability with Ollama or any other third-party server. Full validation commands, the
header-free local path, and the GTK/Xvfb CI gate are documented in
[`docs/testing.md`](docs/testing.md). No release or packaging artifact is implemented yet.

The native workflow checks out Core at the exact reviewed revision above. Development against an
arbitrary default branch is not accepted as compatibility evidence.

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
