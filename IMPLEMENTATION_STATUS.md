# Implementation Status

Status: Runtime storage ENOSPC rollback, forced Wayland/X11 GTK gates, baseline GTK accessibility semantics, runtime catalog-backed workspace/status/theme localization, the GIO Secret Service adapter, generic completion desktop notifications, bounded native text-file import with source-editor drag-and-drop, the corrected Secret Service session wire shape, isolated real-daemon Secret Service CRUD plus persistent restart/locked lifecycle fixtures, secure persistent-credential onboarding, fail-closed Secret Service prompted-flow handling, a remotely built pinned Flatpak bundle with bounded sandbox startup, private notification-service transport validation, headless real notification-daemon delivery, physical desktop-shell notification rendering, a real XDG document-portal lease lifecycle fixture, a real interactive portal FileChooser backend fixture, application-level GTK FileDialog callbacks, and an actual GTK source-editor drag/drop gesture fixture are implemented; end-user prompt acceptance and release artifacts remain open

Global goal SHA-256: `11f9a65927aac7e57e2af119e9d21cc98e8d5a08b8a112a19ee1c47903e36198`

Assumption: canonical generated PO/MO resources are synchronized and format-validated. The GTK host
now parses all twelve pinned official Linux MO catalogs at runtime, exposes BCP 47 locale choices,
switches the root direction for Arabic, and preserves the source editor buffer during a locale
switch; status summaries, partial-output markers, text-file import controls, provider-profile
controls, and source/target language options now use the same catalogs; complete UI coverage,
plural handling, and visual locale/RTL review remain open.

Assumption: the existing first-party `linguamesh-storage` crate and the already-reviewed GTK/GIO
dependency closure are the approved persistence contract for this Linux slice. The Secret Service
adapter uses GIO D-Bus calls and adds no third-party direct dependency; a desktop keyring remains
an external runtime prerequisite.

Assumption: the Linux GTK boundary may create `profile-<GLib UUID>` stable IDs and validate them
through Core `ProviderProfileId`; this avoids an unnecessary Core revision while names and
timestamps remain excluded from persistent identity.

Assumption: headless Weston is a test-only Ubuntu CI package. A forced Wayland GTK pass proves the
current real-widget flow without X11 fallback; it does not prove physical-compositor, GPU, desktop,
or assistive-technology compatibility.

Assumption: exhausting a private tmpfs until Linux returns `ENOSPC` verifies the implemented
post-startup SQLite transaction-failure boundary. It does not represent read-only media,
corruption, power loss, or every SQLite VFS failure.

Assumption: enabling gtk-rs `v4_10` is an existing-dependency API feature, not a new third-party
package. Ubuntu 24.04 native CI is the compatibility gate for this GTK 4.10-or-newer surface;
older distributions and future Flatpak runtimes require separate packaging validation.

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
  reference are stripped before any profile write and must be entered again after restart. Persistent
  secret references are stored and resolved through the Linux Secret Service adapter;
  unavailable or locked keyrings fail closed and never fall back to plaintext. Secret-item cleanup,
  persistent desktop restoration after daemon restart, locked-item fail-closed behavior, and the
  secure persistent-credential onboarding path are covered by the isolated CI daemon fixture.
- Connection cancellation uses a `CancellationToken`; translation cancellation uses Core's
  cross-thread handle. Both bypass command-queue backpressure. Partial output is retained, control
  commands receive priority, and a cancellation/terminal-event race remains idempotent.
