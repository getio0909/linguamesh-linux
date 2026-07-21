# Testing and Validation

The Linux worker tests cover both local-model wire contracts: the `local-loopback` preset uses
OpenAI-compatible `/v1/` discovery and SSE, while the `ollama` preset uses native `/api/tags`
discovery and `/api/chat` NDJSON streaming. The native fixture asserts deliberate model selection,
fragmented UTF-8 handling, cancellation, and the translated `你好，Ollama！` output without a
credential. These tests do not replace interoperability testing against a running third-party
Ollama daemon.

The GTK provider regression also selects the Anthropic Messages preset, verifies its HTTPS `/v1/`
default, and requires a manual Model ID before Connect. The model is validated locally before any
worker connection or host SecretRef resolution; saved profiles restore the non-secret model ID.

The same provider fixture covers the Google Gemini preset through the `/v1beta/` Generate Content
contract: `models` discovery filters entries that support `generateContent`, and the streaming
path consumes fragmented SSE candidates until a `finishReason` terminal event. Credentials use the
`x-goog-api-key` header and never appear in diagnostics. This deterministic loopback fixture does
not claim live external Gemini-account or quota coverage.

The Azure OpenAI fixture uses a resource URL, the fixed API version `2024-10-21`, and an
`api-key` session header. It supplies `fake-deployment` manually, verifies that no deployment-list
request is made, and streams the deterministic `你好，Azure！` response through the real worker path.
This proves request shaping and secret isolation only; live Azure account, quota, and deployment
availability remain unverified.

The OpenAI Responses fixture uses the shared `/v1/models` discovery route and `/v1/responses` with
the session Bearer credential. The worker verifies one model-list request, one typed-SSE translation
request, ignores `response.created`, streams `response.output_text.delta`, and completes on
`response.completed` with deterministic `你好，Responses！` output.

The GTK regression `provider_presets_map_to_stable_native_and_compatible_defaults` validates the
six-position Linux preset order against the bundled Core provider catalog. Adapter types must match
the catalog, and manual-model visibility is derived from its `model_listing` field. The application
performs the same compatibility check before creating the window and fails closed with an English
diagnostic if the pinned catalog drifts.

The Linux worker regression `gemini_provider_discovers_and_streams_without_secret` now exercises
that fixture through the real `ProviderManager` and worker path: it discovers
`gemini-2.0-flash`, deliberately selects it, and completes `你好，Gemini！` without a credential.

The real-daemon regression is opt-in and never downloads a model by default. With a running
third-party daemon and an installed model, execute:

```sh
LINGUAMESH_OLLAMA_MODEL=smollm:135m bash tools/run-ollama-interop-test.sh
```

Set `LINGUAMESH_OLLAMA_PULL=1` only in an isolated environment when the named model should be
pulled through Ollama's `/api/pull`; the script then exercises real `/api/tags` discovery and
`/api/chat` translation through the Linux worker. The default test suite keeps this regression
ignored because the daemon and model are external prerequisites.

The Linux checkpoint has a reproducible external pass using Docker image
`ollama/ollama:0.11.10`, model `qwen2.5-0.5b-instruct:latest`, and the Qwen GGUF SHA-256
`9ee36184e616dfc76df4f5dd66f908dbde6979524ae36e6cefb67f532f798cb8`. The harness reported
`1 passed; 0 failed` through native `/api/tags` and `/api/chat`; the temporary daemon and model
were removed after validation. This evidence is prerelease-only and does not cover GPU execution.

The Linux checkout consumes the canonical gettext bundle from immutable l10n revision
`737d890e60fd34f15fd8708698448ef9ab96299f`. The bundle contains 426 messages, and
`bash tools/sync-l10n.sh --check` verifies every PO/MO catalog and the generated manifest before
the native build. History/memory row metadata, document-job IDs, active-provider mode summaries,
unavailable provider/model labels, and routing-profile actions/mode labels are asserted through
catalog keys rather than concatenated English UI fragments; non-English packs remain
machine-generated drafts pending human review. The editor metrics regression checks character
counts and an explicitly approximate token estimate without exposing text in diagnostics. Routing
profile tests also verify preference-index round trips and preservation of hidden Core constraints
when visible privacy/capability controls are edited. Constraint parser tests cover comma-separated
provider/model lists, positive numeric limits, and rejection of unsafe or empty values.

Every routing decision carries a redacted explanation into the diagnostics panel: the eligible
candidate keys, rejected candidate keys with stable reason codes, ranking inputs, and configured
fallback order. Model, worker, and GTK lifecycle tests assert these details while ensuring that
endpoints, credentials, and translated content never enter the event or visible diagnostics.

Quality-mode UI behavior maps the localized Fast/Balanced/Best dropdown to the Core
`TranslationQualityMode` values and keeps the selector enabled for a selected document job. The
worker restart regression selects `Best`, persists it through a routed dispatch, and verifies the
resumed snapshot retains `Best`. Core
tests cover the versioned `translation-prompt-v2` directives and deterministic rejection of empty
or Unicode-replacement output before `Completed`; no hidden extra provider request is introduced.

