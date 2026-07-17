# Linux Architecture

## Implemented vertical slice

`src/model.rs` is a toolkit-independent state reducer over typed `linguamesh-domain` events. It
owns selection and presentation state, enforces monotonic event ordering, preserves partial output,
formats safe diagnostics, and never performs provider work.

With the `demo-provider` feature, `src/worker.rs` runs bounded command/event channels and the sibling
core engine on a dedicated Tokio runtime. It starts the shared loopback fake provider, performs
model discovery, forwards typed stream events, and shuts down the operation and server. A shared
Core cancellation handle bypasses command-queue backpressure, control commands receive selection
priority, and cancellation that races with a terminal event is idempotent.
Provider HTTP, SSE parsing, prompts, errors, and cancellation semantics remain in
`linguamesh-core`.

With the `gui` feature, `src/main.rs` binds that same state and worker to GTK 4/libadwaita widgets.
GTK objects remain on the main context, which processes at most 64 queued events and coalesces UI
refreshes once per timer tick without performing network work. GTK model updates release
application-state borrows before they can emit reentrant selection notifications. The native shell
provides provider/model and language controls, source and streamed output editors, translate/stop
actions, typed error display, appearance switching, locale preference with an explicit English
fallback, and redacted diagnostics.

`l10n/linux/` is a byte-for-byte consumer snapshot of the canonical PO catalogs at the revision
enforced by `tools/sync-l10n.sh`. Resource provenance and format validation are implemented, while
runtime gettext lookup remains a separate incomplete client concern.

The application state and worker command/event wrappers intentionally do not derive `Debug`, so
source text and streamed output are not exposed through routine debug formatting.

## Required boundaries

The client uses stable Rust, GTK 4 through gtk-rs, GLib/GIO, and libadwaita for presentation. GTK
objects remain on the main context; network, database, document, tokenization, and core work must
not block it. UI state changes cross bounded, cancellable channels and preserve event ordering.

The client calls the public Rust application layer directly while preserving the same observable
event behavior as FFI clients. It must not fork provider, routing, translation, document,
persistence, or error-domain logic. Full semantic, catalog, feature, and persistence compatibility
negotiation remains required before a release.

The client owns native lifecycle, accessibility, appearance, keyboard behavior, file dialogs,
portals, drag-and-drop, clipboard, notifications, XDG paths, desktop metadata, display-server
integration, and credential resolution. Only lifecycle, basic keyboard mnemonics, appearance, and
the text workspace are present in this slice.

## Security and portability boundaries

Secrets must use Secret Service. When no secure service exists, the UI may offer a clearly labeled session-only in-memory secret and must never fall back to plaintext. File and directory handling must follow XDG locations, restrictive permissions, portal leases, and cleanup rules. Wayland is required; X11 support is practical where dependencies and tests allow it.

Changes affecting shared contracts, the security model, display support, GTK/libadwaita policy, or distribution packaging require central compatibility review. GTK and other LGPL dependencies require documented license compliance before distribution.