- GTK 4/libadwaita source provides a saved-profile dropdown, random stable IDs for new profiles,
  provider name, endpoint, optional session credential, explicit non-secret remember/remove
  choices, connection and model selection, saved/session status, language controls, source/output
  views, Translate/Stop, typed errors, partial-result display, appearance, runtime catalog-backed
  action, workspace-widget, active-provider, status summary/partial-output, text-file import, provider-profile, source/target language, and theme-option labels with an explicit
  fallback notice, a generic completion desktop notification that
  excludes source and translated content, bounded native UTF-8 TXT/Markdown import through GTK
  `FileDialog`/GIO, keyboard mnemonics, and redacted diagnostics. An
  always-current Provider setup card explains the
  next required action, warns that unavailable saved-profile storage requires session-only use, and
  keeps that warning visible through connection, model selection, and Ready while naming the
  confirmed provider stable ID/model for the next request. Selecting a row only prefills non-secret
  fields. Profile/remember/remove controls fail closed when storage is unavailable, all conflicting
  controls are blocked during connection, model selection, translation, or deletion, and event
  processing is capped per main-context tick.
- The GTK boundary provides baseline accessibility semantics: `Main`, `Heading`, `Status`, and
  `Alert` roles; named multi-line source/output `TextBox` editors with output read-only; visible
  label-to-control `LabelledBy` and mnemonic relations; focusable editor and action controls; an
  explicit `Stop translation` accessible name; output `Busy` state during translation with terminal
  reset; and an accessibility-hidden empty error label. These are semantic wiring guarantees, not
  AT-SPI/Orca, physical-keyboard, RTL, high-contrast, or full desktop accessibility evidence.
- The Linux host now uses existing GIO D-Bus bindings for Secret Service `OpenSession`, item search,
  create/update, and `GetSecret` resolution. Persistent profiles retain only a SecretRef; the
  one-shot credential is passed through the existing typed broker and is never written to SQLite.
- Fourteen canonical official/pseudo PO/MO catalog pairs containing 172 messages pinned to l10n revision
  `cc841103c3480ece237baa088bbb5881a321cf0a`. Sync rejects a different revision, dirty generated
  source artifacts, stale copies, and unexpected catalog counts. The GTK locale selector exposes
  the twelve official packs, runtime action, workspace-widget, active-provider, status summary,
  partial-output, text-file import, provider-profile, source/target language, onboarding stage/detail,
  fixed provider/file/worker and reducer-state/category error messages, and
  System/Light/Dark theme labels switch without losing state, preserves source text while moving
  from Simplified Chinese to Arabic, and applies right-to-left root direction; uncovered UI strings
  still use explicit English fallbacks.
- Foundation and native workflow sources use immutable Node 24-compatible action commits and
  disable persisted checkout credentials. Native CI pins reviewed Core revision
  `fbf3e9b5927049dccaa19f8c36013495ffebba12` and localization revision
  `cc841103c3480ece237baa088bbb5881a321cf0a`. The revised native gate retains serialized all-target,
  all-feature X11/Xvfb tests, runs the exact ignored storage-fault test in a private user/mount
  namespace when available, then runs the existing GTK binary test under forced Wayland and
  headless Weston before building the application. On restricted Ubuntu hosts, only the private
  mount coordinator uses passwordless `sudo`; `setpriv` returns to the original UID/GID before the
  test binary runs, clears supplementary groups plus inheritable, ambient, and bounding
  capabilities, resets the environment, and sets `no_new_privs`. The storage runner requires one
  pass, zero ignored tests, and explicit normal cleanup. The Wayland runner uses a private runtime
  directory and socket, bounded readiness, no X11 fallback, and cleanup traps.

## Local validation evidence

Validated on 2026-07-18 with Rust 1.93.0:

- The pinned global-goal SHA-256 matched the sibling authoritative file.
- Core functional revision `fbf3e9b5927049dccaa19f8c36013495ffebba12` is the reviewed source
  pin, and every direct Core dependency is constrained to `=0.1.0-alpha.2`. The clean local Core
  HEAD was a documentation-only descendant whose scoped compiled-source diff from that revision
  was empty.
- `cargo fmt --all --check`, the locked demo-provider check, strict Clippy, and build passed.
- `cargo test --no-default-features --locked` passed: 45 tests, 0 failed. Coverage includes the
  derived onboarding progression, safe stage labels, pending-model confirmation, worker-unavailable
  and storage-unavailable fallbacks, and failed-switch rollback that preserves the confirmed Ready
  identity.
