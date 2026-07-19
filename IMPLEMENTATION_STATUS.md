# Implementation Status

Status: Runtime storage ENOSPC rollback, forced Wayland/X11 GTK gates, baseline GTK accessibility semantics, live AT-SPI tree export checks, a headless GTK keyboard traversal fixture for tested controls, runtime catalog-backed workspace/status/theme localization, the GIO Secret Service adapter, generic completion desktop notifications, bounded native text-file import with source-editor drag-and-drop, recoverable TXT/Markdown/CSV/JSON/HTML/SRT/WebVTT/DOCX/PPTX/XLSX/EPUB/PDF document-job translation with sequential segment persistence, bounded DOCX/PPTX/XLSX/EPUB package reconstruction and resource retention, bounded optional image-only PDF OCR with page-marked text output, page-aware text-PDF reconstruction with structured HTML fallback, subtitle timestamp validation, CSV quoting and selected-column reconstruction, JSON structure/path selection and escaping preservation, HTML tag-stack validation, script/style protection, and text-node reconstruction, the corrected Secret Service session wire shape, isolated real-daemon Secret Service CRUD plus persistent restart/locked lifecycle fixtures, secure persistent-credential onboarding, fail-closed Secret Service prompted-flow handling, a remotely built pinned Flatpak bundle with bounded sandbox startup, private notification-service transport validation, headless real notification-daemon delivery, physical desktop-shell notification rendering, a real XDG document-portal lease lifecycle fixture, a real interactive portal FileChooser backend fixture, application-level GTK FileDialog callbacks, and an actual GTK source-editor drag/drop gesture fixture are implemented; Orca speech, end-user prompt acceptance, complete visible-string gettext coverage, other clients, and release artifacts remain open

Global goal SHA-256: `11f9a65927aac7e57e2af119e9d21cc98e8d5a08b8a112a19ee1c47903e36198`

Assumption: canonical generated PO/MO resources are synchronized and format-validated. The GTK host
now parses all twelve pinned official Linux MO catalogs at runtime, exposes BCP 47 locale choices,
switches the root direction for Arabic, and preserves the source editor buffer during a locale
switch; status summaries, partial-output markers, text-file import controls, provider-profile
controls, source/target language options, and stable worker/runtime/storage error sentences now use
the same catalogs; complete UI coverage, plural handling, and visual locale/RTL review remain open.

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

Assumption: Core automatic protected-span scanning covers common URLs, email addresses, Markdown
code, and placeholder forms. Linux now adds a bounded request-level glossary for product terms,
deterministic CSV import/export, and conservative semantic long-text chunking; TBX import, persistent
glossary libraries, tokenizer-derived model budgets, and provider-specific syntax remain later work.

## Implemented

- Rust 1.93.0 Cargo package at `0.1.0-alpha.2`, with locked Core alpha.2 path dependencies and
  optional `demo-provider`/`gui` features. Native CI pins Core functional revision
  `7275c5ec195946ea20a2d65e5f42790b2d631ff2`.
- Startup rejects any Core other than semantic version `0.1.0-alpha.2`, ABI 1, protocol 1, provider
  catalog `0.1.0`, with the required cancellation, compatibility, typed Rust host-secret broker,
  model-discovery, protected-span, streaming-text, and text-translation features.
- Core's `protected_spans_v1` contract shields common structured spans before provider prompt
  construction, restores them across split streamed deltas, and fails closed on missing, duplicate,
  or changed markers; Linux negotiates the feature before starting provider work.
- Core's `long_text_chunking_v1` contract splits oversized requests at paragraph, sentence, or
  whitespace boundaries without cutting opaque markers, streams chunks in source order, and keeps
  cancellation between chunks; the default 16 KiB limit is explicitly an approximate byte budget.
- Linux accepts bounded semicolon-separated `source => target` glossary rules per translation
  request and imports/exports a fixed-schema UTF-8 CSV through native GTK file dialogs. Core
  validates CSV size, row count, quoting, conflicts, and credential-shaped values, protects matching
  terms before provider prompt construction, and restores required target terms or immutable names
  across streamed fragments without writing glossary content to profiles or SQLite.
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
- Imported TXT/Markdown/CSV/JSON/HTML/SRT/WebVTT/DOCX/PPTX/XLSX/EPUB/PDF files are converted into Core `DocumentJob` snapshots before the source
  editor is populated. The existing Translate action starts a sequential worker pipeline for pending
  prose segments, forwards the request glossary and privacy policy, and writes each completed segment
  back to schema-14 storage. Document terminal snapshots reconstruct safely into the output editor;
  DOCX/PPTX/XLSX/EPUB packages retain non-text resources and rewrite supported text parts under bounded
  archive/path/XML checks; binary export uses the original extension and rejects malformed or incomplete jobs.
  PDF pages retain page association and available coordinates; safe ASCII streams are rewritten in place,
  while unsupported encodings use a page-aware HTML alternative. Image-only pages remain unchanged
  unless the user enables the bounded optional OCR plugin, which imports page-marked text without
  rewriting the source PDF.
  Stop persists cancellation, and Incognito rejects new document jobs rather than creating durable
  progress. The GTK surface still lacks a dedicated multi-job queue.