Retry policy is covered at both contract boundaries. Core provider adapters parse numeric or HTTP-date
`Retry-After` headers into an optional, backward-compatible error field capped at sixty seconds.
Linux consumes that hint with an eight-second maximum wait, otherwise uses bounded exponential backoff
with stable candidate-key jitter, cancels the wait on shutdown, and opens an in-memory candidate
circuit after two retryable failures for a thirty-second cooldown. The worker tests
`routing_backoff_prefers_retry_hint_and_stays_bounded` and
`routing_circuit_breaker_opens_after_repeated_failures_and_resets` cover the policy without logging
endpoints, credentials, request text, or provider output.

Translation-preset UI behavior maps the localized General/Technical/Marketing dropdown to the Core
`TranslationPreset` values and carries the selection into ordinary requests. Linux tests cover all
stable IDs and compatibility negotiation rejects a Core that does not advertise
`translation_presets_v1`; document jobs persist and restore the selected preset through schema 18
after pause, retry, and worker restart.

The source-level localization checks are reproducible without GTK or third-party packages:

```sh
python3 -B tools/check-localization-keys.py
python3 -B tools/check-localization-placeholders.py
python3 -B tools/check-visible-localization.py
```

The first command checks catalog key coverage for localization calls, diagnostics, and error-message
mappings; the second checks canonical English fallback text, placeholder identity, and malformed braces in literal templates; the third rejects non-empty literal
strings passed directly to GTK visible-control APIs and direct string-list options. Empty strings
used to clear a transient label are intentionally permitted.

The routing-profile worker regression saves, lists, and deletes a Core `routing_planner_v1` profile
without persisting provider endpoints, credentials, or translation content. A separate regression
selects a saved candidate, reconnects it through the host secret broker, and completes an ordinary
text request while asserting the typed decision event. The ordered-chain regression stops the first
saved provider before dispatch, verifies the next eligible candidate is selected, and asserts the
typed routing-fallback event and translated output. The worker also retries a retryable stream
failure across remaining automatic or ordered candidates, preserving event ordering and partial
output. The automatic-chain regression additionally sets a quality preference, verifies the higher-
quality candidate is selected, then shuts it down before dispatch and proves the worker uses only
the next approved candidate. A document-job regression selects a saved document-capable routing candidate while a
different provider is active, translates every pending segment through that candidate, and asserts
that the document decision reports no fallback even when the profile permits explicit fallback.
The production fallback confirmation window is covered by the dedicated GTK test
`gtk_fallback_approval_dialog_requires_an_explicit_one_shot_action`. It verifies the modal warning
copy and focusable actions, that `Close` dismisses without dispatch or approval, and that one
`Translate` click records one-shot approval and exactly one translation dispatch. The test is
marked ignored in the parallel Rust suite and is run explicitly under `dbus-run-session` and
`xvfb-run` in the Native workflow so GTK initialization remains on one thread.
The dedicated GTK test `gtk_routing_profile_candidate_controls_have_accessible_lifecycle` covers
the production candidate editor with two saved provider/model pairs. It checks the labelled profile
ID field, stable mode choices, explicit fallback checkbox, focusable candidate rows, accessible
up/down button labels, row reordering, Manual-mode single-candidate enforcement, and the Use
close-and-select lifecycle. It uses the same serialized DBus/Xvfb fixture boundary as the fallback
dialog test and remains prerelease evidence until visual, translated-copy, and end-user Orca review.
The GTK dialog creates a bounded profile from saved provider/model selections and now exposes the
Core `Manual`, `Ordered`, and `Automatic` modes in a stable order. Its separate explicit fallback
checkbox is off by default; when a routing profile is selected, it takes precedence over the
ordinary text fallback path, while document jobs never auto-fallback. Candidate checkboxes and
adjacent up/down controls and row drag-and-drop allow a profile to include and order a subset of
saved provider/model pairs. Manual mode persists only the first selected pair and deactivates extra
selections when loaded or selected; Ordered and Automatic preserve the candidate chain. The icon controls expose localized accessible labels, while empty
selections and invalid drag IDs are rejected before persistence. Each saved profile row also has an
**Edit** action that restores the persisted mode, fallback consent, candidate selection/order, and
ID; saving updates the same profile record rather than creating a duplicate.
New profiles validate IDs against Core's 1–128 byte ASCII identifier rule; edit mode locks the
existing ID to protect saved references, and a new profile cannot reuse an existing ID.
Core exchange tests round-trip a bounded profile, reject malformed/oversized JSON and unknown
fields, and assert that no endpoint or credential-shaped field can be exported. Worker tests cover
UTF-8 import, duplicate-ID rejection, persistence errors, and export of the validated profile.
Native and Flatpak CI remain authoritative for the GTK file chooser callbacks.
The restart regression `document_job_resume_reconnects_saved_routing_profile_after_restart`
interrupts a routed job, reopens the database, reconnects the saved profile through the host secret
broker, and completes the remaining segments while asserting a zero-fallback decision.

## Host prerequisites

