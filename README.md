# LinguaMesh for Linux

LinguaMesh for Linux is the native Rust, GTK 4, and libadwaita client for the LinguaMesh
translation suite. The current Core `0.1.0-alpha.2` vertical slice starts disconnected, connects
only after an explicit user action, requires a deliberate model choice for a new profile, streams
translated text, and supports cancellation with partial-output retention. It can explicitly
remember multiple non-secret provider profiles, switch or update them through the same explicit
connection action, and revalidate each model preference after reconnect while keeping credentials
session-only. A derived provider-setup guide moves from startup through configuration, connection,
and model selection, reports an unavailable worker without remaining stuck at startup, then
identifies the provider stable ID/model that will receive the next request. Saved
copies can be removed without interrupting an already connected session. The client also displays
typed errors, switches appearance, records locale preference, and exposes redacted diagnostics.

## Project authority

- [`GLOBAL_GOAL.md`](GLOBAL_GOAL.md) pins the global specification revision.
- [`REPOSITORY_ROLE.md`](REPOSITORY_ROLE.md) defines this repository's ownership boundaries.
- [`IMPLEMENTATION_STATUS.md`](IMPLEMENTATION_STATUS.md) records what is actually implemented and verified.

The authoritative specification lives in the sibling `linguamesh-project` repository. Product
work must remain compatible with LinguaMesh Core and the central release train. Native CI pins the
reviewed Core functional revision `fbf3e9b5927049dccaa19f8c36013495ffebba12`, which adds
`SQLITE_OPEN_NOFOLLOW` to file-backed storage.

## Native stack

The client uses stable Rust, GTK 4.10 or newer through gtk-rs, GLib/GIO, and libadwaita. Shared provider,
streaming, cancellation, compatibility, and secret-broker behavior comes through Core's public
Rust application layer. The Linux layer owns native state reduction, host-secret responses,
background scheduling, and widgets.

The current GTK surface includes baseline accessibility semantics: the main workspace, headings,
status, and errors expose explicit roles; source/output editors expose names, multi-line and
read-only properties; editable fields and dropdowns are labelled by visible mnemonic labels; the
output region reports translation busy state; empty errors are hidden from the accessibility tree;
and Stop has the explicit accessible name “Stop translation”. Focusable controls retain the visual
tab order. This is semantic widget wiring, not a claim of Orca/AT-SPI or physical-keyboard coverage.

## Build and run

On Debian or Ubuntu, install native development headers:

```sh
sudo apt-get install libgtk-4-dev libadwaita-1-dev gettext pkg-config
cargo run --features gui
```

The development worker starts a loopback-only fake service and places its endpoint in the form when
the default endpoint is still untouched. Readiness does not connect it: click **Connect**, wait for
model discovery, and deliberately choose a model before translating. The **Provider setup** card
shows each required step and, once ready, the exact provider name, stable ID, and confirmed model
for the next request. A pending model change remains in Step 2 until committed, and a stopped worker
is shown as unavailable with request controls disabled. The card is derived from live state and
writes no completion flag. A user-supplied OpenAI-compatible base endpoint such as
`http://127.0.0.1:11434/v1/` follows the same flow.

Use **Open text file** to load a UTF-8 TXT or Markdown file into the source editor. The native
GTK file dialog and asynchronous GIO partial read enforce a 4 MiB limit, strip a UTF-8 BOM, reject
invalid UTF-8, and never place the selected path or file contents in diagnostics. Dropping one GIO
file onto the source editor reuses the same bounded import path; portal-specific file leases remain
a separate follow-up work item.

The credential field is optional. Its value is copied into Core's secret-aware `SecretValue`, the
widget is cleared immediately, and the temporary GTK string is dropped. Without Remember, a
`session:` `SecretRef` lets the bounded typed host-secret broker provide it once during connection.
When Remember is selected with a credential, the Linux host stores it through Secret Service and
persists only the resulting `secret-service:` reference with the profile. New saved profiles
receive a random stable ID independent of their display name. The worker stores each non-secret
profile copy in Core's SQLite database at
`$XDG_DATA_HOME/dev.linguamesh.LinguaMesh/linguamesh.sqlite3` (normally under
`~/.local/share`) with a `0700` application directory and `0600` database file. Core opens SQLite
with no-follow protection on Linux's default Unix VFS, rejecting any symbolic-link component; the
Linux layer additionally rejects hard links and non-private storage paths.

Startup restores the complete saved-profile list and displays the last persistently activated row,
but remains disconnected and performs no provider request. Selecting another row only prefills the
form. Enter the credential again when required, then click **Connect** to validate and switch.
**Remove saved profile** deletes only that stored row; if it is currently connected, the validated
runtime session and model continue in visibly session-only mode. Provider controls remain disabled
until startup finishes. Credential values are never written to the database; only persistent
`SecretRef` identifiers are stored. Secret Service absence, locked items, and unsupported
interactive prompts fail closed with typed errors instead of falling back to plaintext. Session-only
connection remains available when remembering is disabled or profile storage/keyring access is
unavailable. Connection and translation can both be cancelled, and a failed provider switch
preserves the previously confirmed provider and model.

If an already-open database later returns a persistent write error, the triggering Connect, model
change, or deletion is rejected before any success is reported. The worker drops that storage
handle and saved-profile marker, keeps the previously validated engine and model usable in
session-only mode, and reports storage as unavailable. A private Linux tmpfs regression forces a
real `ENOSPC` at each transaction boundary and verifies after restart that only pre-fault state was
committed.

The tested external-provider path uses the LinguaMesh fake provider on loopback. It is not evidence
of interoperability with Ollama or any other third-party server. Full validation commands, the
header-free local path, and the GTK gates for X11/Xvfb and forced Wayland/headless Weston are
documented in
[`docs/testing.md`](docs/testing.md). No release artifact is implemented yet.

The repository now includes a reproducible Flatpak manifest, pinned Cargo source set, desktop
entry, AppStream metadata, and icon under [`packaging/flatpak`](packaging/flatpak). Run
`bash tools/validate-flatpak-metadata.sh` for local metadata validation. The GNOME 49 SDK build and
bounded private-D-Bus sandbox smoke run remotely; the resulting bundle is a prerelease CI artifact,
not a signed or published release, and portal leases plus desktop-shell delivery remain separate
gates.

The two display gates execute the same real GTK binary test. Headless Weston proves that the client
can initialize and complete that flow with `GDK_BACKEND=wayland` and no X11 fallback; it is not
evidence for a physical compositor, GPU rendering, assistive technology, or a complete desktop
matrix.

At worker startup, the client requires exact Core `0.1.0-alpha.2`, ABI 1, protocol 1, provider
catalog `0.1.0`, and the reviewed feature subset. The native workflow checks out the exact
functional revision above; an arbitrary default branch is not compatibility evidence.

Canonical PO catalogs are synchronized from immutable l10n revision
`52e73ea2a6cc7e6e7409b2b6eb0d02db35576a49` and validated with `msgfmt`. The locale selector
records `en` or `zh-CN`; the Linux host now loads the pinned English and Simplified Chinese PO
catalogs at runtime for localized action labels without losing active text. Remaining UI strings
still use English fallbacks until complete gettext coverage is wired.

When a translation completes, the registered Linux application sends a desktop notification with
generic English text only; source and translated content are never included in notification
payloads.

## Documentation

- [Architecture](docs/architecture.md)
- [Testing](docs/testing.md)
- [Releasing](docs/releasing.md)
- [Contributing](CONTRIBUTING.md)
- [Security policy](SECURITY.md)