- The GTK boundary provides baseline accessibility semantics: `Main`, `Heading`, `Status`, and
  `Alert` roles; named multi-line source/output `TextBox` editors with output read-only; visible
  label-to-control `LabelledBy` and mnemonic relations; focusable editor and action controls; an
  explicit `Stop translation` accessible name; output `Busy` state during translation with terminal
  reset; and an accessibility-hidden empty error label. The `tools/run-gtk-atspi-test.sh` fixture
  additionally reads the live Xvfb application through `python3-pyatspi` and verifies the named Stop
  button plus two text-editor roles. This is AT-SPI semantic export evidence, not Orca speech,
  RTL, high-contrast, or full desktop accessibility evidence. The application-window Capture-phase
  handler provides an explicit provider-form Tab/Shift+Tab order while a CI-only focus probe records
  widget focus events without changing normal runtime behavior.
- The Linux host now uses existing GIO D-Bus bindings for Secret Service `OpenSession`, item search,
  create/update, and `GetSecret` resolution. Persistent profiles retain only a SecretRef; the
  one-shot credential is passed through the existing typed broker and is never written to SQLite.
- Fourteen canonical official/pseudo PO/MO catalog pairs containing 222 messages pinned to l10n revision
  `8fd778a5869c8b8c91610c22241883fff2e41c99`. Sync rejects a different revision, dirty generated
  source artifacts, stale copies, and unexpected catalog counts. The GTK locale selector exposes
  the twelve official packs, runtime action, workspace-widget, active-provider, status summary,
  partial-output, text-file import/export, provider-profile, source/target language, onboarding stage/detail,
  fixed provider/file/worker, reducer-state/category, translation-export, construction-stage
  provider/default-control, and request-level glossary messages, and
  System/Light/Dark theme labels switch without losing state, preserves source text while moving
  from Simplified Chinese to Arabic, and applies right-to-left root direction; arbitrary backend
  diagnostic detail remains an explicit English fallback.
- Foundation and native workflow sources use immutable Node 24-compatible action commits and
  disable persisted checkout credentials. Native CI pins reviewed Core revision
  `7275c5ec195946ea20a2d65e5f42790b2d631ff2` and localization revision
  `d64d4085fb3c1cc69c9f7965bd97ffca54ca1995`. The revised native gate retains serialized all-target,
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
- Core functional revision `7275c5ec195946ea20a2d65e5f42790b2d631ff2` is the reviewed source
  pin, and every direct Core dependency is constrained to `=0.1.0-alpha.2`.
- `cargo fmt --all --check`, the locked demo-provider check, strict Clippy, both locked test suites,
  the demo-provider build, `DOCS_RS=1` check and Clippy, `bash tools/sync-l10n.sh --check`, all 14
  PO syntax checks, and `git diff --check` passed.
- `cargo test --no-default-features --locked` passed: 53 tests, 0 failed. Coverage includes the
  request-level glossary transport without persistence and the negotiated long-text feature, in
  addition to the
  derived onboarding progression, safe stage labels, pending-model confirmation, worker-unavailable
  and storage-unavailable fallbacks, and failed-switch rollback that preserves the confirmed Ready
  identity.
- `cargo test --features demo-provider --locked` passed: 80 tests, 0 failed, with one dedicated
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
- The native text import slice accepts only UTF-8 TXT/Markdown/CSV/JSON/HTML/SRT/WebVTT content up to 4 MiB, strips a UTF-8
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
- The runtime/storage-error localization slice adds 15 Linux-only catalog messages for Core and
  loopback startup, compatibility reads, and profile-database path/permission failures. Local
  validation passed `cargo fmt --all`, all-target all-feature locked check, strict Clippy, 52
  portable tests, 5 localization tests, the targeted runtime/storage localized-error test, all 14
  PO syntax checks, and `bash tools/sync-l10n.sh --check` against l10n
  `dc9a9d48a38dfeb8f6b2020417960023678d8252`. The deterministic l10n bundle checksum is
  `a8c5535b23eb27f02ff5fd3bb4c4c1c6948718f1233321305c173b1741b27e6f`.
- The same runtime/storage-error localization revision passed the remote Linux gates: Foundation
  run `29631662275` (job `88046380379`), Native run `29631662278` (job `88046380380`), and Flatpak
  run `29631662280` (job `88046380350`). Native validation covered the real GTK X11 and forced
  Wayland paths, storage-fault, Secret Service, portal, notification, drag/drop, catalog, and
  MO/PO checks; Flatpak completed the GNOME 49 SDK build and bounded sandbox smoke.
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
- Linux locale-selector coverage adds 12 locale-name keys and refreshes the selector labels when
  the interface locale changes. Source-level validation passed all-target all-feature check,
  strict Clippy, 51 portable tests, 5 localization tests, PO/MO syntax checks, and
  `bash tools/sync-l10n.sh --check` against l10n `d21c3b0d065831b20cf31c9bf3009ffd262e4797`.
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
display-server, AT-SPI/Orca, physical-keyboard, or GTK button-test result is claimed locally. In particular, the Wayland
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
  physical desktop-shell notification rendering, AT-SPI/Orca, provider-form Tab-chain and broader physical-keyboard coverage,
  physical-compositor/GPU Wayland coverage, broader X11/desktop coverage, Flatpak portal/notification
  delivery, and release artifacts.
