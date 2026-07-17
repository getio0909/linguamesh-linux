# Implementation Status

Status: Core alpha.2 non-secret persistence/restart slice verified locally; native Linux CI pending

Global goal SHA-256: `11f9a65927aac7e57e2af119e9d21cc98e8d5a08b8a112a19ee1c47903e36198`

Assumption: canonical generated PO resources are synchronized and format-validated, but selecting
`zh-CN` continues to present an explicit English fallback until the GTK gettext adapter is
implemented and tested.

Assumption: the existing first-party `linguamesh-storage` crate and its already-reviewed bundled
SQLite dependency closure are the approved persistence contract for this Linux slice. No new
third-party direct dependency or native Secret Service implementation is introduced.

## Implemented

- Rust 1.93.0 Cargo package at `0.1.0-alpha.2`, with locked Core alpha.2 path dependencies and
  optional `demo-provider`/`gui` features. Native CI pins Core functional revision
  `fbf3e9b5927049dccaa19f8c36013495ffebba12`.
- Startup rejects any Core other than semantic version `0.1.0-alpha.2`, ABI 1, protocol 1, provider
  catalog `0.1.0`, with the required cancellation, compatibility, typed Rust host-secret broker,
  model-discovery, streaming-text, and text-translation features.
- Toolkit-independent state starts disconnected and uses canonical Core `ProviderProfile` and
  `ProviderProfileId` values. Matching connection results atomically commit pending provider and
  models; stale results and failed switches preserve the last active provider, models, and
  selection.
- Model discovery does not auto-select its first result. A saved model is restored only while it is
  still available; otherwise translation remains disabled until deliberate model selection is
  confirmed by the worker. Provider controls remain disabled until startup completes, and a
  rejected model selection resets the dropdown to the last confirmed model.