Rust 1.93.0 is pinned by `rust-toolchain.toml`. A sibling `../linguamesh-core` checkout is required
because the client deliberately uses typed path dependencies instead of copying shared behavior.
Its functional source must match approved revision
`19229184a21a6725326a3d30dea9bc72e5ac999f`. This revision carries bounded document lease
consumption smoke, POSIX-descriptor document consumption, and the AddressSanitizer gate, plus the
protocol decoder fuzz gate and bounded FileLease lifecycle,
including Linux's portal-read lease checks, and the explicit request-level
Incognito privacy policy and changes file-backed Core storage to add SQLite's `SQLITE_OPEN_NOFOLLOW`
flag, adds protected-span restoration and request-level glossary
protection for streamed text, and adds bounded semantic chunking. On
Linux's default Unix VFS, any symbolic-link path component is rejected. A clean documentation-only
descendant is acceptable
for local path builds when the compiled source tree is unchanged; validate it with:

```sh
git -C ../linguamesh-core cat-file -e 19229184a21a6725326a3d30dea9bc72e5ac999f^{commit}
git -C ../linguamesh-core diff --quiet \
  19229184a21a6725326a3d30dea9bc72e5ac999f..HEAD -- \
  Cargo.toml Cargo.lock rust-toolchain.toml rustfmt.toml crates assets migrations
test -z "$(git -C ../linguamesh-core status --porcelain)"
```

The same Core pin also negotiates `bounded_text_document_v1`, `routing_planner_v1`,
`translation_quality_modes_v1`, and `translation_presets_v1`: Linux imports only bounded UTF-8 TXT,
Markdown, CSV, JSON, HTML, SRT, WebVTT, DOCX, PPTX, XLSX, EPUB packages, and text-based PDF pages, preserves line endings, keeps Markdown fenced code and subtitle timing
structure verbatim, and
persists pending/running/paused document jobs and validated non-secret translation options, including
the selected translation preset and
the optional routing-profile ID, for worker
restart recovery. The Linux worker tests also cover
sequential prose-segment translation, per-segment persistence, safe reconstruction (including DOCX/PPTX/XLSX/EPUB package resources and PDF page association), structured HTML fallback for unsupported PDF encodings, and cancellation
to a persisted cancelled snapshot. The GTK surface now exposes per-job progress and
pause/resume/retry controls, and the worker regression
`document_job_list_returns_multiple_saved_jobs_for_queue_selection` verifies that two pending jobs
are listed together for explicit selection. `cancelled_document_job_can_be_retried_without_losing_pending_segments`
verifies that a cancelled job retains both pending segments and can be retried to completion with
the saved provider/model options. The worker regressions
`imports_pptx_and_preserves_notes_and_resources`,
`document_job_translation_reconstructs_docx_and_preserves_binary_parts` and
`document_job_translation_reconstructs_xlsx_and_preserves_formulas_and_numbers`,
`document_job_translation_reconstructs_pptx_and_preserves_notes_and_resources` drive the
persisted-job translation path end to end, then inspect reconstructed OOXML while checking that
binary resources, formulas, and numeric cells survive. The worker concurrency gate allows four
document jobs and isolates their event streams and cancellation state by job ID.
`concurrent_document_jobs_run_independently` proves that two slow jobs can stream and complete
together, while `cancelling_one_concurrent_document_job_keeps_the_other_running` proves that
targeted cancellation does not interrupt its survivor. The fifth-job limit and duplicate-start
guard reject before any new Running snapshot is persisted. PDF imports
also expose bounded structured warnings for image-only pages, uncertain reading order, and limited
reconstruction; the UI warning test verifies that only page numbers and fixed text are shown, never
source content. Subtitle imports also expose configurable Core thresholds for line length and
reading speed; the default UI warning test verifies cue-number-only output.

The reviewed Core pin also rejects an OOXML ZIP entry whose uncompressed size is more than 200 times
its compressed size once the entry reaches 1 KiB. It also rejects OOXML macro (`vbaProject.bin`) and
digital-signature (`_xmlsignatures/`) parts as unsupported before XML inspection. These boundaries
are exercised by the Core document fixture and Linux wrapper fixtures
`rejects_docx_archive_with_suspicious_compression_ratio` and
`rejects_macro_and_signature_ooxml_packages_before_import`; they apply to DOCX, PPTX, and XLSX imports
before worker translation.

A sibling `../linguamesh-l10n` checkout at the revision pinned by `tools/sync-l10n.sh` is required
to verify the checked-in PO catalogs.

Validate localization provenance and gettext syntax with:

```sh
bash tools/sync-l10n.sh --check
python3 -B tools/check-localization-keys.py
for file in l10n/linux/*/LC_MESSAGES/linguamesh.po; do
  msgfmt --check --check-format -o /dev/null "$file"
done
```

Toolkit-independent validation requires no GTK development headers:

```sh
cargo fmt --all --check
cargo check --all-targets --features demo-provider --locked
cargo clippy --all-targets --features demo-provider --locked -- -D warnings
cargo test --no-default-features --locked
cargo test --features demo-provider --locked
cargo build --features demo-provider --locked
DOCS_RS=1 cargo check --all-targets --all-features --locked
```