- Directory-descriptor or `openat2` hardening against a concurrent same-UID path replacement during
  Linux host preflight; static components are checked before mutation and Core remains the final
  no-follow open gate.

## 2026-07-18 — History inspection, export, and per-entry deletion

Assumption: the existing bounded Core history table is the authoritative Linux history source; the
inspection window loads at most 100 entries, exports only the displayed snapshot, and keeps history
enable/disable policy and translation-memory storage as separate follow-up work.

Implemented:

- Added a localized GTK View history action that opens a scrollable, selectable list of source and
  translated text with locale, model, timestamp, and a per-entry Delete action.
- Added an asynchronous UTF-8 TSV export with escaped tabs, newlines, carriage returns, and
  backslashes so untrusted translated content cannot forge rows or columns.
- Added worker list/delete commands and typed events; deletions are exact operation-ID requests and
  refresh the dialog from the Core snapshot after success.
- Pinned Core `6079138348f3182b19c017f50db768df05da62cb` and l10n
  `971d1691a4eff396c71216b898e30fcfb23e72fa`, with 240 generated localization messages.

Validated locally:

- `cargo fmt --all` passed.
- `cargo check --features gui --offline` passed through Rust compilation.
- `cargo test --features demo-provider --lib --offline` passed: 82 tests, 0 failed, 1 intentional
  ignore; `cargo test --features gui --all-targets --offline` reached native linking but failed on
  unavailable GTK 4 symbols in the host libraries, so no local GUI link result is claimed.
- `bash tools/sync-l10n.sh --check`, `git diff --check`, and l10n schema/generator tests passed.

## 2026-07-18 — History enable/disable policy

Assumption: disabling local history affects only future standard completions; existing rows remain
available to the already implemented inspection, export, and deletion controls. Incognito remains
an unconditional request-level opt-out.

Implemented:

- Added a persisted **Save translation history** GTK toggle with localized label, tooltip, and
  enabled/disabled status notes.
- Added worker startup/update/rejection events and a serialized command that writes the Core schema
  4 policy without deleting existing rows or changing session privacy mode.
- Disabled policy suppresses future standard history writes; re-enabling resumes bounded persistence.
- Added model, worker, and storage regression coverage for default enablement, persistence, policy
  changes, and preservation of existing rows.
- Pinned Core `fb00f3dd6b62a8a3a47350acc85831e60e266929` and l10n
  `40f3914e1b28fddd8f38d287fa121010f5192f1c`, with 244 generated messages and bundle checksum
  `f3e49113ed85e7e4fadeef6b872ccfe5a2e4fa67548028db5f4524479aedeeb4`.

Validated locally:

- `cargo fmt --all -- --check` passed.
- Strict all-target/all-feature Clippy passed.
- `cargo check --features gui --offline` passed through Rust compilation.
- `cargo test --features demo-provider --lib --offline` passed: 85 tests, 0 failed, 1 intentional
  ignore.
- `bash tools/sync-l10n.sh --check` and `git diff --check` passed.

## 2026-07-18 — Linux document pause/resume/retry checkpoint

Assumption: pausing a document job cancels only the active provider operation and commits no
partial segment; completed segments remain durable and the job becomes `paused`. Resume continues
pending segments with explicit provider options, while retry accepts cancelled or failed jobs.

Implemented Core schema 7 and Linux worker pause, resume, and retry commands. The GTK surface now
shows per-job completed/total progress and exposes lifecycle controls for pause, resume, and retry.
Android, Windows, and macOS remain intentionally out of scope for this Linux-first slice. At this
checkpoint, automatic provider-parameter persistence, archive codecs, and multi-job queue selection
remained open; the next checkpoint records the parameter persistence implementation.

Validated locally:

- `cargo test --features demo-provider --lib --offline` passed: 92 passed, 0 failed, 1 intentional
  environment-dependent ignore, including the pause/resume/retry worker regression.
- `cargo check --all-targets --all-features --offline` and `git diff --check` passed.
- Full GUI test linking remains CI-only because this host lacks the GTK 4.10 symbols required by
  the installed system libraries.

## 2026-07-18 — Linux document restart options checkpoint

Assumption: only non-secret document translation parameters are reusable after restart. Resume and
Retry must match the saved provider profile and model; endpoints, credentials, session secrets, and
privacy-mode state remain runtime-only.

Implemented Core schema 8 persistence for validated source/target locales, provider/model IDs, and
optional glossary rules. Linux Translate saves these options before entering the running state;
Resume and Retry load them from storage and use standard privacy after exact runtime matching. The
worker regression pauses a slow job, restarts the worker and loopback provider, reconnects, and
completes the job without supplying UI translation options again. Android, Windows, and macOS remain
intentionally out of scope; archive codecs and multi-job queue selection remain open.

Validated locally:

- `cargo test --features demo-provider --lib --offline` passed: 93 tests, 0 failed, 1 intentional
  environment-dependent ignore, including the restart-options regression.
- `cargo check --all-targets --all-features --offline` and `cargo fmt --all` passed.
- Full GUI test linking remains CI-only because this host lacks the GTK 4.10 symbols required by
  the installed system libraries.

## 2026-07-18 — Linux document-job execution