- `cargo test --features demo-provider --locked` passed: 70 tests, 0 failed, with the dedicated
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
- `bash tools/run-secret-service-test.sh` is a native-only check on this host because GTK headers
  are unavailable locally; native CI uses a real `gnome-keyring` daemon with a persistent `login`
  collection, verifies store/resolve, locks an item and checks fail-closed resolution, restarts the
  daemon, resolves and deletes the item, runs the worker secure-onboarding connect/translate and
  restart path, and runs the GTK Remember/clear-form/real-authenticated-provider path under Xvfb.
- `bash tools/run-secret-service-prompt-test.sh` passed locally. Its isolated Python D-Bus fixture
  returns a non-root prompt path for `CreateItem` and `Delete`; both exact ignored tests passed and
  asserted `SecureStorageUnavailable` with the stable interactive-prompt message. The fixture does
  not automate user approval or unlock UI.
- The loopback OpenAI-compatible path connects without a credential, manually selects a discovered
  model, streams `你好，LinguaMesh！`, and counts one chat request against the isolated fake provider.
  The notification slice keeps the desktop payload to localized generic copy and sends no
  source or translated content. The native transport fixture uses a private
  `org.freedesktop.Notifications` service and verifies the real `Notify` call plus the generic
  payload. A second native fixture starts the real `dunst` notification daemon under Xvfb, waits
  for it to own the service, verifies delivery of the same redacted payload, and asserts a visible
  viewable Dunst desktop-shell window through X11 window inspection.
- The Secret Service adapter now sends an `(sv)` `OpenSession` request with a single plain-string
  Variant; its shape regression passed locally. The isolated real-daemon fixture is wired into
  native CI and covers persistent daemon-restart restoration, locked-item fail-closed resolution,
  cleanup, worker credential resolution, and GTK secure persistent-credential onboarding. The
  prompted-flow fixture also verifies that non-root `CreateItem` and `Delete` prompt paths are
  rejected with `SecureStorageUnavailable`; end-user prompt acceptance remains open.
- The native text import slice accepts only UTF-8 TXT/Markdown content up to 4 MiB, strips a UTF-8
  BOM, rejects invalid or oversized input, and reads through GIO's partial asynchronous API. The
  source editor also accepts a single URI-list/GIO file through GTK drag-and-drop and reuses the
  same validation path. Decoder tests and source-level checks passed locally. The real XDG document
  portal fixture verifies add, host-path mapping, application permission grant/revoke, and lease
  deletion. Native CI drives the real `xdg-desktop-portal-gtk` FileChooser under Xvfb, injects a
  temporary UTF-8 fixture path, verifies the application's asynchronous GTK FileDialog callback,
  and then performs a real XTest drag through the source editor to the GIO import callback.
- `bash tools/validate-flatpak-metadata.sh` passed locally. It parsed the Flatpak manifest and
  Cargo source set, verified immutable Linux/Core source pins and archive hashes, and passed
  `desktop-file-validate` plus `appstreamcli`. The manifest uses the GNOME 49 SDK, installs the
  native binary and desktop metadata, and declares only the runtime interfaces required for the
  current Linux surface. The `Flatpak Linux` workflow runs this manifest in a GNOME 49 SDK
  container, uploads a prerelease CI bundle, and runs the bounded Xvfb/private-D-Bus sandbox smoke;
  local `flatpak-builder` is unavailable, so the SDK build and sandbox smoke remain remote-only.
  Release-artifact reproducibility remains a separate gate; prompted chooser/keyring flows remain
  manual boundaries.
