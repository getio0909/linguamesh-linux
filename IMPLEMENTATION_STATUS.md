# Implementation Status

Status: Runtime storage ENOSPC rollback is verified locally; native Linux CI is pending

Global goal SHA-256: `11f9a65927aac7e57e2af119e9d21cc98e8d5a08b8a112a19ee1c47903e36198`

Assumption: canonical generated PO resources are synchronized and format-validated, but selecting
`zh-CN` continues to present an explicit English fallback until the GTK gettext adapter is
implemented and tested.

Assumption: the existing first-party `linguamesh-storage` crate and its already-reviewed bundled
SQLite dependency closure are the approved persistence contract for this Linux slice. No new
third-party direct dependency or native Secret Service implementation is introduced.

Assumption: the Linux GTK boundary may create `profile-<GLib UUID>` stable IDs and validate them
through Core `ProviderProfileId`; this avoids an unnecessary Core revision while names and
timestamps remain excluded from persistent identity.

Assumption: headless Weston is a test-only Ubuntu CI package. A forced Wayland GTK pass proves the
current real-widget flow without X11 fallback; it does not prove physical-compositor, GPU, desktop,
or assistive-technology compatibility.

Assumption: exhausting a private tmpfs until Linux returns `ENOSPC` verifies the implemented
post-startup SQLite transaction-failure boundary. It does not represent read-only media,
corruption, power loss, or every SQLite VFS failure.

## Implemented

- Rust 1.93.0 Cargo package at `0.1.0-alpha.2`, with locked Core alpha.2 path dependencies and
  optional `demo-provider`/`gui` features. Native CI pins Core functional revision
  `fbf3e9b5927049dccaa19f8c36013495ffebba12`.
- Startup rejects any Core other than semantic version `0.1.0-alpha.2`, ABI 1, protocol 1, provider
  catalog `0.1.0`, with the required cancellation, compatibility, typed Rust host-secret broker,
  model-discovery, streaming-text, and text-translation features.
- Toolkit-independent state starts disconnected and uses canonical Core `ProviderProfile` and
  `ProviderProfileId` values. It atomically restores a sorted multi-profile snapshot, keeps the
  selected form row, persisted default, connected saved row, and pending deletion distinct, and
  rejects duplicate IDs, missing defaults, session references, and stale results without partial
  mutation. Matching connection results atomically commit pending provider and models; failed
  switches preserve the last active provider, models, selection, and every saved row.
- Model discovery does not auto-select its first result. A saved model is restored only while it is
  still available; otherwise translation remains disabled until deliberate model selection is
  confirmed by the worker. Provider controls remain disabled until startup completes, and a
  rejected model selection resets the dropdown to the last confirmed model. A provider-setup stage
  is derived from the same reducer state as Starting, Unavailable, Configure provider, Connecting,
  Select model, or Ready. Pending model confirmation cannot claim Ready, fatal worker shutdown
  cannot remain stuck at Starting, and no wizard flag or completion marker is persisted. Worker
  shutdown or event-channel disconnection disables all controls that can send worker commands.