Assumption: the current Linux-first slice reuses the existing single-document editor as the first
queue surface. Imported TXT/Markdown jobs persist their source-safe segments before translation;
provider parameters are supplied again when a resumed job is started, and multi-job queue UI remains
future work.

Implemented:

- Added `TranslateDocumentJob` and per-segment events to the Linux worker. Each pending prose segment
  becomes a normal Core request carrying the selected source/target locales, request glossary, and
  standard privacy mode, then its completed text is committed before the next segment starts.
- Connected native file import to `CreateDocumentJob`, the existing Translate and Stop buttons, and
  startup restoration. Completed and cancelled snapshots reconstruct safely into the output editor;
  Incognito rejects document jobs because durable progress is required.
- Added file-import, worker cancellation, and sequential reconstruction regressions while preserving
  source-file immutability and the schema-6 storage bounds.

Validated locally:

- `cargo fmt --all` passed.
- Strict all-target/all-feature Clippy passed.
- `cargo test --features demo-provider --lib --offline` passed: 91 tests, 90 passed, 0 failed,
  1 intentional environment-dependent ignore.
- `git diff --check` passed.
- Native GUI linking remains CI-only because this host lacks the GTK 4.10 symbols required by the
  current system libraries.

## 2026-07-18 — Linux AT-SPI semantic export checkpoint

Assumption: the smallest reproducible screen-reader slice is live AT-SPI tree export for the
primary translation controls. This verifies names and roles through the Linux accessibility bus;
it does not claim Orca speech output, manual desktop review, or high-contrast/compositor coverage.

Implemented:

- Added `tools/run-gtk-atspi-test.sh`, which starts an isolated Xvfb/xfwm4 session and the AT-SPI
  bus, then launches the real GTK binary without source text or credentials in diagnostics.
- Added `tools/gtk-atspi-inspect.py`, which waits for the application to register and verifies the
  exported `Stop translation` push button plus two text-editor roles through `python3-pyatspi`.
- Added the pinned `python3-pyatspi` CI dependency and Foundation required-file checks; existing
  GTK helper assertions continue to cover label relations, editor properties, and state changes.

Validated:

- Native Linux run `29664268158` (job `88131961550`) passed the AT-SPI fixture, all existing GTK,
  portal, Secret Service, storage, X11, and Wayland gates.
- Repository Foundation run `29664268165` and Flatpak run `29664268145` passed for Linux head
  `c0fddf471377444b9a565ec762bec949a8d3055d`.
- Local `git diff --check`, Bash syntax, and Python AST checks passed; this host cannot run the
  GUI fixture because Xvfb/GTK 4.10 symbols are unavailable locally.

Orca speech, provider-form default Tab-chain review, physical desktop accessibility, OCR, other
clients, and stable-release evidence remain open.

## 2026-07-18 — Linux subtitle document checkpoint

Assumption: SRT and WebVTT preserve cue IDs, headers, timestamps, ordering, and original line
endings verbatim. Only cue text is translatable; timing and subtitle line-length policy are not
rewritten automatically.

Implemented Core `linguamesh-document` support for bounded UTF-8 `.srt` and `.vtt` jobs, including
timestamp/cue-order validation, structural segmentation, and reconstruction validation. Linux's
native chooser now accepts both suffixes and maps malformed subtitle structure to a safe import
error. TXT/Markdown behavior and schema-8 restart option reuse remain unchanged; HTML/JSON/CSV and
archive formats, multi-job queue presentation, Android, Windows, and macOS remain open.

Validated locally:

- Core workspace fmt, all-target/all-feature check, strict Clippy, offline workspace tests, and
  diff checks passed; the document crate has 7 passing tests.
- Linux fmt, all-target/all-feature check, strict Clippy, offline library tests (94 passed, 1
  intentional environment-dependent ignore), and diff checks passed.

## 2026-07-18 — Linux DOCX package checkpoint

Assumption: DOCX support is intentionally bounded to OOXML ZIP packages of at most 4 MiB and 512
entries. Only document, header/footer, footnote, endnote, comment, and glossary XML text nodes are
translated; package resources are retained. Encrypted, traversal, duplicate, malformed, DTD-bearing,
oversized, and incomplete packages are rejected, and no source path or credential is persisted.

Implemented Core `0f71a652a536753f48bb8c852fd38e97740c23ce` DOCX parsing, XML-safe text-node
reconstruction, binary export, and schema-10 package BLOB persistence. Linux's chooser accepts DOCX,
the worker reconstructs completed jobs through Core, and the GTK save path writes the binary package
without allowing source overwrite.

Validated locally:

- Core `cargo test --workspace --all-features --locked`: all workspace tests passed, including 19
  document and 25 storage tests; strict check, Clippy, format, and diff checks passed.
- Linux `cargo test --lib`: 61 tests passed; strict all-target/all-feature check, Clippy, format,
  and diff checks passed. Full GTK test linking remains blocked by missing GTK symbols in the local
  system libraries; the native CI gate remains authoritative for the GUI binary.

## 2026-07-18 — Linux PPTX package checkpoint

Assumption: PPTX support reuses the bounded OOXML ZIP/XML contract: packages are at most 4 MiB and
512 entries, only slide, notes, master, layout, handout, and comment text parts are translated, and
all relationships and media resources remain unchanged. Encrypted, traversal, duplicate, malformed,
DTD-bearing, oversized, and incomplete packages are rejected.