- `bash tools/run-storage-fault-test.sh` passed its exact ignored test separately: 1 passed, 0
  failed, 0 ignored. A private 8 MiB tmpfs produced real kernel `ENOSPC` failures for persistent
  model update, deletion, and provider switch; each preserved prior-session translation, and each
  restart exposed only pre-fault state. Post-fault model selection also succeeded only in session
  mode and remained absent after restart. No temporary mount or directory remained afterward. This
  host used the unprivileged path; the controlled `sudo` fallback passed in the remote evidence
  below.
- `DOCS_RS=1 cargo check --all-targets --all-features --locked` and the equivalent strict Clippy
  command passed only as source-level diagnostics of the GTK code, including the `v4_10`
  accessibility APIs and the test assertions. The native GTK binary test for this slice remains a
  remote-only gate on this host.
- The exact foundation block in `docs/testing.md`, `git diff --check`, shell syntax validation, and
  code-comment/secret-pattern scans passed. The new Wayland runner also passed Bash syntax and its
  expected missing-Weston and early-compositor-exit failure paths without leaving a temporary
  runtime directory; both workflow files parsed as YAML. Its actual GTK run requires the remote
  native environment.
- The current status-localization slice passed `DOCS_RS=1 cargo check --all-targets --all-features
  --locked`, strict library Clippy, 45 no-default tests, 4 localization tests, and
  `bash tools/sync-l10n.sh --check`. The GTK test asserts Simplified Chinese and Arabic status
  summaries alongside the existing action, widget, theme, source-preservation, and RTL checks in
  the remote native gate.
- The text-import localization slice passed the same source-level checks and adds localized **Open
  text file**, tooltip, file-filter, and native chooser labels; the GTK fixture continues to verify
  the asynchronous UTF-8 import callback and drag/drop path in the remote native gate.
- The provider-profile localization slice passed `cargo fmt`, all-target all-feature locked check and
  Clippy, 45 no-default tests, 4 localization tests, and `bash tools/sync-l10n.sh --check`. It adds
  23 Linux-only provider-card, tooltip, action, and source/target language messages; the GTK test
  asserts Simplified Chinese provider controls and language options while preserving the existing
  Arabic RTL/source-buffer checks.
- The onboarding localization slice passed the same source-level checks and adds 17 Linux-only
  stage/detail messages with `{provider}`, `{profile_id}`, and `{model}` runtime substitutions;
  the GTK onboarding card now localizes every derived stage and persistence warning.
- The fixed-error localization slice passed the same source-level checks and adds 10 Linux-only
  provider, saved-profile, text-import, and worker-disconnect/stop messages; GTK validation now
  resolves these fixed errors through the active runtime locale while preserving dynamic backend
  diagnostics as explicit English fallbacks.
- The reducer-state localization slice adds 31 Linux-only category and fixed state-error messages;
  `localized_error_text` translates stable reducer failures and category prefixes while preserving
  dynamic backend diagnostics as explicit English fallbacks. Local validation passed all-target
  locked check, strict Clippy, 50 no-default tests, 4 localization tests, the targeted localized
  error test, and `bash tools/sync-l10n.sh --check` against l10n `08118b498646ebf56cbb072b937d95fceb34b75c`.
- Linux MO integration revision `daa19923d5dfd4f8d00801f067569daf78a98ab0` adds deterministic
  GNU MO companions for all 14 PO catalogs, switches the runtime parser to MO tables, and
  validates the generated Simplified Chinese state-error translation. Local validation passed
  locked all-target check, strict Clippy, 51 no-default tests, 5 localization tests, and sync
  validation against l10n `0b906034784a1b5e81a879649abbfda001fa9e67`.
- Linux worker/file/storage/provider error coverage adds 24 fixed and detail-bearing message keys,
  including invalid UTF-8 import, storage fallback, provider/model state, secret-channel, and
  profile validation failures. Local validation passed `cargo test --locked` (51 tests),
  `cargo test --features gui --lib localization::tests --locked` (5 tests), all-target locked
  check, strict all-feature Clippy, and `bash tools/sync-l10n.sh --check` against l10n
  `cc841103c3480ece237baa088bbb5881a321cf0a`.