The no-default suite contains 54 tests. It covers the text-import decoder, request-level glossary,
and explicit Incognito privacy policy in addition to the disconnected initial state, atomic
sorted restoration of multiple profiles without activation, duplicate/missing/default/session-ref
snapshot rejection, form-only selection, exact pending deletion, connected-row removal that keeps
the runtime session, pending and active canonical profiles, exact stale-result rejection, atomic
rollback, deliberate model selection, per-ID credential-free upserts, session switches that
preserve every restart row/default, active-ID model updates while another row is displayed,
saved-model restoration only when available, ordered events, partial output, all Core alpha.2 error
categories, the derived provider-setup stages through Ready, rollback that preserves the confirmed
Ready identity, pending-model confirmation that cannot claim Ready, worker-unavailable state,
storage-unavailable fallback, runtime persistence degradation that retains the confirmed session,
and diagnostics that omit content, endpoints, IDs, model IDs, and secret references.

The ordinary `demo-provider` run passes 104 tests and reports two intentionally ignored environment
test. Its worker tests validate the exact Core
compatibility contract including `long_text_chunking_v1`, prove that fake-service readiness does not auto-connect, require explicit
Connect and model selection, exercise real loopback HTTP/SSE streaming, consume an authenticated
session secret through the bounded typed host-secret broker, and fail closed for unavailable
session or persistent secrets. Persistence coverage creates two profiles with independent models,
restores the full list and active ID without provider requests, reconnects after explicit credential
re-entry, proves two credential values remain isolated, scans SQLite side files for both credential
and `session:` canaries, deletes inactive/missing/connected rows, keeps a deleted connected runtime
usable without recreating it, verifies exact `0700`/`0600` permissions, rejects a permissive parent,
symbolic ancestor, final database symlink, and hard-linked database without following unsafe paths,
and replaces the visible parent after `openat2(RESOLVE_NO_SYMLINKS)` while verifying the
descriptor-pinned Core migration remains in the original directory. It preserves every
restart row/default across session switches, failed persistent changes, and public connection
cancellation, and keeps session mode usable after storage initialization fails. A dedicated
preflight-race regression replaces the validated parent with a symlink between path validation and
descriptor opening and requires rejection before any database file is created. It also verifies
that a completed standard translation is recorded in bounded history, an Incognito completion is
skipped, and the startup count/clear command path uses the same database. A Linux-side
Scenario 5 regression authenticates and saves distinct providers A and B with independent models,
then uses one Connect action per remembered switch and proves each next translation reaches only
the newly confirmed provider. It rejects B with A's credential without changing the active B/model
pair, scans storage for both credentials and all secret references, and verifies the full
profile/model/default snapshot after restart. The suite also covers immediate connection
cancellation, translation cancellation with partial output, active,
queued, and full-command-queue shutdown, translation terminal delivery during shutdown,
delete rejection during translation, saved-model behavior, and failed-switch rollback to the
previous Core `ProviderManager` and model.

The isolated regressions are executed separately and must each pass exactly once:

```sh
bash tools/run-storage-fault-test.sh
bash tools/run-secret-service-test.sh
```

The runner compiles the exact library test as the calling user, enters a private mount namespace,
mounts an 8 MiB tmpfs, and fills it until the kernel returns `ENOSPC`. It prefers an unprivileged
user namespace. When Ubuntu restricts that mount, the CI fallback uses passwordless `sudo` only for
the private mount coordinator and drops back to the original UID/GID with `setpriv` before executing
the test binary; supplementary groups and inheritable, ambient, and bounding capabilities are
cleared, the environment is reset, and `no_new_privs` is enabled. It verifies failed persistent
model update, exact-ID deletion, and provider switch independently. Each operation must be rejected
before storage becomes unavailable, the previous engine/model must still translate, a subsequent
model change must remain session-only, and restart must restore only the pre-fault profile, default,
and model. The runner requires
`1 passed; 0 failed; 0 ignored` so a missing or skipped test cannot count as evidence, and cleanup
unmounts the private filesystem. This proves the implemented Linux `ENOSPC` transaction boundary;
the focused `read_only_database_directory_reports_error_but_session_mode_still_works` regression
additionally covers a non-writable private directory. Corrupt-database fail-closed behavior is
covered by the regular worker suite. Power loss and every SQLite VFS failure remain outside these
automated boundaries.

The Secret Service runner creates an isolated XDG data directory, starts a real `gnome-keyring`
Secret Service daemon on a private D-Bus session with a persistent `login` collection, stores and
resolves an item, locks the collection and verifies fail-closed lookup, stops and restarts the
daemon, then resolves and deletes the item before rerunning the cleanup round trip. It also runs the
worker secure-onboarding connect/translate/restart test and the GTK Remember/clear-form flow against
an authenticated loopback provider under Xvfb. It proves CRUD, persistent restoration, locked-item
handling, cleanup, and SecretRef-only persistence without touching a developer keyring.

The prompted-flow runner starts a separate Python Secret Service fixture four times. It returns a
non-root prompt path from `CreateItem` and `Delete`, then exercises both completion outcomes: an
approved prompt completes the store/delete operation, while a dismissed prompt fails closed with
`SecureStorageUnavailable` and the stable interactive-prompt message:

```sh
bash tools/run-secret-service-prompt-test.sh
```