Implemented Core `0f71a652a536753f48bb8c852fd38e97740c23ce` PPTX text-node inspection/reconstruction
and schema-11 format migration. Linux's chooser accepts PPTX and reuses the worker's binary export;
the persisted package bytes never contain source paths or credentials.

Validated locally:

- Core document tests: 20 passed; storage tests: 26 passed, including schema-11 migration and PPTX
  package reopen/reconstruction.
- Linux fmt, all-target/all-feature check, strict Clippy, 61-test library suite, and diff checks
  passed. Full GTK binary linking remains a CI-only boundary on this host.

## 2026-07-18 — Linux XLSX package checkpoint

Assumption: XLSX support reuses the bounded OOXML ZIP/XML contract: packages are at most 4 MiB and
512 entries, only shared-string and worksheet text nodes are translated, and workbook relationships,
styles, formulas, numbers, and media resources remain unchanged. Encrypted, traversal, duplicate,
malformed, DTD-bearing, oversized, and incomplete packages are rejected.

Implemented Core `c21a7df193d73568875315c94153f458d7f905ce` XLSX shared-string/worksheet inspection
and reconstruction with schema-12 format migration. Linux's chooser accepts XLSX and reuses the
worker's binary export; the persisted package bytes never contain source paths or credentials.

Validated locally:

- Core document tests: 21 passed; storage tests: 27 passed, including schema-12 migration and XLSX
  package reopen/reconstruction.
- Linux fmt, all-target/all-feature check, strict Clippy, 61-test library suite, and diff checks
  passed. Full GTK binary linking remains a CI-only boundary on this host.

## 2026-07-18 — Linux EPUB package checkpoint

Assumption: EPUB support is bounded to 4 MiB and 512 ZIP entries, requires the first uncompressed
`mimetype` entry plus `META-INF/container.xml`, an OPF package document, and at least one XHTML/HTML
content document. Container and OPF XML reject DTDs and malformed structure; traversal, duplicate,
encrypted, symlink, oversized, and invalid-UTF-8 entries are rejected. Visible XHTML/HTML text is
segmented while tags, scripts, styles, navigation, CSS, and binary resources remain verbatim.

Implemented Core `554c09521b57de45be154a99edfbf24aa2fc6538` EPUB inspection/reconstruction and
schema-13 format migration. Export updates an existing OPF `dc:language` value from the persisted
target locale. Linux's chooser accepts EPUB and the worker reuses the binary export path; package
resources remain unchanged and source paths or credentials are never persisted.

Validated locally:

- Core document tests: 22 passed; storage tests: 28 passed, including schema-13 migration and EPUB
  package reopen/reconstruction.
- Linux format, all-target/all-feature check, worker export integration, and file-import regression
  checks passed locally; remote native, foundation, and Flatpak gates remain required.

## 2026-07-18 — Linux text-PDF checkpoint

Assumption: PDF support is intentionally limited to bounded text-based files. The parser keeps
page association, extracts basic text coordinates and reading-order boundaries, rejects encrypted or
unsupported filtered streams, and distinguishes empty/image-only pages by their lack of text
segments. It does not perform OCR or promise pixel-identical layout reconstruction.

Implemented Core PDF inspection, page-aware segment persistence in schema 14, safe literal/hex text
stream rewriting with optional Flate compression, and a structured HTML alternative that preserves
page dimensions and translated text when PDF encoding cannot safely represent the target text.
Linux's chooser accepts `application/pdf`; export falls back from `.pdf` to `.html` for that typed
encoding limitation while preserving the original PDF source in the stored job.

Validated locally:

- Core workspace tests passed: 23 document tests and 29 storage tests, including PDF reopen and
  reconstruction; strict Clippy passed.
- Linux format, all-target/all-feature check, strict Clippy, 61 no-default tests, and 98
  demo-provider tests passed (one namespace test intentionally ignored).
- Remote Linux native, foundation, and Flatpak gates remain required for this checkpoint.

## 2026-07-18 — Linux CSV document checkpoint

Assumption: CSV import is bounded to the existing 4 MiB document limit, 10,000 records, 1,024
fields per record, and 10,000 persisted segments. The codec detects comma, semicolon, tab, or pipe
delimiters from the first record, preserves quoted fields, escaped quotes, variable-width rows,
record line endings, and the original source shape. Linux translates eligible text fields by default,
skips common identifier and numeric columns, and Core's selected-column constructor lets a host
override those heuristics explicitly.

Implemented Core CSV format detection, structural validation, decoded provider text, encoded
translations, selected-column segmentation, and schema-9 storage migration. Linux's native source
chooser accepts `.csv`, maps malformed CSV to the generic document-structure error, and decodes CSV
fields before they reach the provider while persistence re-encodes translated fields safely.

Validated locally:

- Core document tests: 11 passed; storage tests: 22 passed, including schema-9 migration and CSV
  reopen/reconstruction.
- Core workspace tests and strict all-target/all-feature Clippy passed. Linux `cargo test --lib`: 59
  passed; `cargo fmt --all -- --check` passed after formatting.
