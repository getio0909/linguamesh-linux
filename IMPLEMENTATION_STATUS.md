# Implementation Status

Status: Core alpha.2 session-secret slice verified locally and in native Linux CI

Global goal SHA-256: `11f9a65927aac7e57e2af119e9d21cc98e8d5a08b8a112a19ee1c47903e36198`

Assumption: canonical generated PO resources are synchronized and format-validated, but selecting
`zh-CN` continues to present an explicit English fallback until the GTK gettext adapter is
implemented and tested.

## Implemented

- Rust 1.93.0 Cargo package at `0.1.0-alpha.2`, with locked Core alpha.2 path dependencies and
  optional `demo-provider`/`gui` features. Native CI pins Core functional revision
  `c9a96da52e10554c8458f4d49600ec9336ea651b`.
- Startup rejects any Core other than semantic version `0.1.0-alpha.2`, ABI 1, protocol 1, provider
  catalog `0.1.0`, with the required cancellation, compatibility, typed Rust host-secret broker,
  model-discovery, streaming-text, and text-translation features.
- Toolkit-independent state starts disconnected and uses canonical Core `ProviderProfile` and
  `ProviderProfileId` values. Matching connection results atomically commit pending provider and
  models; stale results and failed switches preserve the last active provider, models, and
  selection.
- Model discovery does not auto-select its first result. A saved model is restored only while it is
  still available; otherwise translation remains disabled until deliberate model selection is
  confirmed by the worker.
- The bounded worker uses `linguamesh_application::ProviderManager` and Core's bounded typed
  host-secret broker on a dedicated Tokio runtime. Fake-provider readiness only fills the default
  endpoint; it does not connect. Explicit Connect, SelectModel, and Translate commands drive real
  loopback model discovery and HTTP/SSE streaming.
- The optional credential is copied into secret-aware `SecretValue` storage, the widget is cleared
  immediately, and the temporary GTK string is dropped without claiming GTK-buffer zeroization. A
  random session `SecretRef` resolves it once through the broker. Neither it nor the provider
  profile is persisted. Persistent intent and persistent secret references fail closed because the
  native Secret Service backend is not implemented; no plaintext fallback exists.
- Connection cancellation uses a `CancellationToken`; translation cancellation uses Core's
  cross-thread handle. Both bypass command-queue backpressure. Partial output is retained, control
  commands receive priority, and a cancellation/terminal-event race remains idempotent.
- GTK 4/libadwaita source provides provider name, endpoint, optional session credential, explicit
  connection and model selection, active-provider status, language controls, source/output views,
  Translate/Stop, typed errors, partial-result display, appearance, locale fallback notice,
  keyboard mnemonics, and redacted diagnostics. Provider/model/language controls are blocked during
  connection or translation, and event processing is capped per main-context tick.
- Fourteen canonical official/pseudo PO catalogs pinned to l10n revision
  `52e73ea2a6cc7e6e7409b2b6eb0d02db35576a49`. Sync rejects a different revision, dirty generated
  source artifacts, stale copies, and unexpected catalog counts.
- Foundation and native workflow sources use immutable Node 24-compatible action commits and
  disable persisted checkout credentials. Native CI pins reviewed Core revision
  `c9a96da52e10554c8458f4d49600ec9336ea651b` and localization revision
  `52e73ea2a6cc7e6e7409b2b6eb0d02db35576a49`. The revised native gate installs D-Bus/Xvfb support
  and runs serialized all-target, all-feature tests under X11 before building the application.

## Local validation evidence

Validated on 2026-07-17 with Rust 1.93.0:

- The pinned global-goal SHA-256 matched the sibling authoritative file.
- Core functional revision `c9a96da52e10554c8458f4d49600ec9336ea651b` is the reviewed source
  pin, and every direct Core dependency is constrained to `=0.1.0-alpha.2`. The clean local Core
  HEAD was documentation-only descendant `418fd48e5f64e6758947dda8e33306db887bc978`; a scoped diff
  confirmed its compiled Cargo, crate, asset, and migration sources match the functional pin.
- `cargo fmt --all --check`, the locked demo-provider check, strict Clippy, and build passed.
- `cargo test --no-default-features --locked` passed: 23 tests, 0 failed.
- `cargo test --features demo-provider --locked` passed: 35 tests, 0 failed. Coverage includes
  explicit connection and model selection, exact compatibility rejection, authenticated one-shot
  session secrets, fail-closed persistent references, cancellable connection/model discovery,
  cancellable streaming with partial output, active/queued/full-command-queue shutdown,
  translation terminal delivery during shutdown, saved-model validation, and failed-switch
  rollback.
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

## Remaining scope

- A native Secret Service implementation, persistent provider profiles and model preferences,
  restart restoration, onboarding, and active switching among saved profiles. The current
  credential path is deliberately session-only and does not satisfy secure-profile persistence.
- Central release-manifest integration and broader product compatibility beyond the exact alpha.2
  startup gate implemented here.
- Interoperability evidence for third-party local servers, including Ollama; automated endpoint
  coverage currently uses only LinguaMesh fake providers on loopback.
- Runtime gettext lookup, complete canonical UI coverage, and visual locale/RTL verification.
- XDG/portal integration, file workflows, clipboard/drag-and-drop/notifications, comprehensive
  accessibility, Wayland/X11 smoke tests, packaging, Flatpak, and release artifacts.