- The worker opens schema-2 Core storage at
  `$XDG_DATA_HOME/dev.linguamesh.LinguaMesh/linguamesh.sqlite3` (with GLib's user-data fallback),
  creates a private `0700` application directory and `0600` regular single-link database, and
  continues in explicit session-only mode after a typed storage initialization failure. On Linux's
  default Unix VFS, Core's SQLite no-follow open rejects any symbolic-link component, while the
  Linux host also rejects hard-linked, non-regular, or inode-mismatched leaf files before use.
- An explicit remember choice atomically creates or updates only provider ID, display name, preset,
  adapter, endpoint, enabled state, and the last confirmed model, then makes that row the restart
  default. Startup restores every row and default without a network request. Form selection does
  not activate or connect. Model changes update the connected ID even when another row is displayed.
  A session switch, stale model command, failed persistent switch, or connection cancelled before
  commit cannot replace any saved row or default. Each database commit must succeed before its
  corresponding runtime state and success event are committed.
- Exact-ID deletion removes one saved row transactionally. Missing rows, storage failure, active
  translation, shutdown, and stale results are typed rejections. Deleting the connected saved copy
  intentionally preserves its validated engine/model as session-only, and later model selection
  cannot silently recreate it.
- A post-startup `Persistence` failure during persistent Connect, model selection, or deletion
  rejects that exact operation before success, drops the worker storage handle and active saved
  marker, and reports storage unavailable. The prior validated engine/model remains usable as a
  session-only connection, while restart restores only state committed before the fault.
- The bounded worker uses `linguamesh_application::ProviderManager` and Core's bounded typed
  host-secret broker on a dedicated Tokio runtime. Fake-provider readiness only fills the default
  endpoint; it does not connect. Explicit Connect, SelectModel, and Translate commands drive real
  loopback model discovery and HTTP/SSE streaming. Authenticated A/B regression coverage proves
  the next translation uses only the confirmed provider/model, while a wrong-credential switch
  leaves the previous provider/model active.
- The optional credential is copied into secret-aware `SecretValue` storage, the widget is cleared
  immediately, and the temporary GTK string is dropped without claiming GTK-buffer zeroization. A
  random session `SecretRef` resolves it once through the broker. The credential and its `session:`
  reference are stripped before any profile write and must be entered again after restart. A
  persistent secret reference still fails closed because the native Secret Service backend is not
  implemented; no plaintext fallback exists.
- Connection cancellation uses a `CancellationToken`; translation cancellation uses Core's
  cross-thread handle. Both bypass command-queue backpressure. Partial output is retained, control
  commands receive priority, and a cancellation/terminal-event race remains idempotent.
- GTK 4/libadwaita source provides a saved-profile dropdown, random stable IDs for new profiles,
  provider name, endpoint, optional session credential, explicit non-secret remember/remove
  choices, connection and model selection, saved/session status, language controls, source/output
  views, Translate/Stop, typed errors, partial-result display, appearance, locale fallback notice,
  keyboard mnemonics, and redacted diagnostics. An always-current Provider setup card explains the
  next required action, warns that unavailable saved-profile storage requires session-only use, and
  keeps that warning visible through connection, model selection, and Ready while naming the
  confirmed provider stable ID/model for the next request. Selecting a row only prefills non-secret
  fields. Profile/remember/remove controls fail closed when storage is unavailable, all conflicting
  controls are blocked during connection, model selection, translation, or deletion, and event
  processing is capped per main-context tick.
- Fourteen canonical official/pseudo PO catalogs pinned to l10n revision
  `52e73ea2a6cc7e6e7409b2b6eb0d02db35576a49`. Sync rejects a different revision, dirty generated
  source artifacts, stale copies, and unexpected catalog counts.
- Foundation and native workflow sources use immutable Node 24-compatible action commits and
  disable persisted checkout credentials. Native CI pins reviewed Core revision
  `fbf3e9b5927049dccaa19f8c36013495ffebba12` and localization revision
  `52e73ea2a6cc7e6e7409b2b6eb0d02db35576a49`. The revised native gate retains serialized all-target,
  all-feature X11/Xvfb tests, runs the exact ignored storage-fault test in a private user/mount
  namespace when available, then runs the existing GTK binary test under forced Wayland and
  headless Weston before building the application. On restricted Ubuntu hosts, only the private
  mount coordinator uses passwordless `sudo`; `setpriv` returns to the original UID/GID before the
  test binary runs, clears supplementary groups plus inheritable, ambient, and bounding
  capabilities, resets the environment, and sets `no_new_privs`. The storage runner requires one
  pass, zero ignored tests, and explicit normal cleanup. The Wayland runner uses a private runtime
  directory and socket, bounded readiness, no X11 fallback, and cleanup traps.

## Local validation evidence

Validated on 2026-07-17 with Rust 1.93.0:

- The pinned global-goal SHA-256 matched the sibling authoritative file.
- Core functional revision `fbf3e9b5927049dccaa19f8c36013495ffebba12` is the reviewed source
  pin, and every direct Core dependency is constrained to `=0.1.0-alpha.2`. The clean local Core
  HEAD was a documentation-only descendant whose scoped compiled-source diff from that revision
  was empty.
- `cargo fmt --all --check`, the locked demo-provider check, strict Clippy, and build passed.
- `cargo test --no-default-features --locked` passed: 41 tests, 0 failed. Coverage includes the
  derived onboarding progression, safe stage labels, pending-model confirmation, worker-unavailable
  and storage-unavailable fallbacks, and failed-switch rollback that preserves the confirmed Ready
  identity.
- `cargo test --features demo-provider --locked` passed: 66 tests, 0 failed, with the dedicated
  namespace test intentionally ignored in the ordinary suite. Coverage includes
  explicit connection and model selection, exact compatibility rejection, authenticated one-shot
  session secrets, fail-closed persistent references, two-profile create/update/activate/restart,
  independent last models, no-auto-connect full-snapshot restore, two-credential isolation and live
  and closed SQLite artifact secret scans, inactive/missing/connected deletion, continued
  session-only translation after deleting the connected copy, exact private permissions,
  permissive-directory, symbolic-ancestor, and hard-link rejection, session fallback after storage
  failure,
  stale/cancelled/failed persistence rollback through the public command path, authenticated A/B
  one-Connect remembered switching and next-request routing with per-server request counts,
  wrong-credential rejection that preserves B and its model, restart verification, cancellable
  connection/model discovery, cancellable streaming with partial output,
  active/queued/full-command-queue shutdown, translation terminal delivery during shutdown,
  saved-model validation, and failed-switch rollback.
- `bash tools/run-storage-fault-test.sh` passed its exact ignored test separately: 1 passed, 0
  failed, 0 ignored. A private 8 MiB tmpfs produced real kernel `ENOSPC` failures for persistent
  model update, deletion, and provider switch; each preserved prior-session translation, and each
  restart exposed only pre-fault state. Post-fault model selection also succeeded only in session
  mode and remained absent after restart. No temporary mount or directory remained afterward. This
  host used the unprivileged path; passwordless `sudo` is unavailable, so the controlled fallback
  awaits the first Native Linux CI run.
- `DOCS_RS=1 cargo check --all-targets --all-features --locked` and the equivalent strict Clippy
  command passed only as source-level diagnostics of the GTK code.
- The exact foundation block in `docs/testing.md`, `git diff --check`, shell syntax validation, and
  code-comment/secret-pattern scans passed. The new Wayland runner also passed Bash syntax and its
  expected missing-Weston and early-compositor-exit failure paths without leaving a temporary
  runtime directory; both workflow files parsed as YAML. Its actual GTK run requires the remote
  native environment.
- `bash tools/sync-l10n.sh --check` passed against the exact clean l10n checkout, and all 14 PO
  catalogs passed `msgfmt --check --check-format`.
- The checkout, Rust-toolchain, and Rust-cache action SHAs resolved through the GitHub commits API;
  their action metadata uses Node 24 or a composite action.

## Remote validation evidence

The runtime storage write-fault change has not yet completed its first Native Linux CI run. The
following evidence remains the latest completed remote gate until that run is recorded.

Wayland-gate revision `10b31a040fd3c44ecbaef31eb5c66c0c8e5cb620` passed
repository-foundation run `29582513073` (job `87891382540`) and Native Linux run `29582513061`
(job `87891382469`). The native Ubuntu 24.04 job checked out exact Core revision
`fbf3e9b5927049dccaa19f8c36013495ffebba12`, synchronized localization, passed formatting and
strict all-feature Clippy, passed 65 library tests plus the real GTK binary test under serialized
X11/D-Bus/Xvfb, reran the same GTK binary test under forced Wayland/headless Weston with no X11
fallback, and built all targets with all features. The Wayland result covers the existing widget
flow only; physical compositors, GPU rendering, and assistive technology remain unclaimed.

Functional onboarding revision `9729b23ce1a4280ebb434339e880010103b4859d` passed
repository-foundation run `29580444697` and Native Linux run `29580444723` (job `87884607879`).
The native Ubuntu 24.04 job checked out exact Core revision
`fbf3e9b5927049dccaa19f8c36013495ffebba12`, synchronized localization, passed formatting and
strict all-feature Clippy, passed 65 library tests plus the real GTK binary test under serialized
X11/D-Bus/Xvfb, and built all targets with all features. The GTK test exercised the derived setup
stages, stable next-request identity, pending-model state, persistent storage-degradation warning,
worker-unavailable command blocking, the existing multi-profile lifecycle, explicit connection and
model selection, failed-switch rollback, and streamed translation through real widgets.

Functional multi-profile revision `c88d37a5de2f03c2ae5d2940c4d25e5d998c301d` passed
repository-foundation run `29577918346` and Native Linux run `29577918335` (job `87876528763`).
The native Ubuntu 24.04 job checked out exact Core revision
`fbf3e9b5927049dccaa19f8c36013495ffebba12`, synchronized localization, passed formatting and
strict all-feature Clippy, passed 62 library tests plus the real GTK binary test under serialized
X11/D-Bus/Xvfb, and built all targets with all features. The GTK test exercised persistent-secret
fail-closed behavior, disabled-row preservation, form-only selection, exact deletion, random draft
identity, session credential handling, explicit model selection, failed-switch rollback, and
streaming translation through real widgets.

Functional persistence revision `c58a54c2479045773358bd9c456b45a958e98e1e` passed
repository-foundation run `29574265553` and Native Linux run `29574265570` (job `87865028892`).
The native Ubuntu 24.04 job checked out exact Core revision
`fbf3e9b5927049dccaa19f8c36013495ffebba12`, synchronized localization, passed formatting and
strict all-feature Clippy, passed 50 library tests plus the real GTK binary test under serialized
X11/D-Bus/Xvfb, and built all targets with all features.

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

This host has no `gtk4.pc`, `libadwaita-1.pc`, `graphene-gobject-1.0.pc`, or `weston`. After clearing the
source-only `graphene-sys` cache, a normal `cargo check --all-targets --all-features --locked`
correctly stopped at missing `graphene-gobject-1.0`. No local GUI link, launch, screenshot,
display-server, accessibility, or GTK button-test result is claimed. In particular, the Wayland
runner was not executed against a local compositor. With the GTK binary test
present, `DOCS_RS=1 cargo test --all-targets --all-features --locked --no-run` reaches native
linking and failed on unavailable GTK symbols; it is not a valid header-free substitute.
The Provider setup and multi-profile flows passed their real widget test, native linking, and build
in the GitHub Actions evidence above, but those native checks remain unavailable on this local host.

## Remaining scope

- A native Secret Service implementation, credential create/read/update/delete tests, and secure
  persistent-credential onboarding. Guided non-secret/session setup and multiple-profile
  create/update/switch/delete are implemented, but the current credential path is deliberately
  session-only and does not satisfy secure credential persistence.
- Central release-manifest integration for this exact Linux/Core revision; broader product
  compatibility beyond the alpha.2 startup gate remains unclaimed.
- Interoperability evidence for third-party local servers, including Ollama; automated endpoint
  coverage currently uses only LinguaMesh fake providers on loopback.
- Runtime gettext lookup, complete canonical UI coverage, and visual locale/RTL verification.
- Runtime database faults beyond the verified private-tmpfs `ENOSPC` transaction boundary,
  including read-only media, corruption, power loss, and broader SQLite VFS failures.
- XDG portals beyond the implemented user-data path, file workflows,
  clipboard/drag-and-drop/notifications, comprehensive
  accessibility, physical-compositor/GPU Wayland coverage, broader X11/desktop coverage, packaging,
  Flatpak, and release artifacts.
- Directory-descriptor or `openat2` hardening against a concurrent same-UID path replacement during
  Linux host preflight; static components are checked before mutation and Core remains the final
  no-follow open gate.