- Full workspace and native CI gates remain required before this checkpoint is considered remotely
  released; HTML, JSON, archive formats, multi-job queue presentation, Android, Windows, and macOS
  remain open.

## 2026-07-18 — Linux TXT/Markdown document contract

Assumption: Linux keeps the existing bounded native file chooser and editor UX, while Core owns
format detection, UTF-8/BOM validation, line-ending preservation, Markdown fenced-code protection,
and reconstruction semantics. This slice does not claim a persistent document queue or archive
format support.

Implemented:

- Pinned Core `6c54f329e9a62ffa1d2f9503087e59d4b9e9d6e9`, which exposes the negotiated
  `bounded_text_document_v1` feature and the `linguamesh-document` crate.
- Routed Linux TXT/Markdown file import through the Core document contract. Unsupported formats,
  oversized data, and invalid UTF-8 are rejected without exposing paths or file contents; existing
  localized file error keys are used for user-facing failures.
- Added Linux regression coverage for Markdown selection, BOM/line endings, unsupported formats,
  UTF-8 failures, and the existing 4 MiB boundary.

Validated locally:

- `cargo fmt --all` passed.
- `cargo test --lib --offline` passed: 56 tests, 0 failed.
- `git diff --check` passed.
- Native linking remains CI-only because this host lacks the GTK 4.10 symbols required by the
  current system libraries.

## 2026-07-18 — Linux document queue localization follow-up

Assumption: the Linux-first scope keeps all other clients deferred. Document queue actions,
dialogues, empty/paused/progress states, and queue tooltips must resolve through the canonical
l10n catalog; non-English packs remain explicitly unreviewed drafts with English source fallback.

Implemented:

- Pinned l10n `0ef4fb9b6878655e46e2b8ca5bbed9562f97b0f0`, a 277-message bundle with generated
  PO/MO resources for all twelve official locale packs and the document queue control keys.
- Added catalog coverage assertions for queue actions, dialog, empty/paused/progress statuses,
  and tooltip keys, while retaining the existing PDF and subtitle warning checks.
- Updated the Linux workflow, synchronization guard, README, and release notes to the immutable
  l10n revision and bundle checksum `e26da1a391369ed84c0f57f5fd5d440f50ed56dcbc8f069abd4d6d27db7dd9c1`.

Validated locally:

- `bash tools/sync-l10n.sh --write` and `--check` passed.
- `cargo fmt --all -- --check`, all-target/all-feature `cargo check`, strict Clippy, and
  `cargo test --no-default-features --locked` passed (61 tests).
- `cargo test --features demo-provider --locked` passed (98 tests, 1 existing
  environment-dependent ignore).
- Native Linux `29656549651`, Foundation `29656549644`, and Flatpak `29656549677` passed.

The checkpoint remains unreleased; OCR, remaining archive formats, complete acceptance scenarios,
non-Linux clients, and stable-release evidence remain open.

## 2026-07-18 — Linux keyboard traversal fixture checkpoint

Assumption: the first automated keyboard slice should exercise the real GTK window under Xvfb with
an actual lightweight window manager, while documenting controls that remain outside the default Tab
chain rather than masking the gap.

- Linux `ee23ca4` adds a runtime-only focus probe and `tools/run-gtk-keyboard-focus-test.sh`.
  The fixture starts `xfwm4`, injects Tab and Shift+Tab events, and asserts traversal for the tested
  onboarding/workspace controls. Provider fields are also asserted visible, enabled, mapped, and
  focusable; their omission from the default Tab chain remains an explicit follow-up.
- Native Linux run `29661016843` (job `88123562296`) and Repository Foundation run `29661016844`
  passed. Flatpak run `29661016848` is the matching packaging gate for this head.
- Local `cargo fmt`, all-target/all-feature `cargo check`, `bash -n tools/run-gtk-keyboard-focus-test.sh`,
  and `git diff --check` passed. The GUI fixture itself requires the CI GTK/portal environment.

The checkpoint remains unreleased; provider-form Tab-chain repair, screen-reader narration, and
physical desktop review remain open.

## 2026-07-18 — Linux document queue keyboard reachability regression

Assumption: queue actions must remain keyboard reachable while the Linux client continues to defer
other native clients. The existing GTK accessibility gate and the new headless traversal fixture are
the automated checks; provider-form Tab-chain coverage and screen-reader narration still require
manual desktop review.

- Extended the real GTK lifecycle test to assert focusability for Document jobs, Pause document,
  Resume document, and Retry document alongside the existing primary actions.
- Local `cargo fmt`, all-target/all-feature check, strict Clippy, no-default 61-test suite, and
  demo-provider 99-test suite (98 passed, 1 existing environment-dependent ignore) passed.
- The change is test-only and does not alter persisted data, provider routing, or document output.

## 2026-07-18 — Linux document job recovery

Assumption: the first recoverable queue slice persists only an opaque job ID, source basename,
format, ordered bounded segments, and lifecycle state. It does not persist source paths, provider
credentials, session secrets, or archive payloads. GUI queue presentation and archive codecs remain
future work.

Implemented:

- Pinned Core `6c54f329e9a62ffa1d2f9503087e59d4b9e9d6e9`, whose schema 6 adds bounded document-job
  and segment snapshots plus resumable-state APIs.