- The worker opens schema-2 Core storage at
  `$XDG_DATA_HOME/dev.linguamesh.LinguaMesh/linguamesh.sqlite3` (with GLib's user-data fallback),
  creates a private `0700` application directory and `0600` regular single-link database, and
  continues in explicit session-only mode after a typed storage initialization failure. On Linux's
  default Unix VFS, Core's SQLite no-follow open rejects any symbolic-link component, while the
  Linux host also rejects hard-linked, non-regular, or inode-mismatched leaf files before use.
- An explicit remember choice atomically saves only provider ID, display name, preset, adapter,
  endpoint, enabled state, and the last confirmed model. Startup restores those fields into the
  form but remains disconnected and performs no network request. A session switch, stale model
  command, failed persistent switch, or connection cancelled before its persistence commit cannot
  replace the last confirmed restart profile. Each database commit must succeed before its
  corresponding runtime state and success event are committed.
- The bounded worker uses `linguamesh_application::ProviderManager` and Core's bounded typed
  host-secret broker on a dedicated Tokio runtime. Fake-provider readiness only fills the default
  endpoint; it does not connect. Explicit Connect, SelectModel, and Translate commands drive real
  loopback model discovery and HTTP/SSE streaming.
- The optional credential is copied into secret-aware `SecretValue` storage, the widget is cleared
  immediately, and the temporary GTK string is dropped without claiming GTK-buffer zeroization. A
  random session `SecretRef` resolves it once through the broker. The credential and its `session:`
  reference are stripped before any profile write and must be entered again after restart. A
  persistent secret reference still fails closed because the native Secret Service backend is not
  implemented; no plaintext fallback exists.
- Connection cancellation uses a `CancellationToken`; translation cancellation uses Core's
  cross-thread handle. Both bypass command-queue backpressure. Partial output is retained, control
  commands receive priority, and a cancellation/terminal-event race remains idempotent.
- GTK 4/libadwaita source provides provider name, endpoint, optional session credential, explicit
  non-secret remember choice, connection and model selection, saved/session status, language
  controls, source/output views, Translate/Stop, typed errors, partial-result display, appearance,
  locale fallback notice, keyboard mnemonics, and redacted diagnostics. Provider controls are also
  blocked until worker startup finishes; provider/model/language controls are blocked during
  connection or translation, and event processing is capped per main-context tick.
- Fourteen canonical official/pseudo PO catalogs pinned to l10n revision
  `52e73ea2a6cc7e6e7409b2b6eb0d02db35576a49`. Sync rejects a different revision, dirty generated
  source artifacts, stale copies, and unexpected catalog counts.
- Foundation and native workflow sources use immutable Node 24-compatible action commits and
  disable persisted checkout credentials. Native CI pins reviewed Core revision
  `fbf3e9b5927049dccaa19f8c36013495ffebba12` and localization revision
  `52e73ea2a6cc7e6e7409b2b6eb0d02db35576a49`. The revised native gate installs D-Bus/Xvfb support
  and runs serialized all-target, all-feature tests under X11 before building the application.

## Local validation evidence

Validated on 2026-07-17 with Rust 1.93.0:

- The pinned global-goal SHA-256 matched the sibling authoritative file.
- Core functional revision `fbf3e9b5927049dccaa19f8c36013495ffebba12` is the reviewed source
  pin, and every direct Core dependency is constrained to `=0.1.0-alpha.2`. The clean local Core
  HEAD was a documentation-only descendant whose scoped compiled-source diff from that revision
  was empty.
- `cargo fmt --all --check`, the locked demo-provider check, strict Clippy, and build passed.
- `cargo test --no-default-features --locked` passed: 29 tests, 0 failed.
- `cargo test --features demo-provider --locked` passed: 50 tests, 0 failed. Coverage includes
  explicit connection and model selection, exact compatibility rejection, authenticated one-shot
  session secrets, fail-closed persistent references, non-secret save/restart/re-entry/translation,
  no-auto-connect restore, live and closed SQLite artifact secret scans, exact private permissions,
  permissive-directory, symbolic-ancestor, and hard-link rejection, session fallback after storage
  failure,
  stale/cancelled/failed persistence rollback through the public command path, cancellable
  connection/model discovery, cancellable streaming with partial output,
  active/queued/full-command-queue shutdown, translation terminal delivery during shutdown,
  saved-model validation, and failed-switch rollback.
- `DOCS_RS=1 cargo check --all-targets --all-features --locked` and the equivalent strict Clippy
  command passed only as source-level diagnostics of the GTK code.
- The exact foundation block in `docs/testing.md`, `git diff --check`, shell syntax validation, and
  code-comment/secret-pattern scans passed.
- `bash tools/sync-l10n.sh --check` passed against the exact clean l10n checkout, and all 14 PO
  catalogs passed `msgfmt --check --check-format`.
- The checkout, Rust-toolchain, and Rust-cache action SHAs resolved through the GitHub commits API;
  their action metadata uses Node 24 or a composite action.

## Remote validation evidence

Functional alpha.2 revision `0455baf8f258c6280d66d1d568fd6a01fdad8486` passed repository-foundation
run `29569227294` (job `87848829297`) and Native Linux run `29569227256` (job `87848829235`). The
native Ubuntu 24.04 job installed GTK 4, libadwaita, D-Bus, and Xvfb support; validated the exact
Core and localization pins; and passed formatting, strict all-feature Clippy, 35 library tests, the
real GTK binary test, and the all-target all-feature native application build.

Historical alpha.1 revision `c13394dd477fa6e919632c61c28ac0708f61b769` passed repository-foundation run
`29559609346` (job `87819062507`) and Native Linux run `29559609298` (job `87819062331`). The
native job installed GTK 4, libadwaita, D-Bus, and Xvfb support on Ubuntu 24.04; validated the exact
Core and localization pins; and passed formatting, strict all-feature Clippy, all-target all-feature
tests, and the native application build. The library suite passed 23 tests, and the separate GTK
binary suite passed its real-button connect-and-translate test under a serialized X11/D-Bus/Xvfb
session.

Earlier revision `10977931ceb11bc9d4b86ec49d7fd710e3c1a063` also passed repository-foundation
run `29557845248` and Native Linux run `29557845223` (job `87813768615`) before the session-provider
UI and Xvfb test were added.

## Not locally validated

This host has no `gtk4.pc`, `libadwaita-1.pc`, or `graphene-gobject-1.0.pc`. After clearing the
source-only `graphene-sys` cache, a normal `cargo check --all-targets --all-features --locked`
correctly stopped at missing `graphene-gobject-1.0`. No local GUI link, launch, screenshot,
display-server, accessibility, or GTK button-test result is claimed. With the GTK binary test
present, `DOCS_RS=1 cargo test --all-targets --all-features --locked --no-run` reaches native
linking and failed on unavailable GTK symbols; it is not a valid header-free substitute.
Runtime database I/O fault injection after successful startup is not covered locally or remotely.

## Remaining scope

- A native Secret Service implementation, credential create/read/update/delete tests, onboarding,
  deletion, and active switching among multiple saved profiles. The current credential path is
  deliberately session-only and does not satisfy secure credential persistence.
- Native CI evidence and central release-manifest integration for this exact Linux/Core revision;
  broader product compatibility beyond the alpha.2 startup gate remains unclaimed.
- Interoperability evidence for third-party local servers, including Ollama; automated endpoint
  coverage currently uses only LinguaMesh fake providers on loopback.
- Runtime gettext lookup, complete canonical UI coverage, and visual locale/RTL verification.
- XDG portals beyond the implemented user-data path, file workflows,
  clipboard/drag-and-drop/notifications, comprehensive
  accessibility, Wayland/X11 smoke tests, packaging, Flatpak, and release artifacts.
- Directory-descriptor or `openat2` hardening against a concurrent same-UID path replacement during
  Linux host preflight; static components are checked before mutation and Core remains the final
  no-follow open gate.