- Linux revision `7a8526f7a1a0e3cfe068e3dd20934cf3e11d18ca` adds a GTK regression that sets source
  text in Simplified Chinese, switches to Arabic, verifies RTL direction, and asserts the source
  buffer is unchanged. Native run `29623544194` (job `88023275325`), Foundation run `29623544187`,
  and Flatpak run `29623544225` passed.
- `bash tools/sync-l10n.sh --check` passed against the exact clean l10n checkout, all 14 PO
  catalogs passed `msgfmt --check --check-format`, and `msgunfmt` read the generated MO table.
- The Linux localization unit suite parsed all twelve official catalogs and verified non-empty
  application/action entries, unique BCP 47 tags, and Arabic RTL metadata. `cargo test --features
  gui --lib localization::tests --locked` passed 4 tests; the portable model suite passed 45 tests.
- Linux evidence revision `b6d2503` passed Native Linux run `29627668119`, Foundation run
  `29627668093`, and Flatpak run `29627668108`;
  the native gate covered the 117-message pinned catalog, localized provider controls, source/target
  language and onboarding stage/detail guidance,
  language labels, and the existing X11/Wayland, storage, Secret Service, portal, notification,
  drag/drop, and accessibility fixtures.
- Linux reducer-state localization revision `9f21836f214d3056934fac9322adc0f20791834e` passed
  Native Linux run `29628307915`, Foundation run `29628307945`, and Flatpak run `29628307886`;
  the native gate covered the 148-message pinned catalog, localized fixed reducer errors and
  category prefixes, and the existing X11/Wayland, storage, Secret Service, portal, notification,
  drag/drop, and accessibility fixtures.
- Linux MO integration revision `6c5bfb305967d0f01488ad09ade6e5b88eebbdb0` passed Native Linux
  run `29628986188`, Foundation run `29628986160`, and Flatpak run `29628986187`; the native gate
  validated all 14 PO catalogs with `msgfmt`, all 14 MO catalogs with `msgunfmt`, and runtime MO
  lookup through the existing locale, error, RTL, storage, portal, notification, and accessibility
  fixtures. The workflow pins l10n revision `0b906034784a1b5e81a879649abbfda001fa9e67`.
- The checkout, Rust-toolchain, and Rust-cache action SHAs resolved through the GitHub commits API;
  their action metadata uses Node 24 or a composite action.

## Remote validation evidence

Secret Service session-parameter fix revision `9bcd8d9ca30d109f5c7c9c20e6f72f6a77df078d` passed
repository-foundation run `29598255993` and Native Linux run `29598255988` (job `87943844854`).
The Ubuntu 24.04 job passed strict all-feature Clippy, the GTK-enabled test suite, both display
gates, the private-tmpfs storage fault test, and the all-target native build after the wire-shape
regression was added. Persistent desktop keyring restoration and locked/prompted behavior remain
unverified because the CI session has no unlocked desktop keyring.

Secret Service CRUD functional revision `726508f8412727f8b14e32d27407487491f5e4cd` passed
repository-foundation run `29600898977` and Native Linux run `29600898951` (job `87952473459`).
The Ubuntu 24.04 job passed strict all-feature Clippy, 75 library tests with 2 intentional ignores,
the real GTK binary test under X11/Xvfb and forced Wayland/headless Weston, the exact storage-fault
test, the isolated real-daemon store/resolve/delete fixture (1 passed), and the all-target native
build. This proves the session wire shape, default-collection CRUD, secret resolution, and cleanup
against an isolated `gnome-keyring` daemon; persistent desktop keyring restoration and locked/prompted
behavior remain separate lifecycle gates.