- Added Linux worker commands/events for document-job create, list, segment update, resume, cancel,
  startup restoration, and exact storage rejection. Segment progress survives worker restart and
  reconstruction still preserves source line endings and Markdown structure segments.
- Kept persistence limited to the safe basename/segment snapshot contract; no filesystem paths or
  credential values are written.

Validated locally:

- `cargo fmt --all -- --check` and strict all-target/all-feature Clippy passed.
- `cargo check --features gui --offline` passed through Rust compilation.
- `cargo test --features demo-provider --lib --offline` passed: 89 tests, 88 passed, 0 failed,
  1 intentional environment-dependent ignore.
- `bash tools/sync-l10n.sh --check` and `git diff --check` passed.

## 2026-07-18 — Linux translation memory controls

Assumption: translation memory is a separate optional local cache from history. Incognito never
reads or writes it; disabling the policy keeps existing entries; and provider/model identity is
included so same-named models from different confirmed providers cannot cross-reuse results.

Implemented:

- Pinned Core `6c54f329e9a62ffa1d2f9503087e59d4b9e9d6e9`, whose schema 6 storage exposes bounded document jobs alongside the existing
  schema-5 translation-memory policy, deterministic identity, lookup/write, inspection, export data, exact
  translation-memory policy, deterministic identity, lookup/write, inspection, export data, exact
  deletion, and clear-all controls.
- Added Linux worker startup/policy/list/delete/clear events and cache-hit translation flow. A hit
  emits the normal ordered translation lifecycle without contacting the provider and still obeys
  the separate history policy.
- Added GTK controls for Save/View/Export/Delete/Clear translation memory, localized across all 12
  official packs. The pinned l10n revision is
  `d64d4085fb3c1cc69c9f7965bd97ffca54ca1995` (262 messages; bundle checksum
  `a3de4b0bf4afd710a01d15e0426f0d163b56910c0b04f26c411870eae9eea368`).
- Added model, worker, and storage regressions for policy persistence, cache reuse, provider
  isolation, identity mismatches, Incognito, exact deletion, and clear-all.

Validated locally:

- `cargo fmt --all -- --check` passed.
- Strict all-target/all-feature Clippy passed.
- `cargo check --features gui --offline` passed through Rust compilation.
- `cargo test --features demo-provider --lib --offline` passed: 87 tests, 0 failed, 1 intentional
  ignore.
- `bash tools/sync-l10n.sh --check` and l10n schema/generator tests passed.
- Native GUI linking remains CI-only because this host lacks the GTK 4.10 symbols required by the
  current system libraries.

## 2026-07-18 — Linux dialog field localization checkpoint

Assumption: the existing catalog keys `field.source_text` and `field.translation` are the canonical
labels for source and translated content in the history and translation-memory dialogs. No new
message keys or locale-pack changes are needed for this bounded UI-copy cleanup.

- History and translation-memory entries now build their visible `Source text:` and `Translation:`
  prefixes from the active runtime catalog instead of hard-coded English strings; stored content
  remains unchanged and no diagnostics include the displayed text.
- Local `cargo fmt --all -- --check`, strict all-target/all-feature Clippy, the locked no-default
  61-test suite, and `git diff --check` passed.
- Native `29664748564` (job `88133181160`), Foundation `29664748549`, and Flatpak `29664748553`
  passed for Linux revision `3422a004c1330d318543917793d96f1b23105ed9`.

Other dynamic dialog metadata (`Job`, `Identity`) and complete visible-string gettext coverage
remain open alongside Orca speech, physical desktop review, OCR, other clients, and stable-release
evidence.

## 2026-07-18 — Linux dialog metadata localization checkpoint

Assumption: the existing catalog titles `dialog.document_jobs` and `dialog.memory` are the
canonical Linux labels for the corresponding job and translation-memory metadata rows. Reusing
those keys keeps this slice limited to runtime UI copy and avoids inventing untranslated catalog
entries while the broader visible-string audit remains open.

- Document-job rows now render their identifier prefix through the active catalog.
- Translation-memory rows now render their identity prefix through the active catalog; stored
  identifiers and translated content remain unchanged.
- Local `cargo fmt --all -- --check`, strict all-target/all-feature Clippy, the locked no-default
  61-test suite, and `git diff --check` passed.

Native, Foundation, and Flatpak CI gates remain required for the pushed revision. Complete
visible-string gettext coverage, Orca speech, physical desktop review, OCR, other clients, and
stable-release evidence remain open.

## 2026-07-19 — Linux glossary validation localization checkpoint

Assumption: request-level glossary syntax, credential-like data rejection, and conflicting-rule
errors are stable user-facing Linux messages and require dedicated catalog keys instead of the
generic English diagnostic fallback. The pinned l10n revision is
`ede66149c501a1680ed050d76b8b78e7b565ba01` (289 canonical messages; bundle checksum
`c8bd6b0464ebbfa015988a4fc0cfd30b1f9e28d9e1aad19b8c50d36976128e8f`).

- Added catalog-backed mappings for the three glossary validation errors and synchronized all 14
  Linux PO/MO resources, including pseudo-locales.
- Added a regression covering localized rendering of all three messages.
- Local targeted localization test, strict all-target/all-feature Clippy, locked no-default 61-test
  suite, l10n synchronization check, and `git diff --check` passed.

