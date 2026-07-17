# Linux Architecture

## Current state

This repository is documentation-only. No application architecture has been instantiated and no product capability is claimed.

## Required boundaries

The future client will use stable Rust, GTK 4 through gtk-rs, GLib/GIO, and optionally libadwaita for presentation that does not make core behavior GNOME-only. GTK objects remain on the main context; network, database, document, tokenization, and core work must not block it. UI state changes should cross bounded, cancellable channels and preserve event ordering.

The client should call the public Rust application layer directly where possible while preserving the same observable behavior as FFI clients. It must not fork provider, routing, translation, document, persistence, or error-domain logic. Startup must verify core semantic, protocol, catalog, feature, and persistence compatibility and fail safely when unsupported.

The client owns native lifecycle, accessibility, appearance, keyboard behavior, file dialogs, portals, drag-and-drop, clipboard, notifications, XDG paths, desktop metadata, display-server integration, and credential resolution.

## Security and portability boundaries

Secrets must use Secret Service. When no secure service exists, the UI may offer a clearly labeled session-only in-memory secret and must never fall back to plaintext. File and directory handling must follow XDG locations, restrictive permissions, portal leases, and cleanup rules. Wayland is required; X11 support is practical where dependencies and tests allow it.

Changes affecting shared contracts, the security model, display support, GTK/libadwaita policy, or distribution packaging require central compatibility review. GTK and other LGPL dependencies require documented license compliance before distribution.