Secret Service persistent-lifecycle revision `f58388a8e58341a8630088dc8b1782f61ab63a7c` passed
repository-foundation run `29602287281` and Native Linux run `29602287284` (job `87957053225`).
The Ubuntu 24.04 job stored a persistent item in the `login` collection, verified resolution,
locked the collection and observed fail-closed lookup, restarted the daemon, resolved and deleted
the item, and reran the isolated cleanup round trip. Secure persistent-credential onboarding and
prompted interactive flows remain separate gates.

Secure persistent-credential onboarding revision `6654a46b378d68c2c6012ccf2f30e24ae564dc7c` passed
repository-foundation run `29603486477` and Native Linux run `29603486498` (job `87960961963`).
The isolated fixture stored a persistent credential, connected and translated through the worker
without credential re-entry, restarted and reconnected from the restored SecretRef, and ran the
GTK onboarding path that clears the credential form, persists only the SecretRef, authenticates a
real loopback fake provider, translates, and verifies the database contains no credential canary.
Prompted interactive flows remain a separate gate.

Notification transport fixture revision `bf751479c3826ae1529d0d9c33effbc5212cd75f` passed
repository-foundation run `29609857686` and Native Linux run `29609857730` (job `87981724178`).
The Ubuntu 24.04 job ran the real GTK translation test with a private notification-service
implementation, captured `org.freedesktop.Notifications.Notify`, and verified the fixed generic
title/body without source or translated content. Earlier fixture revisions `124ab16` and `3def30a`
remain retained failures: the first listened on a bus without a private session and the second
corrected that session but still had no notification service. Desktop-shell rendering, portal leases,
and prompted interactive flows remain separate gates.

Document portal lease revision `7fbd65f08ebffa55777e0d7804d270fe683ca6c6` passed
repository-foundation run `29611395903` and Native Linux run `29611395919` (job `87986665827`).
The Ubuntu 24.04 job installed the real XDG document portal services and verified add, host-path
mapping, application read-permission grant/revoke, and lease deletion against a private temporary
fixture. This proves the document-portal lease lifecycle, not interactive GTK file chooser or
drag-and-drop gestures; those remain separate gates.

Interactive portal chooser revision `59bed27` passed Native Linux run `29615157729` (job
`87998524591`), repository-foundation run `29615157686`, and Flatpak Linux run `29615157675`.
The native Ubuntu 24.04 job started the real `xdg-desktop-portal-gtk` chooser under Xvfb, used the
actual `FileChooser.OpenFile` request, selected a temporary UTF-8 fixture through the visible
dialog, and verified the returned URI and file contents. This is backend portal UI/lease evidence.

Application GTK interaction revision `24948fbc75cdf101d2279964dd45e1489ce7bb18` passed Native
Linux run `29619211510` (job `88010683331`) and repository-foundation run `29619211581`; the
Flatpak run `29619211521` passed. The native job verified the asynchronous application
`FileDialog` callback and GIO UTF-8 read, then used XTest pointer motion to start a real GTK drag,
enter and move over the source editor, and complete the URI-list import callback. This closes the
application-level chooser and source-editor gesture boundaries under Xvfb.

Notification daemon rendering revision `0d2d6ed` passed Native Linux run `29619768430` (job
`88012305004`), Foundation run `29619768408`, and Flatpak run `29619768331`. The native Ubuntu
24.04 job started the real `dunst` server under Xvfb, observed its
`org.freedesktop.Notifications` name on the session bus, ran the GTK translation flow, verified
the received `Notify` payload stayed generic and contained neither source nor translated text, and
asserted a visible viewable Dunst window through X11 inspection. Physical compositor and GPU
coverage remain separate.

Loopback provider revision `7d7eba9960b657f0460fb0daaaaebaaa609f39b1` passed repository-foundation
run `29604269516` and Native Linux run `29604269568` (job `87963611054`). The Ubuntu 24.04 job
added a no-credential OpenAI-compatible loopback connection, manual model selection, streamed
translation, and request-count assertion to the ordinary worker suite; all native validation,
display gates, Secret Service fixtures, and the all-target build passed.