Native, Foundation, and Flatpak CI gates remain required for the pushed revision. Complete
visible-string gettext coverage, Orca speech, physical desktop review, OCR, other clients, and
stable-release evidence remain open.

## 2026-07-18 — Linux multi-job queue controls checkpoint

Assumption: the existing persisted `DocumentJobSnapshot` list is the source of truth for a
multi-job GTK queue; queue-row controls must reuse the worker's existing pause, resume, and retry
commands and must not introduce a second task state machine.

- Added non-blocking `WorkerCommandHandle` methods for pausing, resuming, and retrying a selected
  document job.
- Extended each persisted-job row with a catalog-backed action appropriate to its state while
  retaining Select as the source-editor binding action. The row action first selects the job,
  then submits the existing worker command; storage schema, segment ordering, and cancellation
  semantics are unchanged.
- Local `cargo fmt --all`, strict all-target/all-feature Clippy, the locked no-default 61-test
  suite, and `git diff --check` passed.

Native, Foundation, and Flatpak CI gates remain required for the pushed revision. Orca speech,
physical desktop review, OCR, other platform clients, complete visible-string gettext coverage,
and stable-release evidence remain open.

## 2026-07-19 — Linux provider-form Tab-chain evidence checkpoint

Assumption: provider onboarding controls require a deterministic application-window Tab/Shift+Tab
order, while Ctrl/Alt/Super-modified Tab remains native workspace navigation. The existing
Capture-phase handler owns that provider order and skips controls that are hidden, insensitive, or
not focusable.

- The Xvfb/xfwm4 keyboard fixture asserts provider name, endpoint, credential, Remember profile,
  Connect, and the tested workspace controls after real Tab/Shift+Tab input.
- The current Linux revision `cb22b2052362ce7b4990cc4be99e26a152b07800` passed Native
  `29666379600`, Foundation `29666379579`, and Flatpak `29666379586`.
- The local fixture could not link on this host because the installed GTK libraries do not expose
  the GTK 4 symbols required by the current build; the remote Native gate supplies executable
  Xvfb/xfwm4 evidence.

Orca speech, physical desktop review, OCR, complete visible-string gettext coverage, other clients,
and stable-release evidence remain open.

## 2026-07-19 — Linux document-job metadata localization checkpoint

Assumption: persisted document-job rows must not expose Rust enum debug names as user-facing copy;
the source filename and technical format names remain data, while lifecycle state labels and the
row summary use the canonical Linux catalog. l10n revision
`c81728faf8679e7a5e9854537ad7c70c046c7800` adds seven Linux-only messages, producing 296 canonical
messages and bundle checksum `d2f4fd439b5fbc8fc6d48f1be0a91ee92f558c70b851271d643829cfe8590e9b`.

- Replaced Rust `Debug` output in persisted document-job rows with stable format labels and
  catalog-backed lifecycle state labels and row metadata.
- Added localization coverage assertions for the row template and all six lifecycle states.
- `bash tools/sync-l10n.sh --write` and `--check` passed; local `cargo fmt --all`, strict
  all-target/all-feature Clippy, locked no-default 61-test suite, demo-provider 99-test suite
  (one existing environment-dependent ignore), and `git diff --check` passed.
- The first pushed head `c93d416` correctly failed Native localization validation
  (`29667345614`) because the workflow still pinned the prior l10n revision; workflow pin
  `fd30017` corrected it. Current Native `29667394462`, Foundation `29667394454`, and Flatpak
  `29667394442` passed, including the GTK keyboard, AT-SPI, Wayland, and Flatpak smoke gates.

Complete visible-string gettext coverage, Orca speech, physical desktop review, OCR, other clients,
and stable-release evidence remain open.

## 2026-07-19 — Linux optional image-only PDF OCR checkpoint

Assumption: OCR is an explicitly enabled Linux capability. The external `pdftoppm` and `tesseract`
processes receive bounded input and output, run without a shell in a private `0700` temporary
directory, and are time-limited. OCR creates a page-marked TXT document job and never rewrites the
source PDF or claims pixel-identical reconstruction.

- Added a Tesseract plugin boundary with fixed localized unavailable, malformed-document, page,
  timeout, output, no-text, and process-failure errors. The GTK source-file action exposes an OCR
  toggle only for image-only PDF pages and keeps the original source URI unchanged.
- Added page-marked OCR job conversion, worker persistence, localized progress/error rendering, and
  a private generated ImageMagick PDF fixture runner for the external plugin.
- Synchronized Linux PO/MO resources to l10n `cacc1577bc1a19a94c11faeffa7a63016d54d64e`.
- Local `cargo fmt --all --check`, all-target/all-feature `cargo check`, strict Clippy, locked
  no-default tests (64 passed, 1 ignored), demo-provider tests (102 passed, 2 ignored), OCR
  fixture, l10n sync check, shell syntax, and `git diff --check` passed.
- Native Linux run `29668533941` (job `88143262465`), Repository Foundation run `29668533939`,
  and Flatpak Linux run `29668533922` (job `88143262421`) passed. Native exercised the new OCR
  fixture after installing ImageMagick, Poppler, and Tesseract; Flatpak continued to pass its
  sandbox smoke without enabling OCR by default.