This proves the adapter's prompt signal handling and fail-closed boundary through a private D-Bus
fixture; it does not claim that a real user approved or visually reviewed a desktop prompt. End-user
prompt acceptance and unlock UX remain separate manual validation gates. The GTK connection flow's
localized session-only recovery dialog is covered by source-level key audits and native CI
compilation; physical prompt approval and visual review remain manual.

The localization unit suite parses every official Linux MO catalog, checks action entries are
available, and verifies unique BCP 47 tags plus Arabic RTL metadata:

```sh
cargo test --features gui --lib localization::tests --locked
```

The checked-in PO sources must pass gettext syntax validation, and the generated MO table must be
readable by gettext tooling:

```sh
for file in l10n/linux/*/LC_MESSAGES/linguamesh.po; do
msgfmt --check --check-format -o /dev/null "$file"
done
msgunfmt l10n/linux/zh-Hans/LC_MESSAGES/linguamesh.mo >/dev/null
```

The opt-in image-only PDF path is validated with a private generated fixture. It requires
ImageMagick, Poppler, and Tesseract and runs the ignored external-plugin test explicitly:

```sh
bash tools/run-ocr-test.sh
```

The fixture proves page text is recovered through the bounded `pdftoppm`/`tesseract` boundary;
ordinary tests keep OCR disabled and cover unavailable-tool, malformed-PDF, and safety-limit paths.

The notification transport runner starts a private `org.freedesktop.Notifications` fixture service and
captures the real GTK translation test's `Notify` call on a private D-Bus session. It requires fixed
generic title and body text and absence of the source and translated strings. This proves the
application-to-notification-service transport and privacy boundary, not desktop-shell rendering.

The notification daemon runner starts the real `dunst` server under Xvfb, waits for it to own the
session notification name, runs the same GTK translation flow, verifies the daemon receives the
generic redacted payload, and checks that a visible, viewable Dunst desktop-shell window exists:

```sh
bash tools/run-notification-daemon-test.sh
```

This proves the X11 desktop-shell rendering path in the headless CI display; it does not prove a
physical compositor, GPU behavior, or visual placement on every desktop environment.

The document portal runner starts the real XDG document portal services on a private D-Bus session,
adds a temporary fixture through a file descriptor, verifies the returned host path, grants and
revokes the application read permission, and deletes the lease. This proves the document-portal
lease lifecycle without touching a developer file; application-level chooser and drag/drop callbacks
are exercised by the dedicated GTK fixture below.

The interactive file chooser runner starts the real `xdg-desktop-portal-gtk` backend under Xvfb,
issues the actual `FileChooser.OpenFile` request, injects a fixture path into the visible chooser,
and verifies the response URI and UTF-8 contents. This proves backend portal UI and lease behavior;
the Linux application's GTK `FileDialog` callback is verified by the application-level fixture:

```sh
bash tools/run-gtk-file-chooser-portal-test.sh
```

That runner launches the real binary with a hidden test environment, drives the visible chooser with
`xdotool`, and asserts the callback plus asynchronous GIO read markers.

The toolkit-independent suite also tests the text-import decoder for UTF-8 BOM removal, invalid
UTF-8 rejection, and the 4 MiB bound. The native GTK flow verifies the **Open text file** control
is focusable, is disabled after worker loss, and registers URI-list/GIO `DropTarget` types on the
source editor. The application-level drag fixture performs an actual XTest drag through the editor:

```sh
bash tools/run-gtk-drag-and-drop-test.sh
```

The fallback-enabled ordinary-text path also requires a localized confirmation window before dispatch;
the dialog's **Translate** action grants one request and **Close** sends nothing. Physical
desktop-shell rendering and prompted portal/keyring approval UI remain manual boundaries.

The GTK Rust source can be checked without native linking as a limited diagnostic. The `v4_10`
gtk-rs feature is enabled because the accessibility test helpers and semantic update APIs require
GTK 4.10 or newer:

```sh
DOCS_RS=1 cargo check --all-targets --all-features --locked
DOCS_RS=1 cargo clippy --all-targets --all-features --locked -- -D warnings
```

## Flatpak metadata validation

The primary Linux packaging scaffold is kept in `packaging/flatpak`. It pins the Linux and Core
source commits, vendors every registry dependency from `Cargo.lock`, installs the desktop entry,
AppStream metadata, and uses Wayland with practical X11 fallback, Secret Service, notifications,
network access, and only the application data directory. Validate the checked-in metadata with:

```sh
bash tools/validate-flatpak-metadata.sh
```

This check parses the manifest and Cargo source set, validates 40-character source pins and SHA-256
archives, and runs `desktop-file-validate` plus `appstreamcli`. The `Flatpak Linux` workflow runs
the pinned manifest in the GNOME 49 SDK container, uploads a prerelease CI bundle, and runs
`tools/run-flatpak-smoke.sh` under Xvfb with a private D-Bus session. That smoke only proves
installation and bounded application startup; it does not publish a release or prove interactive
file-chooser portal leases or physical desktop-shell notification rendering. `flatpak-builder` is not installed on this host, so local SDK
build and sandbox launch remain unavailable.

After the SDK build, the workflow runs `tools/create-flatpak-evidence.py` to emit a `SHA256SUMS`
sidecar and deterministic SPDX 2.3 SBOM from the bundle and locked Cargo dependency set. These
uploads are CI-only evidence and are not a stable release or signature.