Flatpak packaging revision `fd1f400058f4c68b47a9bd0823e790c6d9cef263` passed the `Flatpak Linux`
workflow run `29608245156` (job `87976563401`). The GNOME 49 container mounted the pinned Rust
1.93.0 toolchain, built the optimized GTK application with `cargo build --release --locked
--offline --features gui`, installed the binary and desktop metadata, removed the build-only Rust
toolchain from `/app`, and uploaded CI artifact `linguamesh-linux-x86_64-x86_64.flatpak` (artifact
ID `8417803048`, 2,395,628 bytes). The same workflow installed the bundle under Xvfb and a private
D-Bus session, confirmed runtime `org.gnome.Platform/x86_64/49`, and held application startup for
the bounded timeout. Earlier packaging runs are retained as failures: run
`29605863496` stopped at missing Cargo, `29606146197` exposed the Rust 1.89 minimum-version mismatch,
and `29606402301` exposed Flatpak debug extraction corrupting the temporary Rust toolchain; the
current manifest fixes each boundary. The earlier GNOME 48 bundle run `29606612834` remains historical;
this is a prerelease CI bundle, not a signed or published release; portal/notification delivery is
not claimed.

Linux drag-and-drop functional revision `b0da3819d97ae24f8c85147da5e7e1c65fe2d6fc` passed
repository-foundation run `29597016893` (job `87939785643`) and Native Linux run `29597016894`
(job `87939785693`). The Ubuntu 24.04 job passed strict all-feature Clippy, 71 GUI-enabled
library tests with one intentional ignore, the real GTK flow with the single-file source-editor
`DropTarget`, the private-tmpfs storage fault test, both display gates, and the all-target native
build. Interactive drag/drop gestures and portal-specific file leases remain manual boundaries.

Native text-import functional revision `96d34a5448d0f718fd87c68e88129c05fed43ee5` passed
repository-foundation run `29596052213` (job `87936587361`) and Native Linux run `29596052224`
(job `87936587342`). The Ubuntu 24.04 job passed strict all-feature Clippy, 70 GUI-enabled
library tests with one intentional ignore, the real GTK flow including the focusable Open text
file control and worker-loss disablement, the private-tmpfs storage fault test, both display gates,
and the all-target native build. Interactive file selection, portal leases, and drag-and-drop are
not claimed by this evidence.

Desktop-notification functional revision `07b89f36269155469a488ab830e8f485b3a1323b` passed
repository-foundation run `29594795681` (job `87932451692`) and Native Linux run `29594795691`
(job `87932451631`). The Ubuntu 24.04 job passed strict all-feature Clippy, 68 GUI-enabled
library tests with one intentional ignore, the real GTK test and its generic notification path,
the private-tmpfs storage fault test, the X11/Xvfb and forced Wayland/headless Weston GTK flow,
and the all-target native build. Desktop notification-server delivery and packaging integration
remain runtime/release boundaries.

Runtime localization validation revision `1dfe2bcac684696ee55f56e625fcf89ffcb1a6dd` passed
repository-foundation run `29593874763` (job `87929412298`) and Native Linux run `29593874961`
(job `87929412911`). The Ubuntu 24.04 job passed 71 GUI-enabled library tests with one
intentional ignore, the runtime catalog and GTK action-label assertions, the private-tmpfs storage
fault test, the X11/Xvfb and forced Wayland/headless Weston GTK flow, and the all-target build.
The catalogs are embedded from the pinned PO snapshot; complete UI gettext coverage and human
review of non-English drafts remain open.

Secret Service validation revision `81be457fc6cefcaebff6c6afd61408d6eb6900b3` passed
repository-foundation run `29592320055` (job `87924170620`) and Native Linux run
`29592319844` (job `87924169888`). The Ubuntu 24.04 job passed strict all-feature Clippy, 68
library tests with one intentional ignore, the GTK binary test under X11/Xvfb and forced
Wayland/headless Weston, the private-tmpfs storage-fault test, and the all-target all-feature
build. The test environment has no desktop Secret Service implementation, so this evidence covers
compile-time GIO integration and fail-closed behavior only; real keyring CRUD and cleanup remain
explicitly unverified.

