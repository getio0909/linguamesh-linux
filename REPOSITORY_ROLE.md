# Repository Role

## Purpose

`linguamesh-linux` delivers the native Linux application for LinguaMesh.

## This repository owns

- Rust and GTK 4 user interface and native Linux user experience.
- GLib/GIO integration, optional libadwaita presentation, lifecycle, accessibility, appearance, clipboard, drag-and-drop, dialogs, notifications, and keyboard behavior.
- XDG base-directory and desktop-portal integration, Secret Service credential resolution, and session-only credential handling when secure storage is unavailable.
- Direct typed integration with pinned LinguaMesh Core crates without duplicating shared behavior.
- Linux tests, desktop metadata, Flatpak packaging, and distribution-specific release validation.

## This repository does not own

- Provider adapters, routing, prompt construction, document codecs, shared persistence, or translation logic; those belong in `linguamesh-core`.
- Canonical localization source or generators; those belong in `linguamesh-l10n`.
- Cross-repository compatibility and release-train authority; those belong in `linguamesh-project`.

The client must reject incompatible shared contracts and must not fork shared core behavior merely because both layers use Rust.