The native workflow also builds the GTK binary in release mode and uploads it with deterministic
integrity sidecars. To reproduce the generator locally after a successful native build:

```sh
cargo build --release --bin linguamesh-linux --features gui --locked
mkdir -p native-evidence
cp target/release/linguamesh-linux native-evidence/linguamesh-linux
python3 tools/create-native-evidence.py \
  --binary native-evidence/linguamesh-linux \
  --cargo-lock Cargo.lock \
  --output-dir native-evidence \
  --linux-revision "$(git rev-parse HEAD)" \
  --core-revision "19229184a21a6725326a3d30dea9bc72e5ac999f" \
  --localization-revision "737d890e60fd34f15fd8708698448ef9ab96299f"
(cd native-evidence && sha256sum -c SHA256SUMS)
```

The workflow also adds `linguamesh-linux-source.tar.gz` to the same artifact and appends its digest
to `SHA256SUMS`. This is a repository-only source snapshot; it still requires the pinned Core and
localization repositories for a build. The binary, source archive, `SHA256SUMS`, `SBOM.spdx.json`,
and `BUILD-INFO.txt` are unsigned prerelease evidence only, not a stable or distributable release.
The generated `ROLLBACK.md` records the exact source pins and future signed-release rollback
sequence without inventing a previous stable revision.

The Native workflow also runs `tools/run-performance-baseline.sh` for representative DOCX
reconstruction, XLSX reconstruction, and saved-profile routing dispatch tests. It records the
kernel, CPU, memory, Rust, Core, and localization context with elapsed seconds in
`LINUX-PERFORMANCE-BASELINE.tsv`. These are machine-specific trend baselines and must not be quoted
as portable performance numbers.

These commands bypass sys-crate discovery and do not validate headers, ABI, linking, launch, or
display behavior. Their cached sys-crate output can also make a later ordinary Cargo check look
successful. The `pkg-config` commands below are therefore mandatory native-gate prerequisites;
run `cargo clean -p graphene-sys` before rechecking native discovery after a source-only check.
Do not extend this shortcut to `cargo test --no-run`: the GTK binary test still requires native
linking.

## Native GUI validation

On Debian or Ubuntu, install the native development headers:

```sh
sudo apt-get install \
  libgtk-4-dev libadwaita-1-dev dbus-daemon gettext gnome-keyring mount pkg-config python3 util-linux weston \
  xauth xvfb
```

Then run the complete native gate:

```sh
pkg-config --modversion gtk4
pkg-config --modversion libadwaita-1
cargo fmt --all --check
cargo clippy --all-targets --all-features --locked -- -D warnings
GDK_BACKEND=x11 dbus-run-session -- xvfb-run --auto-servernum \
  --server-args="-screen 0 1280x800x24" \
  cargo test --all-targets --all-features --locked -- --test-threads=1
bash tools/run-storage-fault-test.sh
dbus-run-session -- bash tools/run-wayland-test.sh
cargo build --all-targets --all-features --locked
cargo build --release --bin linguamesh-linux --features gui --locked
```

Run the development slice with:

```sh
cargo run --features gui
```

The all-feature binary test creates real GTK/libadwaita widgets, verifies the initial disconnected
state and the Provider setup progression from Starting through Configure, Connecting, Select model,
and Ready, then checks that Ready identifies the confirmed provider stable ID/model. It verifies
that pending model confirmation remains in Step 2 and fatal worker shutdown becomes Unavailable
instead of remaining at Starting, with Connect, model, translation, and stop controls disabled. It
waits for fake-endpoint readiness without auto-connect, clears a session credential from the form
immediately after Connect, explicitly selects a discovered model, preserves the active
provider/model and Ready identity after a failed switch, and completes a streamed translation. The
The same GTK binary regression selects the native Ollama preset, connects to the deterministic
`/api/` fixture, verifies `ollama_chat` model discovery, and translates `你好，Ollama！` without a
credential. A completed translation also exercises the registered application notification path; its payload is
  localized generic copy and does not contain source or translated content. It