Accessibility functional revision `d6bd2bd06ccdf04f3aead0c7f1da5ba74f84c550` passed repository-foundation
run `29589043314` (job `87913221612`) and Native Linux run `29589043315` (job `87913221576`). The
Ubuntu 24.04 job passed strict all-feature Clippy, 66 ordinary library tests with one intentional
ignore, the real GTK binary test under X11/Xvfb with the accessibility role/property/relation,
mnemonic, focusability, hidden-error, and Busy-reset assertions, the exact storage-fault test,
the same GTK test under forced Wayland/headless Weston, and the all-target all-feature build. The
preceding functional revision `e483ad8b9ff0fb9e35fd531e69959c1eb81e7e34` failed only because the
first accessibility run exposed GTK's default non-focusable dropdown behavior; `d6bd2bd` explicitly
sets all labelled controls and actions focusable, and the corrected run is the accepted evidence.

Runtime-storage functional revision `c37702c76c3b1a2f9cec805cf9e219721ef7b5ce` passed
repository-foundation run `29586531915` (job `87904787120`) and Native Linux run `29586532049`
(job `87904787338`). The Ubuntu 24.04 job checked out exact Core revision
`fbf3e9b5927049dccaa19f8c36013495ffebba12`, synchronized localization, passed formatting and strict
all-feature Clippy, passed 66 library tests with the namespace test intentionally ignored in the
ordinary suite, and passed the real GTK binary test under X11/D-Bus/Xvfb. Ubuntu restricted the
unprivileged mount, so the dedicated gate used the controlled coordinator fallback and passed the
exact runtime storage test once with zero ignored tests. The same GTK binary then passed under
forced Wayland/headless Weston, and the all-target all-feature build passed. This proves the
implemented `ENOSPC` transaction boundary, not every database or storage failure.

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
display-server, AT-SPI/Orca, physical-keyboard, or GTK button-test result is claimed. In particular, the Wayland
runner was not executed against a local compositor. With the GTK binary test
present, `DOCS_RS=1 cargo test --all-targets --all-features --locked --no-run` reaches native
linking and failed on unavailable GTK symbols; it is not a valid header-free substitute.
The Provider setup and multi-profile flows passed their real widget test, native linking, and build
in the GitHub Actions evidence above, but those native checks remain unavailable on this local host.

## Remaining scope

- End-user Secret Service prompt acceptance and unlock UX. The GIO adapter, fail-closed prompted
  store/delete boundary, remembered-credential path, and secure persistent-credential onboarding
  are implemented, while session-only fallback remains available when the keyring is unavailable.
- Central release-manifest integration for this exact Linux/Core revision; broader product
  compatibility beyond the alpha.2 startup gate remains unclaimed.
- Interoperability evidence for third-party local servers, including Ollama; automated endpoint
  coverage currently uses only LinguaMesh fake providers on loopback.
- Complete canonical UI gettext coverage, plural/placeholder handling, and visual locale/RTL verification.
- Runtime database faults beyond the verified private-tmpfs `ENOSPC` transaction boundary,
  including read-only media, corruption, power loss, and broader SQLite VFS failures.
- XDG portals beyond the implemented user-data path, document-portal lease lifecycle, and direct
  FileChooser backend fixture; application-level GTK file-dialog and drag-and-drop gestures,
  physical desktop-shell notification rendering, AT-SPI/Orca and physical-keyboard accessibility coverage,
  physical-compositor/GPU Wayland coverage, broader X11/desktop coverage, Flatpak portal/notification
  delivery, and release artifacts.
- Directory-descriptor or `openat2` hardening against a concurrent same-UID path replacement during
  Linux host preflight; static components are checked before mutation and Core remains the final
  no-follow open gate.