also verifies the storage-unavailable session-only warning persists in Ready, injects a two-profile
startup snapshot, verifies persisted-active prefill without activation, browses another row without
changing the runtime/default, checks the disconnected storage warning, preserves a
persistent secret reference so the real Connect path fails closed when the test desktop has no
matching Secret Service item, rejects a disabled saved row
without re-enabling it, checks delete-pending control blocking, applies an exact deletion result,
and verifies a fresh random draft ID. The test runs once in a private D-Bus session under X11/Xvfb,
then the binary-only suite runs again in another private D-Bus session under forced Wayland and
headless Weston. `tools/run-wayland-test.sh` creates a private `0700` runtime directory, unsets
`DISPLAY`, sets `GDK_BACKEND=wayland`, waits at most ten seconds for a dedicated socket, and uses
exit traps to stop Weston and remove the directory. Tests are serialized because GTK owns
process-global state. The same test asserts the baseline accessibility roles, editor properties,
visible-label relations and mnemonics, focusability, explicit Stop name, hidden-empty-error
behavior, and Busy-state reset. The test also switches the runtime locale to Simplified Chinese and
verifies the catalog-backed Translate and Stop labels, then switches to Arabic and verifies RTL
direction without replacing the source editor buffer before restoring English. GTK's helpers prove
semantic presence and reset behavior. The dedicated `tools/run-gtk-keyboard-focus-test.sh` also runs
the real binary under Xvfb and `xfwm4`, injects Tab/Shift+Tab events, and asserts focus events for
the tested onboarding and workspace controls. The application-window Capture-phase handler keeps
the provider fields in an explicit Tab/Shift+Tab order while preserving modified shortcuts.
`tools/run-gtk-atspi-test.sh` starts
the AT-SPI bus, reads the live accessibility tree with `python3-pyatspi`, and verifies the named
Open, Translate, Retry, fallback-consent, and Stop controls with their expected roles, plus two
exported text-editor roles. The GTK unit test also verifies that document
progress uses the native progress-bar role, exposes a bounded completed/total fraction, and hides
the progress control when no document job is selected. This proves AT-SPI semantic export only; it does
not prove Orca speech, physical-compositor behavior, RTL/high-contrast presentation, or GPU
rendering. The diagnostics panel uses the catalog-backed `diagnostics.summary` template for its
Core ABI/protocol header, localizes fixed labels and state values through the Linux diagnostics
keys, and keeps source content, endpoints, identifiers, and secret references redacted.

`tools/run-orca-atspi-test.sh` adds the installed Orca process to a separate Xvfb/private-D-Bus
session. `tools/orca-atspi-inspect.py` finds the production `Stop translation` push button through
AT-SPI and confirms its focusable state; the fixture then requires Orca's debug stream to contain the
Linux application tree and a `SPEECH GENERATOR` record. This proves headless Orca integration and speech-generation
dispatch only. It does not replace a human listening review, physical desktop review, or a claim
about speech quality across locales.

The GitHub Actions native workflow pins Core revision
`19229184a21a6725326a3d30dea9bc72e5ac999f`, installs the headers plus D-Bus, Xvfb, test-only
mount-namespace tools, and Weston support, and runs the real storage write-fault gate and both
display gates before the all-feature build. The storage write-fault change passes its exact local
namespace test through the unprivileged path.

The GTK AT-SPI fixture bounds cleanup of its private application, window manager, and accessibility
launcher processes; a fixture that prints successful accessibility assertions but cannot reap its
processes is recorded as a failed gate rather than accepted as evidence.
The current Linux diagnostics localization revision `026c35b8dbb1c13c22d77809cc5fe72e6af6f5a3`
contains 422 catalog messages; the source-level catalog
audit and runtime locale tests cover the diagnostics labels. The current fixed-error localization revision `b6d2503`
passed Native Linux run `29627668119`, Foundation run `29627668093`, and
Flatpak run `29627668108`; the native job validated the pinned 117-message catalog and GTK
Simplified Chinese provider-card/source-target assertions in addition to the existing storage,
Secret Service, portal, notification, drag/drop, X11, and forced-Wayland gates. The workflow
checks out l10n revision `0ba26705e113230ae7d9e74db54039e1e82296ce`.
The current reducer-state localization revision `9f21836f214d3056934fac9322adc0f20791834e`
passed Native Linux run `29628307915`, Foundation run `29628307945`, and Flatpak run
`29628307886`; the native job validated the pinned 148-message catalog, localized fixed reducer
errors and category prefixes, and the existing storage, Secret Service, portal, notification,
drag/drop, X11, and forced-Wayland gates. The workflow checks out l10n revision
`08118b498646ebf56cbb072b937d95fceb34b75c`.
The MO integration revision `6c5bfb305967d0f01488ad09ade6e5b88eebbdb0` passed Native Linux run
`29628986188`, Foundation run `29628986160`, and Flatpak run `29628986187`; the workflow validates
both PO syntax and MO readability before the runtime GTK gates and checks out l10n revision
`0b906034784a1b5e81a879649abbfda001fa9e67`.
The current locale-selector coverage revision is validated locally with 184 catalog messages and
l10n revision `d21c3b0d065831b20cf31c9bf3009ffd262e4797`; it adds localized names for all twelve
official interface packs and refreshes those labels during runtime locale switching. The same
revision includes fixed translations for invalid UTF-8 import, storage fallback, provider/model
state, secret-channel, and profile validation failures before the native CI gate.
Runtime-storage functional revision
`c37702c76c3b1a2f9cec805cf9e219721ef7b5ce` passed Native Linux run `29586532049` (job
`87904787338`): strict all-feature Clippy, 66 ordinary library tests with one intentional ignore,
the real GTK binary test under X11/Xvfb, the exact storage-fault test with 1 pass and 0 ignored via
the controlled `sudo` fallback, the same GTK test under forced Wayland/headless Weston, and the
all-target all-feature build all succeeded. Repository-foundation run `29586531915` (job
`87904787120`) also passed. Wayland-gate revision
`10b31a040fd3c44ecbaef31eb5c66c0c8e5cb620` passed Native Linux run `29582513061` (job
`87891382469`): strict all-feature Clippy, 65 library tests, the real GTK binary test under
X11/Xvfb, the same test under forced Wayland/headless Weston, and the all-target all-feature build
all succeeded. Repository-foundation run `29582513073` (job `87891382540`) also passed. The
preceding functional onboarding revision
`9729b23ce1a4280ebb434339e880010103b4859d` passed Native Linux run `29580444723` (job
`87884607879`): strict all-feature Clippy, 65 library tests, the real GTK binary test, and the
all-target all-feature build all succeeded under the then-current X11 gate. Repository-foundation
run `29580444697` also passed.
The preceding functional multi-profile revision
`c88d37a5de2f03c2ae5d2940c4d25e5d998c301d` passed Native Linux run `29577918335` (job
`87876528763`): strict all-feature Clippy, 62 library tests, the real GTK binary test, and the
all-target all-feature build all succeeded. Repository-foundation run `29577918346` also passed.
The preceding functional persistence revision
`c58a54c2479045773358bd9c456b45a958e98e1e` passed Native Linux run `29574265570` (job
`87865028892`): strict all-feature Clippy, 50 library tests, the real GTK binary test, and the
all-target all-feature build all succeeded. Repository-foundation run `29574265553` also passed. A
local host without `gtk4.pc` or `libadwaita-1.pc` must not substitute this remote result for an
unexecuted local GUI build, launch, or GTK test.

Accessibility functional revision `d6bd2bd06ccdf04f3aead0c7f1da5ba74f84c550` passed repository
foundation run `29589043314` (job `87913221612`) and Native Linux run `29589043315` (job
`87913221576`). The native job passed the accessibility assertions described above together with
strict Clippy, 66 ordinary library tests and one intentional ignore, the exact storage-fault gate,
both display gates, and the all-target all-feature build. Its predecessor `e483ad8` failed at the
first focusability assertion because GTK dropdowns defaulted to non-focusable; the final revision
sets every labelled control and action explicitly focusable.

The history-policy checkpoint is Core `fb00f3dd6b62a8a3a47350acc85831e60e266929`, Linux
`7173d4a4217d6211c7dc92c368d9f033874198f5`, and l10n `40f3914e1b28fddd8f38d287fa121010f5192f1c`.
Local Core workspace tests, Clippy, offline build, and cargo-deny passed; Linux passed all-target
Clippy, 54 portable tests, and 82 demo-provider tests with one intentional ignore; l10n validation
passed with 244 messages. The Linux GUI test remains CI-linked because the local GTK libraries
cannot link the current GTK 4.10 symbols.
Core and l10n remote validation for this checkpoint is recorded after their pushes; local Core
storage tests passed 15 cases and Linux passed the policy worker regression. The Linux GUI test
remains CI-linked because the local GTK libraries cannot link the current GTK 4.10 symbols.
The Linux translation-memory controls are covered by the current worker/storage slice; remote
native and Flatpak evidence for this revision is recorded after the push.

## Repository foundation check

```sh
set -euo pipefail
required_files="README.md LICENSE AGENTS.md REPOSITORY_ROLE.md GLOBAL_GOAL.md SECURITY.md CONTRIBUTING.md CODE_OF_CONDUCT.md THIRD_PARTY_NOTICES.md IMPLEMENTATION_STATUS.md Cargo.toml Cargo.lock rust-toolchain.toml rustfmt.toml src/lib.rs src/model.rs src/worker.rs src/main.rs docs/architecture.md docs/testing.md docs/releasing.md tools/sync-l10n.sh tools/run-wayland-test.sh tools/run-storage-fault-test.sh l10n/compatibility.json l10n/manifest.json .gitignore .github/workflows/foundation.yml .github/workflows/native.yml"
for file in $required_files; do
  test -s "$file" || {
    printf 'Missing required file: %s\n' "$file"
    exit 1
  }
done
grep -Fqx 'Global goal SHA-256: `11f9a65927aac7e57e2af119e9d21cc98e8d5a08b8a112a19ee1c47903e36198`' GLOBAL_GOAL.md
if find . -type f \( -name '*.md' -o -name '*.yml' \) -not -path './.git/*' -exec awk '/[[:blank:]]$/ { printf "%s:%d: trailing whitespace\n", FILENAME, FNR; bad=1 } END { exit bad }' {} +; then
  printf '%s\n' 'Foundation validation passed.'
else
  exit 1
fi
git diff --check
```

The current Linux runtime/storage-error localization slice adds catalog-backed startup,
compatibility-read, and profile-database path/permission errors. Its portable regression test
asserts Simplified Chinese translations while preserving safe dynamic diagnostic detail.

## Unimplemented validation

Broader GTK component/UI automation, AT-SPI/Orca, and broader physical-keyboard coverage,
physical-compositor and GPU-backed Wayland coverage, a broader X11/desktop matrix, prompted
interactive Secret Service flows, broader XDG and portal tests, Flatpak
smoke tests, runtime localization behavior beyond the
currently catalog-backed UI and stable error paths, workspace-widget, active-provider, status summary/partial-output, text-file import, provider-profile, source/target language, onboarding stage/detail, and theme-option labels, runtime database
faults beyond the implemented Linux `ENOSPC` transaction boundary, dependency/license automation,
and release builds remain required before a supported release.
