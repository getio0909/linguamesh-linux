# Implementation Status

## 2026-07-20 — Linux OpenAI Responses typed-SSE slice

Assumption: Linux is the first active client target; the OpenAI Responses preset uses the shared
`/v1/models` discovery path and a session-only credential while live account, quota, and model
availability remain external gates.

- Core `58075c997cecdcd9a179b9397cb493da375d3a50` adds the `openai_responses` adapter, typed
  `response.output_text.delta`/`response.completed` decoding, and the `openai_responses_v1`
  compatibility feature.
- Linux adds the localized `openai-responses` preset and a worker regression that discovers
  `fake-translator`, makes one authenticated `/v1/responses` request, and streams
  `你好，Responses！` through the real `ProviderManager` path.
- Canonical l10n `95078b1a0c30defe98995a9879c4c669d213e5bc` contains 405 messages and generated Linux
  resources. This checkpoint does not claim a stable release.

## 2026-07-20 — Linux Azure OpenAI end-to-end worker fixture

Assumption: Linux is the first active client target; deterministic Azure loopback coverage proves
request shaping and session-secret handling while live Azure account, quota, deployment, and other
client behavior remain unverified.

- Core `e46066ccafcd81e50b004c84d7eb8734e77f3279` adds the `azure_openai_chat` adapter with a
  pinned `2024-10-21` API version, resource/deployment URL validation, `api-key` authentication,
  manual deployment selection, and a deterministic testkit fixture.
- Linux adds the localized Azure OpenAI preset and worker regression
  `azure_openai_provider_uses_manual_deployment_and_api_key`; it selects `fake-deployment`, makes
  no model-list request, and streams `你好，Azure！` through the real `ProviderManager` path.
- Canonical l10n `8e0e50577f8714b90bcc08a0d22cc790319f9239` contains 401 messages and generated Linux
  PO/MO resources. Local and remote gate evidence is recorded below after the final push.

## 2026-07-20 — Linux Gemini end-to-end worker fixture

Assumption: deterministic loopback coverage is sufficient for the Linux/Core integration gate,
while live Gemini account, quota, and credential behavior remain explicitly unverified.

- Core `232881263f4f523ce54b3713d83513f2d0170ff2` adds a Gemini Generate Content test server with
  `/v1beta/models` filtering and fragmented SSE candidates ending in `finishReason`.
- Linux adds `gemini_provider_discovers_and_streams_without_secret`, which exercises the real
  `ProviderManager` and worker path, deliberately selects `gemini-2.0-flash`, and completes
  `你好，Gemini！` without a credential. The Flatpak and Native workflows pin this Core revision.
- Local Core workspace tests passed (including 7 testkit tests); Linux formatting, GUI check,
  strict Clippy, demo-provider tests (`137 passed; 3 ignored`), localization audits, Flatpak
  metadata, and diff checks passed. Core CI/Native SDK `29735977442`/`29735977484` passed.
- Linux push Native/Flatpak/Foundation `29736052299`/`29736052289`/`29736052336` and PR
  `29736054831`/`29736054822`/`29736054819` all passed. The PR remains Draft/Open and CLEAN;
  live Gemini account/quota behavior, human review, other clients, signing, rollback, and stable
  release remain open.

## 2026-07-20 — Linux Google Gemini Generate Content provider

Assumption: Linux remains the only active client target for this slice; Android, Windows, and macOS
remain frozen while the shared provider contract is validated through deterministic loopback tests.

- Core `638713c34ce7d5bcc8003bb0d7e54c514ab49ea7` adds the `gemini_generate_content` adapter,
  model discovery, fragmented SSE streaming, cancellation, bounded protected-span/glossary
  restoration, endpoint policy, and redacted diagnostics. The provider uses the documented
  `/v1beta/models` and `:streamGenerateContent?alt=sse` shapes with an optional `x-goog-api-key`.
- Linux `67554cec96ff5774d9bfe4d99790d29a205cdc62` exposes a localized Google Gemini preset,
  preserves custom endpoint edits, keeps manual-model controls Anthropic-only, and restores the
  selected preset for saved profiles. Canonical l10n `f9d74a8f83a89540a58bba65477a5031031bd619`
  contains 396 messages and generated Linux PO/MO resources.
- Local validation passed Core formatting, strict Clippy, and workspace tests (all passing); Linux
  formatting, GUI all-target check, strict Clippy, demo-provider tests (`136 passed; 3 ignored`),
  localization synchronization and three audits, Flatpak metadata validation, and diff checks.
  The deterministic fixture does not claim live Gemini credentials, quota, or account coverage.
- Native/Flatpak/Foundation push and PR checks for this head are pending; the PR remains Draft/Open,
  the release train remains unreleased, and stable signing, rollback, human accessibility/visual
  review, and the other clients remain open.

## 2026-07-20 — Linux bounded concurrent document execution

Assumption: Linux is the first delivery target, so bounded worker concurrency can advance without
unfreezing the other clients or claiming a stable release.

- The worker now runs up to four document jobs concurrently. Each job owns its event pump,
  cancellation handle, partial output, provider manager, and segment index; a fifth or duplicate
  start is rejected before persistence changes the job to Running.
- Added regression coverage for independent concurrent completion and targeted cancellation of one
  job while its survivor completes. Full local validation passed: formatting, GUI all-target check,
  strict Clippy, demo-provider tests (`136 passed; 3 ignored`), and `git diff --check`.
- Native/Flatpak/Repository Foundation push runs `29732668572`, `29732668556`, and `29732668568`
  passed; the matching PR runs `29732671353`, `29732671354`, and `29732671362` also passed. These
  are the authoritative GTK, packaging, and sandbox evidence for code head `42b5ff3`. Cross-platform
  clients, human accessibility/visual review, signing, rollback, and stable-release authorization
  remain open.

## 2026-07-20 — Linux document-job concurrency isolation regression

Assumption: until bounded concurrent document execution is implemented, Linux must fail closed on
an overlapping document start rather than interrupting the active job or mutating the queued job.

- Added `concurrent_document_start_is_rejected_without_interrupting_active_job`, which starts a
  slow document translation, submits a second job while the first is streaming, asserts the typed
  configuration rejection, cancels the active job, and verifies the second job remains pending.
- Local targeted validation passed: `cargo fmt --all -- --check` and the filtered demo-provider test
  (`1 passed; 0 failed`). The Flatpak source pin now follows the code head
  `36b81586b8b148d7adc08ecfc46203b2ef94af4d`; no Core, l10n, workspace-manifest, or
  release-manifest pin changed. True concurrent document execution, cross-platform clients,
  signing, rollback, and stable-release authorization remain open.
- Full local Linux validation also passed: formatting, GUI all-target check, strict Clippy,
  demo-provider tests (`135 passed; 3 ignored`), Flatpak metadata, l10n synchronization, three
  localization audits, and diff checks. The first code-head Flatpak pin failures
  `29729850476` (push) and `29729852622` (PR) are retained; after pinning the code head,
  Native/Flatpak/Foundation push runs `29730049695`/`29730049744`/`29730049648` and PR runs
  `29730052583`/`29730052576`/`29730052602` all passed.

## 2026-07-20 — Linux GTK routing candidate reorder behavior

Assumption: candidate-management evidence must exercise the button callbacks and resulting row
order through the serialized GTK dialog lifecycle, not only assert that accessible controls exist.

- `0658f0f31083e0eb90259784dc2bfd0e642412ed` extends the restored-profile GTK regression to expose
  two enabled provider/model candidates, click the first sorted candidate's down and up controls,
  assert the visible candidate order changes and returns to its original order, then restore the
  disabled-profile fixture state for the remaining lifecycle assertions. The Flatpak source
  manifest is pinned to this exact Linux head; no provider endpoint, credential, or release pin
  changed.
- Local `cargo fmt --all -- --check`, `cargo check --features gui --all-targets --offline`, strict
  Clippy, demo-provider tests (`134 passed; 3 ignored`), Flatpak metadata validation, localization
  audits, l10n synchronization, and `git diff --check` passed. The host still cannot link the
  all-feature GTK test binary because its installed GTK runtime lacks gtk-rs symbols; Native CI is
  authoritative for GTK runtime execution.
- The first PR Native run `29727820986` correctly caught an ordering assumption in the new fixture
  (`left: A,B`, `right: B,A`); the test now follows the sorted saved-profile order and moves A down
  then back up. That failure remains recorded as regression evidence.
- The next PR Native run `29728076058` caught the temporary fixture-state change at the later disabled
  profile assertion; the corrected test restores the disabled profile before continuing the existing
  lifecycle checks. That failure also remains recorded as regression evidence.
- Final push Native/Flatpak/Foundation runs `29728346052`/`29728346055`/`29728346087` and final PR
  Native/Flatpak/Foundation runs `29728348382`/`29728348395`/`29728348472` all passed, including
  the real GTK lifecycle, release build, Flatpak bundle, sandbox smoke, and repository validation.
- This remains prerelease evidence. Human visual/translated-copy review, physical desktop review,
  other clients, signing, rollback, and stable-release authorization remain open.

## 2026-07-20 — Linux GTK routing candidate control regression

Assumption: candidate movement controls must be exercised through the real GTK dialog lifecycle,
not only through pure routing helpers; the existing serialized GTK lifecycle test is the safe
fixture because GTK initialization is thread-affine.

- `23abaf7b09adf2017bcedbdce9b521ca07b42b98` adds a GTK regression that constructs the Routing
  profiles dialog after restored provider profiles are available and verifies the localized
  keyboard-focusable up/down controls through their tooltips. The test then continues through the
  existing GTK flow; no provider endpoint, credential, or production routing contract changed.
- Local `cargo fmt --all -- --check`, GUI all-target check, strict Clippy, demo-provider tests
  (`134 passed; 3 ignored`), Flatpak metadata validation, and `git diff --check` passed. The host
  still cannot link the all-feature GTK test binary because its installed GTK runtime lacks the
  gtk-rs symbols; Native CI is authoritative for GTK runtime execution.
- The first remote test attempt failed on the new assertion (`29725417555`); moving the check to
  the healthy restored-profile phase fixed it. The next source-pin attempt failed only because
  Flatpak still referenced the previous Linux head (`29725940665`). Final push and PR
  Native/Flatpak/Foundation checks for `9c1fa0b9ed32782f67a4dbb10b1d7f58be6d7df8` all passed:
  push `29726187490`/`29726187520`/`29726187565`, PR `29726189998`/`29726189990`/`29726189988`.
- This is prerelease evidence. Human visual/translated-copy review, physical desktop review,
  other clients, signing, rollback, and stable-release authorization remain open.

## 2026-07-20 — Linux Secret Service prompt protocol evidence

Assumption: automated prompt fixtures may prove Secret Service signal/response handling, but they
must not be presented as evidence of a real user's desktop approval or visual review.

- `bash tools/run-secret-service-prompt-test.sh` passed all four private-D-Bus cases: approved and
  dismissed `CreateItem` prompts, plus approved and dismissed `Delete` prompts. The accepted cases
  complete the operation; dismissed cases return the typed `SecureStorageUnavailable` error.
- Updated `docs/testing.md` to distinguish adapter protocol coverage from the still-manual end-user
  prompt approval and unlock-UX gate. No credential value, production source pin, or release status
  changed.

## 2026-07-20 — Linux document queue documentation consistency

Assumption: the existing GTK queue-selection surface should be described as implemented, while
bounded single-active-job execution remains an explicit validation boundary.

- Corrected the architecture description to match the existing `document_job_list_returns_multiple_saved_jobs_for_queue_selection`
  regression and GTK job-selection dialog.
- No runtime behavior, provider routing, persistence schema, or release pin changed. Concurrent
  document execution, human accessibility/visual review, other clients, signing, rollback, and
  stable-release evidence remain open.

## 2026-07-20 — Linux Anthropic Messages provider preset

Assumption: Anthropic Messages remains a manual-model provider on Linux until a provider catalog
service is intentionally introduced; the UI must collect the model ID before any SecretRef is
resolved.

- Added the localized Anthropic Messages preset with the HTTPS `/v1/` default endpoint, manual
  Model ID field, saved-model restoration, and the existing session-only/Secret Service credential
  flow. Empty model IDs fail locally before a worker connection or host-secret request.
- Added the GTK focus/accessibility path for the conditional manual-model field and the regression
  `anthropic_preset_requires_manual_model_before_connecting`.
- Canonical l10n revision `e1ee15a5e9470e2c49077e52b4969597a5c8283f` contains 393 messages and all
  generated PO/MO resources. Local l10n tests, generation, build, Linux localization audits,
  formatting, all-target/all-feature check, strict Clippy, demo-provider tests (`134 passed; 3
  ignored`), synchronization, and diff checks passed.
- The host cannot link the all-feature GTK test binary because its installed GTK runtime lacks
  symbols required by gtk-rs; Native CI remains the authoritative GUI/Secret Service/Flatpak gate.
  The first push/PR run `29720882684`/`29720886129` and `29720882696`/`29720886096` were recorded
  as failures because the Flatpak source pin used a malformed commit name; the next Native run
  exposed the standalone GTK test's cross-thread initialization and was corrected by folding the
  regression into the existing GTK lifecycle test. Final push Native/Flatpak/Foundation
  `29721394859`/`29721394882`/`29721394847` (jobs `88284977586`/`88284977776`/`88284977563`)
  and PR Native/Flatpak/Foundation `29721397504`/`29721397510`/`29721397543` (jobs
  `88284984553`/`88284984617`/`88284984699`) all passed. The PR remains Draft/Open and the
  release train remains unreleased.

## 2026-07-20 — Linux Core Anthropic compatibility pin

Assumption: the Linux-first client should consume the verified Core Anthropic Messages adapter even
before the Linux GTK form exposes an Anthropic-specific preset. The shared Core revision is pinned
to `a87aaf2bef7cca287c4a6faa8addd340e0245b0e`; this adds the manual-model `anthropic_messages`
adapter while preserving the existing Linux provider choices and exact compatibility contract.

- Updated the Native Linux workflow, Flatpak source manifest, local documentation, and lockfile to
  the new Core revision. The lockfile now records the Core provider package without adding a Linux
  production dependency outside the shared workspace.
- Local `cargo fmt --all --check`, demo-provider check, strict Clippy, no-default-feature tests
  (`79 passed; 1 ignored`), demo-provider tests (`134 passed; 3 ignored`), demo-provider build,
  localization key/placeholder/visible-string audits, l10n synchronization, Flatpak metadata
  validation, and diff checks passed.
- Local all-feature check and Clippy passed. The all-feature test binary could not link on this
  workstation because the installed GTK runtime lacks symbols required by the gtk-rs headers;
  this is an environment linker limitation, not a Rust test assertion. Native CI remains the
  authoritative full GTK/Flatpak gate.
- The first pin-refresh head `0b7696e65b322a0ba948c207fe3b10599e7b6f86` correctly failed only
  Flatpak source-manifest validation because its Linux source entry still named the prior commit.
  The manifest was refreshed to the final head `a381d726cb163b8c0546d77e99ef2704898d58ce`.
  Final push and PR Native/Flatpak/Foundation runs all passed: push `29719144958`/
  `29719144935`/`29719144961` (jobs `88278214130`/`88278214125`/`88278214202`) and PR
  `29719143513`/`29719143521`/`29719143509` (jobs `88278209789`/`88278209742`/
  `88278209753`). `gh pr checks 1` reports all six checks passing.
- The Linux UI now claims the Anthropic preset at the source and automated GTK-regression level;
  human visual review, end-user Secret Service prompt approval, other native clients, signing,
  rollback, and stable-release evidence remain open.

## 2026-07-20 — Linux Secret Service session-only fallback guidance

Assumption: a declined or unavailable Secret Service prompt must not silently change a user's
request to remember credentials; the Linux client must provide an explicit session-only recovery.

- The provider connection flow now presents a localized modal warning after a persistent
  Secret Service store failure. **Use session-only mode** disables Remember and returns focus to the
  credential field; **Close** leaves the connection unsubmitted. No credential is persisted in the
  fallback path, and the existing typed error remains visible in the workspace.
- Local Linux formatting, all-target/all-feature check, strict Clippy, demo-provider tests
  (`134 passed; 3 ignored`), Flatpak metadata validation, and diff checks passed. End-user approval
  of the desktop keyring prompt and physical visual review remain manual boundaries.
- The first `7c2fe0a` push/PR Native and Flatpak runs (`29717314361`/`29717314328` and
  `29717312990`/`29717312998`) correctly failed on the canonical placeholder audit and stale
  Flatpak source pin; the fallback text and pin were corrected without weakening either check.
- Final source-pin head `89e2b534d3efb3c6719eb4c731ab22820419f0b9` passed push
  Native/Flatpak/Foundation `29717505522`/`29717505525`/`29717505575` (jobs
  `88273550108`/`88273550195`/`88273550212`) and PR Native/Flatpak/Foundation
  `29717506936`/`29717506979`/`29717506956` (jobs `88273554142`/`88273554144`/
  `88273554114`).

## 2026-07-20 — Linux read-only profile storage fallback

Assumption: a profile database directory mounted or configured read-only must fail closed for
persistent mutations while preserving session-only translation.

- The regression `read_only_database_directory_reports_error_but_session_mode_still_works` runs
  the worker against a private `0500` database directory, verifies a typed persistence failure,
  completes a session-only fake-provider translation, and confirms that no database file is
  created. Directory permissions are restored before test cleanup.
- Local format/check/Clippy/full-test validation and Flatpak metadata/source validation passed.
- Push Native/Flatpak/Foundation `29716560386`/`29716560397`/`29716560392` (jobs
  `88270888992`/`88270889198`/`88270889141`) and PR Native/Flatpak/Foundation
  `29716561843`/`29716561828`/`29716561907` (jobs `88270892907`/`88270892905`/
  `88270893106`) all completed successfully. Corruption and `ENOSPC` boundaries remain
  separately documented; power-loss and broader SQLite VFS behavior are still open.

## 2026-07-20 — Linux descriptor-pinned database open

Assumption: Linux profile storage must keep the exact validated database inode fixed through the
Core migration/open call, not merely preflight a pathname.

- Linux opens the parent directory with `openat2(RESOLVE_NO_SYMLINKS)`, opens the final regular
  file with `O_NOFOLLOW | O_CLOEXEC`, and hands Core the live `/proc/self/fd/<fd>` descriptor path.
  Core's ordinary path open remains no-follow; only the validated descriptor form is accepted by
  `Storage::open_from_trusted_descriptor`.
- The regression `pinned_database_parent_survives_path_replacement` renames the validated parent,
  replaces its visible path with a symlink to an alternate directory, and verifies migrations
  still land in the pinned inode's directory. Local format/check/Clippy/full-test validation and
  Flatpak metadata/source validation passed.
- Source pin correction `3b26c0795ecd369aee2b99a211c8e6408ed208ac` passed all six Linux gates after
  the first code push's expected stale-pin failures: push Native/Flatpak/Foundation
  `29715284721`/`29715284671`/`29715284678` (jobs `88267263432`/`88267263349`/`88267263588`) and
  PR Native/Flatpak/Foundation `29715287347`/`29715287327`/`29715287399` (jobs
  `88267269342`/`88267269231`/`88267269534`) all completed successfully.
- The final status-document head `c5d36eb354d115047d5da84fe02d36da57586e30` also passed push
  Native/Flatpak/Foundation `29715545070`/`29715545071`/`29715545056` (jobs
  `88268027779`/`88268027804`/`88268027728`) and PR Native/Flatpak/Foundation
  `29715546522`/`29715546521`/`29715546520` (jobs `88268031921`/`88268031905`/`88268031836`).

## 2026-07-20 — Linux final database no-follow hardening remote verification

- Source revision `39712ab0dabe26980a076a9068d6fb7282364d94` passed all six required GitHub checks.
- Push evidence: Native `29713770948` (job `88262506277`), Flatpak `29713770918` (job
  `88262506137`), and Foundation `29713770942` (job `88262506324`) all completed successfully.
- Pull-request evidence: Native `29713772062` (job `88262510191`), Flatpak `29713772061` (job
  `88262510201`), and Foundation `29713772058` (job `88262510179`) all completed successfully.
- The final database component is opened with Linux `O_NOFOLLOW | O_CLOEXEC`; local regression and
  full validation passed. Parent-directory replacement races still require a future
  directory-descriptor or `openat2` design and remain outside this checkpoint.

## 2026-07-20 — Linux Manual routing candidate cardinality

Assumption: Manual routing must identify exactly one provider/model; candidate chains belong to
Ordered and Automatic modes.

- The GTK editor now deactivates extra Manual selections when a profile is loaded or the mode changes,
  and the save path normalizes Manual selections to the first displayed candidate. Ordered and
  Automatic retain their selected chains.
- Local `cargo fmt --all -- --check`, `cargo check --all-targets --all-features --locked --offline`,
  `cargo clippy --all-targets --all-features --locked --offline -- -D warnings`,
  `cargo test --features demo-provider --offline` (`131 passed; 3 ignored`),
  `bash tools/validate-flatpak-metadata.sh`, and `git diff --check` passed. Source revision
  `a75468a6666a1954b85a8dbc646b4cb07144bf93` then passed all six GitHub checks: push Native
  `29712945266` (job `88260013464`), push Flatpak `29712945264` (job `88260013439`), push
  Foundation `29712945321` (job `88260013652`), PR Native `29712946196` (job `88260016618`),
  PR Flatpak `29712946158` (job `88260016478`), and PR Foundation `29712946166`
  (job `88260016565`). `gh pr checks` reports all six as pass.

## 2026-07-20 — Linux final database component no-follow hardening

Assumption: the Linux host should reject a final database path component swapped to a symbolic
link during open, while the existing Core no-follow gate remains authoritative for SQLite.

- Linux now opens the profile database with `O_NOFOLLOW | O_CLOEXEC` in addition to the existing
  static path checks and post-open inode comparison. A regression proves a symlinked database file
  is rejected without following or modifying its target.
- Local `cargo fmt --all -- --check`, targeted and full demo-provider tests (`132 passed; 3 ignored`),
  all-target/all-feature offline check, strict Clippy, Flatpak metadata validation, and diff checks
  passed. Parent-directory replacement races still require a future directory-descriptor or
  `openat2` design; this checkpoint does not claim that stronger guarantee.

## 2026-07-20 — Linux third-party Ollama interop harness

Assumption: the deterministic `/api` fixture is not evidence of interoperability with an
independently running Ollama daemon, so the external path must be opt-in and model-explicit.

- Linux adds `tools/run-ollama-interop-test.sh` and an ignored worker regression that performs real
  `/api/tags` discovery and `/api/chat` translation through the `ollama` preset without a secret.
  The script can start an isolated daemon and only pulls a model when `LINGUAMESH_OLLAMA_PULL=1`.
- Local default validation passed with `131 passed; 3 ignored`, including the new external test.
  External validation then passed with `LINGUAMESH_OLLAMA_MODEL=qwen2.5-0.5b-instruct:latest`
  through a temporary Docker `ollama/ollama:0.11.10` daemon: the harness reported `1 passed;
  0 failed`. The source GGUF was fetched from the public Qwen repository with SHA-256
  `9ee36184e616dfc76df4f5dd66f908dbde6979524ae36e6cefb67f532f798cb8`; the Ollama model digest was
  `91a334af822cdceab2234d673b0099d726d4944e1997b275744f4418e8b6a254`. The model and daemon were
  removed after the run. This closes the Linux third-party Ollama daemon/model interoperability
  gate for this prerelease checkpoint; it does not claim GPU or stable-release evidence.

## 2026-07-20 — Linux fallback-send confirmation checkpoint

Assumption: explicit fallback consent must be visible at the moment content could cross to the
approved provider, not only when the checkbox is configured.

- Linux now opens a localized, modal confirmation window before an ordinary text request with
  fallback enabled is dispatched. **Translate** grants one request and **Close** cancels without
  queueing a worker command; the existing retryable-error and partial-output policy is unchanged.
- The one-shot approval state is reset after dispatch, and a focused unit regression covers enabled,
  approved, and disabled combinations. Local `cargo fmt --all -- --check` and
  `cargo test --features demo-provider --offline` passed (`131 passed; 2 ignored`).
- Secret Service and portal unlock prompts, physical desktop review, other clients, signing,
  rollback, and stable release remain open.

## 2026-07-20 — Linux headless Orca remote gate evidence

- Source revision `94e98e71eeb9edd9d0196230e1864ba2a63a9644` passed all six required GitHub checks.
- Push evidence: Native `29710341677` (job `88253271782`), Flatpak `29710341643` (job
  `88253271710`), and Foundation `29710341660` (job `88253271731`) all completed successfully.
- Pull-request evidence: Native `29710342976` (job `88253274294`), Flatpak `29710342978` (job
  `88253274312`), and Foundation `29710342987` (job `88253274307`) all completed successfully.
- The Native log records `Orca AT-SPI fixture passed`, the named control inspection, and the
  Linux application-tree `SPEECH GENERATOR` assertion. This is reproducible process evidence only;
  the remote GTK4/Orca focus handoff limitation and human-listening boundary remain documented above.

## 2026-07-20 — Linux headless Orca/AT-SPI integration checkpoint

Assumption: the next accessibility gate should exercise the installed Orca process against the live
GTK accessibility tree while keeping human listening and physical desktop review as separate gates.

- Linux adds `tools/run-orca-atspi-test.sh` and `tools/orca-atspi-inspect.py`. The fixture starts
  Orca with Speech Dispatcher in an isolated Xvfb/private-D-Bus session, confirms the production
  `Stop translation` control through AT-SPI, and requires Orca's debug stream to contain the Linux
  application tree plus a `SPEECH GENERATOR` record. The remote runner exposed a GTK4/Orca focus
  handoff limitation: the control is confirmed by the AT-SPI inspector, while Orca's recorded
  speech-generator evidence is for the application tree rather than a human-listened label.
- Native CI installs the test-only Orca and Speech Dispatcher packages and runs this fixture after
  the existing AT-SPI semantic export check; Foundation protects both fixture files from omission.
- Local execution is unavailable on this host because `xvfb-run` and `python3-pyatspi` are not
  installed. Shell syntax and Python bytecode compilation were checked; remote Native CI is required
  before this checkpoint is treated as verified.
- This advances headless Orca process and speech-dispatch evidence only. Human listening, translated-copy/RTL review,
  physical desktop behavior, other clients, signing, rollback, and stable release remain open.

## 2026-07-20 — Linux Orca fixture focus correction

- Native push run `29709041999` reached the new Orca fixture but failed at job `88250409247` because
  the production Stop button is intentionally disabled while idle, so AT-SPI correctly rejected
  `grabFocus()` with an accessibility error. The existing AT-SPI semantic fixture passed first.
- The fixture now sets `LINGUAMESH_TEST_ORCA_ATSPI=1`; `refresh_ui` enables only that named control
  for the isolated test process, leaving normal idle production behavior unchanged. The exact
  headless Orca and speech-generator assertions remain unchanged and require a rerun.

## 2026-07-19 — Linux localization fallback-template consistency checkpoint

Assumption: a catalog-backed key must use its canonical English source text as the literal runtime
fallback; placeholder-only checks are insufficient when copy drifts between Rust and l10n.

- Linux aligns literal fallback strings for document-job controls/progress, glossary import errors,
  routing-profile tooltips, and document-job selection with the canonical catalog. The routing mode
  tooltip now uses the dedicated `tooltip.routing_profiles` key instead of overloading the dialog
  title key.
- `tools/check-localization-placeholders.py` now rejects canonical-text drift in addition to malformed
  braces and placeholder identity drift. The dependency-free audit still skips dynamic keys and
  non-literal constants, which remain covered by the key audit and runtime tests.
- The initial full local suite had one transient HTTP 502 in an unrelated profile-restore test; the
  exact test rerun passed, and the final full suite completed with `131 passed; 2 ignored`.

## 2026-07-19 — Linux localization fallback-template consistency remote verification

- Linux `c3aaedfb60f4f65bab4abc67c019b0e3be3538e8` passed push Native/Flatpak/Foundation
  `29708171935`/`29708171940`/`29708171993` and PR Native/Flatpak/Foundation
  `29708173277`/`29708173249`/`29708173224`.
- An earlier Flatpak push `29708101383` correctly failed because the source-pin validator detected
  build-path changes at `25011190fbb4522a6d2c39407b88177d39bac71e`; updating the manifest pin fixed
  the gate without weakening validation. Final Native/Flatpak evidence artifacts were non-expired.
- This verifies canonical fallback copy and placeholder enforcement in CI; translated-copy/visual/Orca
  review, third-party daemon interoperability, other clients, signing, rollback, and stable release
  remain open.

## 2026-07-19 — Linux localization placeholder audit checkpoint

Assumption: catalog-backed fallback templates are part of the Linux visible-string contract, so
their placeholder identities must be checked at source level before GTK or release validation.

- Linux adds `tools/check-localization-placeholders.py`, a dependency-free parser for literal
  `text`, `text_plural`, mnemonic, and template calls. It rejects malformed braces and placeholder
  drift against the canonical l10n catalog while ignoring dynamic keys and non-literal constants.
- Native and Foundation CI run this audit beside the existing key and visible-string audits; the
  foundation required-file list now protects the checker from disappearing during repository work.
- Local validation passed Python compilation, l10n synchronization, all three localization audits,
  formatting, and `cargo test --features demo-provider --offline` (`131 passed; 2 ignored`).
- This closes source-level fallback-template validation only; human translated-copy review, Orca
  speech, broader runtime locale coverage, other clients, signing, and stable release remain open.

## 2026-07-19 — Linux localization placeholder audit remote verification

- Linux `3a20620eb95806baadb1b22ef4833302d0438fea` passed push Native/Flatpak/Foundation
  `29707410914`/`29707410888`/`29707410893` and PR Native/Flatpak/Foundation
  `29707412487`/`29707412476`/`29707412474`.
- The push Native evidence artifact was non-expired at 5,589,644 bytes; the push Flatpak bundle and
  evidence artifacts were non-expired at 3,021,899 and 4,936 bytes. PR Native and Flatpak artifacts
  were also non-expired. This verifies CI enforcement of the source-level placeholder audit without
  claiming reviewed translations, Orca speech, other clients, signing, or a stable release.

## 2026-07-19 — Linux localization placeholder audit documentation verification

- Documentation-only head `c1ddd0f4b3055dd18f93b44b55d2666629044aa0` passed push
  Native/Flatpak/Foundation `29707583865`/`29707583895`/`29707583879` and PR
  Native/Flatpak/Foundation `29707585705`/`29707585670`/`29707585669`.
- The Native push evidence artifact was non-expired at 5,587,305 bytes; push and PR Flatpak bundle
  and evidence artifacts were also non-expired. This records CI verification of the documentation
  checkpoint and does not promote the unsigned artifacts to a stable release.

## 2026-07-19 — Linux performance baseline checkpoint

Assumption: release hardening needs reproducible, machine-contextual measurements before a stable
performance budget can be set; no portable number is inferred from one runner.

- Linux adds `tools/run-performance-baseline.sh`, which times exact DOCX reconstruction, XLSX
  reconstruction, and saved-routing dispatch tests and records kernel, CPU, memory, Rust, Core, and
  localization context in `LINUX-PERFORMANCE-BASELINE.tsv`.
- Native CI runs this baseline beside the release binary and uploads it in the non-expired native
  evidence artifact. The output is a trend baseline only, not a cross-machine claim or stable-release
  performance guarantee.
- Local baseline evidence on this host measured DOCX reconstruction at `0.404s`, XLSX reconstruction
  at `0.382s`, and saved-routing dispatch at `0.399s`; these values are retained only as host context.
- Linux `a511ea4ab5e95d3c94c6076b740471242fc4670c` passed push Native/Flatpak/Foundation
  `29706528034`/`29706528033`/`29706528037` and PR Native/Flatpak/Foundation
  `29706529098`/`29706529114`/`29706529109`. Native push/PR artifacts were non-expired (5,585,827
  and 5,587,044 bytes); downloaded push evidence contained the binary, source archive, checksums,
  SBOM, build context, and `LINUX-PERFORMANCE-BASELINE.tsv`.

## 2026-07-19 — Linux performance baseline remote verification

- Documentation-only head `c01b86ed49587c46ea4c8172bea7741f9d995919` passed push
  Native/Flatpak/Foundation `29706725013`/`29706726466`/`29706726471` and PR
  Native/Flatpak/Foundation `29706726505`/`29706724981`/`29706724970`.
- The current-head Native push artifact was non-expired at 5,587,469 bytes; its downloaded contents
  include the release binary, repository-only source archive, checksums, SPDX SBOM, build context,
  and all three exact performance-baseline rows. This remains machine-specific prerelease evidence.

## 2026-07-19 — Linux native release-mode evidence checkpoint

Assumption: the next Linux release-engineering slice should make the native binary reproducible in
CI and expose integrity metadata without implying that an unsigned build is stable or distributable.

- Linux adds `tools/create-native-evidence.py`, a dependency-free generator for a release-mode
  binary's SHA-256 sidecar and deterministic SPDX 2.3 SBOM from `Cargo.lock`. Native CI builds
  `linguamesh-linux` with `--release`, uploads the binary with `SHA256SUMS`, `SBOM.spdx.json`, and
  a fixed-context `BUILD-INFO.txt`, and the foundation gate requires the generator.
- Local self-checks validate Python compilation, the SHA-256 sidecar, SPDX schema, and the 230-package
  locked dependency set. Remote Native/Flatpak/Foundation and PR gates are required before this
  checkpoint is considered verified.
- This remains unsigned prerelease evidence; source archives, signing, rollback, and stable-release
  authorization remain open.

## 2026-07-19 — Linux source-archive evidence checkpoint

Assumption: a repository-only source snapshot is useful release evidence when its external Core and
localization pins remain explicit, but it must not be presented as a standalone source release.

- Native CI now adds `linguamesh-linux-source.tar.gz` for the reviewed commit and appends its SHA-256
  to `SHA256SUMS` beside the native binary evidence. The archive intentionally contains only this
  Linux repository and still requires the pinned sibling Core and l10n repositories to build.
- This source snapshot is unsigned CI evidence; stable source archives, signing, rollback, and
  distributable release authorization remain open until the coordinated release train is complete.

## 2026-07-19 — Linux source-archive remote verification

- The first source-archive push run `29705840151` failed after all AT-SPI assertions passed because
  asynchronous portal files made the fixture's final `rm` return non-zero. The cleanup now retries
  bounded deletion of its exact temporary directory; no product assertion was weakened.
- Linux `0edefcf464d4a81a4f4ae76595a750225eca887d` then passed push Native/Flatpak/Foundation
  `29705945663`/`29705945669`/`29705945658` and PR Native/Flatpak/Foundation
  `29705946524`/`29705946525`/`29705946526`. Native push and PR evidence artifacts were non-expired
  (5,584,223 and 5,584,054 bytes) and include the binary, source archive, checksum, SBOM, and
  build context; Flatpak bundle/evidence artifacts were also non-expired.

## 2026-07-19 — Linux native evidence remote verification

- Linux `8896aaa6e91e9ee482590701c925dabab96435de` passed the complete push gates: Native
  `29705286112`, Flatpak `29705286140`, and Foundation `29705286105`; the duplicate PR gates
  Native `29705287404`, Flatpak `29705287405`, and Foundation `29705287416` also passed.
- The push Native run uploaded the non-expired artifact
  `linguamesh-linux-native-evidence-8896aaa6e91e9ee482590701c925dabab96435de` (4,933,797 bytes).
  The PR Native run uploaded its corresponding non-expired artifact (4,933,798 bytes). Each
  contains the release-mode binary, `SHA256SUMS`, `SBOM.spdx.json`, and `BUILD-INFO.txt`.
- The push Flatpak run also retained non-expired bundle and evidence artifacts. This validates
  Linux CI packaging evidence only; the build is unsigned, unreleased, and not a stable artifact.

## 2026-07-19 — Flatpak checksum and SBOM evidence checkpoint

Assumption: Linux prerelease packaging should emit reproducible integrity evidence without implying
that an unsigned CI artifact is a stable release.

- Added `tools/create-flatpak-evidence.py`, which hashes the generated Flatpak bundle and emits a
  deterministic SPDX 2.3 SBOM from the checked-in `Cargo.lock` package set.
- The Flatpak workflow uploads the bundle's `SHA256SUMS` and `SBOM.spdx.json` as CI-only sidecars;
  the foundation check requires the generator. No release, signature, or notarization is claimed.
- Local metadata and source-level localization checks remain passing. Remote evidence is pending
  for the new artifact-evidence steps.

## 2026-07-19 — Flatpak source-pin integrity checkpoint

Assumption: a passing Flatpak gate is only evidence for the Linux revision under review; the
manifest must not silently build an older remote commit.

- Updated `packaging/flatpak/dev.linguamesh.LinguaMesh.yml` to pin the Linux source to the current
  checkout `2386d495123d3aeacf2b5815d0c45577808c7a44`.
- `tools/validate-flatpak-metadata.sh` now verifies that the manifest's `linguamesh-linux` git
  source commit equals the current checkout or an ancestor with unchanged build inputs; the
  Flatpak workflow runs this check before building.
- Local metadata validation and diff checks passed. Remote evidence is pending for this packaging
  pin correction; no distributable or stable release artifact is claimed.

## 2026-07-19 — Linux visible-string localization audit checkpoint

Assumption: complete Linux gettext coverage requires a repeatable source check that rejects
non-empty visible GTK literals, while empty labels used to clear transient state remain valid.

- Added `tools/check-visible-localization.py`, a dependency-free audit for GTK labels, titles,
  tooltips, placeholders, dialog actions, and direct list options. It passes the current source
  and its self-check detects both direct literals and localized helper calls correctly.
- Native and Foundation workflows now run the visible-control audit beside the catalog-key audit;
  the repository foundation check requires the new script. The l10n consumer pin is synchronized
  to `3362732be198450ff1ca00f30ec092aab2cf4189`, whose generated resources remain the verified
  387-message bundle.
- Local formatting, GUI-feature check, strict Clippy, 131 demo-provider tests with 2 ignored,
  both localization audits, l10n synchronization, Flatpak metadata, and diff checks passed.
- Remote Linux and central evidence will be recorded after the current head passes its gates.

## 2026-07-19 — Linux complete routing-constraint editor checkpoint

Assumption: the Linux routing-profile editor should expose every non-secret Core constraint that a
user can safely configure, while blank numeric inputs mean no profile-level limit.

- Linux working revision adds comma-separated provider/model allowlists and denylists, optional
  minimum quality tier, and optional maximum request bytes to the existing routing editor. Edit
  restores these values; Save rejects empty list items, unsafe identifiers, zero limits, and values
  outside the Core quality-tier range.
- l10n `3362732be198450ff1ca00f30ec092aab2cf4189` contains 387 messages and all 59 generated
  resources; Linux consumes the immutable revision and audits the new dynamic labels plus error
  key against the canonical catalog.
- Remote evidence will be recorded after the Linux and l10n pins pass their current-head gates.

## 2026-07-19 — Linux routing constraint controls checkpoint

Assumption: Core's non-secret routing constraints must be user-editable in the native Linux
profile dialog; editing visible controls must preserve future Core fields that the GTK surface does
not yet expose.

- Linux adds localized controls for Automatic preference (none/local/quality/latency/cost), local-only
  routing, remote-candidate permission, privacy-sensitive request protection, streaming capability,
  and document capability. Local-only and remote permission are mutually exclusive in the UI.
- Existing profile edits restore these controls and preserve hidden allow/model lists, minimum quality,
  and request-size limits when saving; pure helper tests cover preference mapping and preservation.
- l10n `b871a881f0eaf88cdda67a50f9221375f4c814ce` contains 377 messages and all 59 generated resources;
  Linux consumes the immutable revision and audits 253 catalog-backed source keys.
- Remote Linux/l10n evidence will be recorded after the pinned-resource validation completes.

## 2026-07-19 — Linux editor text-metrics checkpoint

Assumption: users need a non-sensitive size summary while editing, but tokenization remains
provider/model dependent, so the UI must label the token value as approximate and never log text.

- Linux now shows localized source and output character counts plus a clearly approximate token
  estimate; source-buffer changes update immediately and output metrics refresh with translated UI.
- l10n `8adb1f4558e4b1d93a00ce03cf026a98d4a1a5ed` adds `status.text_metrics` to all twelve official
  packs; the deterministic bundle contains 360 messages and the Linux source audit covers 236 keys.
- Local validation passed formatting, GUI all-target checks, strict Clippy, demo-provider tests,
  localization synchronization/key audit, Flatpak metadata, and diff checks.

This improves editing feedback without claiming provider-specific token accuracy, full Orca speech,
manual visual review, other clients, release artifacts, or a stable release.

## 2026-07-19 — Linux duplicate routing-profile ID checkpoint

Assumption: allowing multiple profile IDs must not turn a new-profile action into an accidental
upsert of an existing record; only explicit Edit may replace a saved ID.

- Linux rejects a new routing profile when its validated ID already exists, with a catalog-backed
  error; explicit Edit continues to update the selected record.
- l10n `712c4b1ac814ffbab265e4d0d40629d9d2bba02d` adds the duplicate-ID error to all twelve official
  packs; the deterministic bundle contains 359 messages and the Linux source audit covers 235 keys.
- Local validation passed formatting, GUI all-target check, strict Clippy, 131 demo-provider tests
  with 2 ignored, localization synchronization/key audit, Flatpak metadata, and diff checks.

This closes accidental new-profile replacement without claiming complete fallback-chain editing,
full Orca speech, manual visual review, other clients, release artifacts, or a stable release.

## 2026-07-19 — Linux routing profile identifier checkpoint

Assumption: multiple saved routing profiles require a user-provided stable identifier, while edits
must keep the persisted ID immutable so document-job and selection references remain valid.

- Linux adds a localized routing-profile ID entry, validates it with Core's 1–128 byte ASCII
  identifier rule, and allows distinct IDs for new profiles. Existing-profile edits lock the ID.
- l10n `7b832d765788e5ca64d7ba483b8ad12b3dd382d2` adds the label and invalid-ID error to all twelve
  official packs; the deterministic bundle contains 358 messages and the Linux source audit
  covers 234 keys.
- Local validation passed `cargo fmt --all -- --check`, GUI all-target `cargo check`, strict Clippy,
  131 demo-provider tests with 2 ignored, l10n synchronization, localization-key audit, Flatpak
  metadata validation, and `git diff --check` before the remote checkpoint commit.

This enables multiple Linux routing-profile IDs without claiming complete fallback-chain editing,
full Orca speech, manual visual review, other clients, release artifacts, or a stable release.

## 2026-07-19 — Linux routing profile edit checkpoint

Assumption: complete Linux routing-profile management requires loading an existing non-secret
profile back into the same editor and replacing its stable ID, while preserving any constraints
that the UI does not expose.

- Linux adds an **Edit** action to each saved routing-profile row. The editor restores the persisted
  Manual/Ordered/Automatic mode, explicit fallback consent, candidate selection/order, and stable
  profile ID; **Save routing profile** upserts that ID instead of creating a duplicate.
- Existing profile constraints are retained while the visible mode and fallback controls are
  applied. The worker regression `routing_profiles_persist_without_provider_endpoints_or_secrets`
  now verifies same-ID replacement and a single updated record.
- l10n `aea172c15f421da09a0c848accae7c443820fb27` adds the edit/save actions to all twelve official
  packs; the bundle contains 356 messages and the Linux source audit covers 232 keys.
- Local targeted checks passed; full Linux and remote gates will be recorded after the checkpoint
  commit.

This closes the saved-profile edit/upsert slice without claiming complete fallback-chain editing,
full Orca speech, manual visual review, other clients, release artifacts, or a stable release.

## 2026-07-19 — Linux routing candidate drag-order checkpoint

Assumption: Ordered routing needs both keyboard-accessible bounded moves and a direct pointer
gesture for placing a selected candidate before another; the persisted candidate list remains the
only source of truth and invalid drag payloads must fail closed.

- Linux adds GTK text drag sources and row drop targets to the routing-profile dialog. Dropping a
  candidate before another row rebuilds the visible list and preserves the resulting order used by
  profile creation; the existing localized icon labels and keyboard controls remain available.
- Core-facing helper `move_routing_profile_id_before` rejects self, unknown, and missing target IDs;
  `routing_candidate_drag_reordering_is_bounded` covers forward, reverse, self, and unknown cases.
- Local targeted test and GUI all-target check passed. Full Linux and remote CI validation will be
  recorded after the checkpoint commit.

This advances Linux candidate management without claiming complete profile editing, full Orca
speech, manual visual review, other clients, release artifacts, or a stable release.

## 2026-07-19 — Linux routing candidate accessibility-label checkpoint

Assumption: icon-only candidate movement controls must expose catalog-backed accessible names in
addition to tooltips, while full screen-reader speech and visual review remain separate manual gates.

- Linux GTK up/down controls now use `action.move_candidate_up` and `action.move_candidate_down`
  labels for both tooltips and the GTK accessible `Label` property.
- Localization revision `0d2d8c08f3dec5cd3044558b0b7c75f669a9535d` adds the two Linux-only keys to
  all twelve official packs and regenerated PO/MO resources; the source audit now covers 230 keys.
- Local Linux validation passed with GUI check, strict Clippy, 132 tests (`130 passed; 2 ignored`),
  localization sync/audit, Flatpak metadata, and diff checks. The l10n repository passed its 26 tests,
  generated-resource checks, and Foundation validation at commit `0d2d8c0`.
- Final Linux push gates passed at `0894b87`: Foundation `29698567260`, Native `29698567247`, and
  Flatpak `29698567253`; PR gates passed with Foundation `29698569197`, Native `29698569232`, and
  Flatpak `29698569229`. The preceding `3d60123` run stopped only because CI still pinned the old
  l10n revision; `0894b87` updates that workflow pin before the successful gates.

This strengthens icon-control semantics without claiming complete candidate management, Orca speech,
manual visual review, other clients, release artifacts, or a stable release.

## 2026-07-19 — Linux routing candidate-order checkpoint

Assumption: Ordered routing needs an explicit, keyboard-focusable way to change the sequence of
selected candidates; drag/drop and screen-reader copy review remain separate accessibility work.

- Candidate rows now include focusable up/down controls. The in-memory order is rebuilt before
  persistence, so the Core profile receives the exact Ordered-mode sequence the user selected.
- The bounded `move_routing_profile_id` helper rejects unknown IDs and out-of-range moves; its
  regression covers forward, reverse, boundary, and missing-candidate behavior.
- Commit `251cdbe99bb5a347a7a7d77f56ba1e35712c063f` passed local GUI compilation, strict Clippy,
  132 tests (`130 passed; 2 ignored`), localization synchronization/key audit, Flatpak metadata
  validation, and diff checks.
- Push gates passed: Foundation `29697585211`, Native Linux `29697585189`, and Flatpak Linux
  `29697585215`. PR gates passed: Foundation `29697586335`, Native Linux `29697586345`, and
  Flatpak Linux `29697586336`. The preceding `e939c0a` Native runs exposed only strict Clippy's
  `type_complexity`; the follow-up type alias is included in this validated commit.

This advances Ordered-mode candidate editing without claiming drag/drop semantics, complete candidate
management, other clients, visual/Orca review, release artifacts, or a stable release.

## 2026-07-19 — Linux routing candidate-selection checkpoint

Assumption: a routing profile must let the user restrict dispatch to explicitly approved saved
provider/model pairs; the displayed candidate order is the Ordered-mode order, while full drag/drop
editing remains a later accessibility-reviewed slice.

- The GTK routing-profile dialog now renders enabled saved provider/model pairs as focusable
  checkboxes. Only checked candidates are serialized into the Core profile, in the displayed order.
- Unknown candidate IDs are filtered before profile construction, and an empty selection is rejected
  through the existing fixed, catalog-backed no-candidate error.
- Regression `routing_candidate_selection_preserves_order_and_rejects_unknown_profiles` covers
  deterministic filtering and order preservation without exposing endpoints, credentials, or content.
- Remote push Native/Flatpak/Foundation runs `29696815328`/`29696815363`/`29696815337` and PR
  Native/Flatpak/Foundation runs `29696816705`/`29696816704`/`29696816734` all passed for this
  candidate-selection head.

This advances Linux candidate inclusion without claiming drag/drop reordering, complete candidate
management, other clients, visual/Orca review, release artifacts, or a stable release.

## 2026-07-19 — Linux routing mode and fallback-consent checkpoint

Assumption: routing mode is a user-visible Core contract, while fallback remains opt-in and must
default to disabled for newly created profiles.

- The GTK routing-profile dialog now exposes Core `Manual`, `Ordered`, and `Automatic` modes in a
  stable dropdown order and persists the selected mode with the profile.
- **Allow approved fallback** is a separate, focusable checkbox and is off by default; Core still
  rejects fallback for manual mode and document jobs.
- No new localization keys were added; existing catalog-backed routing and fallback labels are
  reused.
- Local Linux validation passed: GUI all-target check, 130 demo-provider tests (`128 passed; 2
  ignored`), strict Clippy, formatting, localization sync/audit, Flatpak metadata, and diff checks.
  Remote push Native/Flatpak/Foundation runs `29696147534`/`29696147536`/`29696147528` and PR
  Native/Flatpak/Foundation runs `29696149519`/`29696149493`/`29696149499` all passed for this
  published head.

This advances the Linux routing configuration surface without claiming complete candidate editing,
third-party-provider interoperability, other clients, visual/Orca review, release artifacts, or a
stable release.

## 2026-07-19 — Linux routed document restart checkpoint

Assumption: a routed document job must persist only its non-secret routing-profile ID so restart can
re-run deterministic candidate selection; legacy jobs without that ID continue using their saved
provider/model options. Document fallback remains disabled.

- Core `9926d0f9bf6394c6011c6cc886d142bfeb54e10f` adds schema 16 and the transactional migration
  for `document_job_options.routing_profile_id`.
- Linux stores that ID when a document starts through a saved profile. Resume and Retry now reload
  the profile after worker restart, reconnect the selected candidate through the host secret broker,
  emit a zero-fallback routing decision, and translate the remaining segments.
- Regression `document_job_resume_reconnects_saved_routing_profile_after_restart` interrupts a
  routed job, restarts the worker, and verifies complete reconstruction and no fallback.

Local Core storage/workspace validation and Linux 129-test validation passed; two existing Linux
environment-dependent tests remain ignored. Core CI/Native SDK `29694632345`/`29694632350` and
Linux push Native/Flatpak/Foundation `29695192479`/`29695192489`/`29695192477`, plus PR
Native/Flatpak/Foundation `29695193793`/`29695193826`/`29695193809`, all passed for the published
commits.

Status: Runtime storage ENOSPC rollback, forced Wayland/X11 GTK gates, baseline GTK accessibility semantics including accessible document progress, live AT-SPI tree export checks, a headless GTK keyboard traversal fixture for tested controls, runtime catalog-backed workspace/status/theme localization, the GIO Secret Service adapter, generic completion desktop notifications, bounded native text-file import with source-editor drag-and-drop, recoverable TXT/Markdown/CSV/JSON/HTML/SRT/WebVTT/DOCX/PPTX/XLSX/EPUB/PDF document-job translation with sequential segment persistence, bounded DOCX/PPTX/XLSX/EPUB package reconstruction and resource retention, bounded optional image-only PDF OCR with page-marked text output, page-aware text-PDF reconstruction with structured HTML fallback, subtitle timestamp validation, CSV quoting and selected-column reconstruction, JSON structure/path selection and escaping preservation, HTML tag-stack validation, script/style protection, and text-node reconstruction, the corrected Secret Service session wire shape, isolated real-daemon Secret Service CRUD plus persistent restart/locked lifecycle fixtures, secure persistent-credential onboarding, fail-closed Secret Service prompted-flow handling, deterministic and third-party Ollama OpenAI `/v1/` and native `/api` discovery/streaming evidence, a GTK provider preset selector for OpenAI-compatible and native Ollama profiles, bounded Linux ordinary-text dispatch through saved Core routing profiles, a remotely built pinned Flatpak bundle with bounded sandbox startup, private notification-service transport validation, headless real notification-daemon delivery, physical desktop-shell notification rendering, a real XDG document-portal lease lifecycle fixture, a real interactive portal FileChooser backend fixture, application-level GTK FileDialog callbacks, and an actual GTK source-editor drag/drop gesture fixture are implemented; source-referenced Linux gettext keys are statically checked against the canonical catalog; complete candidate-management/fallback-chain UI, Orca speech, end-user prompt acceptance, visual/translated copy review, other clients, and release artifacts remain open

Global goal SHA-256: `11f9a65927aac7e57e2af119e9d21cc98e8d5a08b8a112a19ee1c47903e36198`

## 2026-07-19 — Linux ordinary-text routing execution checkpoint

Assumption: selecting a saved routing profile applies only to ordinary text requests in this
Linux slice; document jobs and the existing explicit single-provider fallback remain separate
boundaries until their routing semantics are specified and tested.

- The worker now builds a non-secret `RoutingContext`, asks Core `routing_planner_v1` to select a
  candidate, resolves that candidate through the saved provider profile and host secret broker,
  and executes the request with the selected provider/model. A typed decision event records only
  stable identifiers and candidate counts.
- The GTK routing-profile dialog adds an explicit **Use** action. The selected profile is applied
  to the next ordinary text translation; the diagnostics panel records the safe decision summary.
- l10n `fade545ec14793893de2603c62e0994689d9c4df` contains 352 messages, including the routing
  selection and decision labels. Local l10n checks, Linux routing/model regressions, formatting,
  GUI check, strict Clippy, localization sync/audit, Flatpak metadata, and diff checks passed.
- Remote Linux push Native/Foundation/Flatpak runs `29692199045`/`29692199022`/`29692199030`
  passed; duplicate PR-triggered Native/Foundation/Flatpak runs
  `29692200873`/`29692200865`/`29692200912` also passed. l10n Foundation/Localization runs
  `29691938103`/`29691938112` passed.

This advances actual Linux ordinary-text routing execution without claiming a complete automatic or
ordered fallback chain, document-job routing, other clients, visual/Orca review, release artifacts,
or a stable release.

## 2026-07-19 — Linux text retry action checkpoint

Assumption: a failed or cancelled ordinary text request must be explicitly retryable without
creating a document job or changing the confirmed provider/model selection.

- Linux adds an accessible, catalog-backed **Retry translation** button. It delegates to the same
  real `Translate` command path and is sensitive only for failed/cancelled ordinary text requests;
  document-job retry remains a separate persisted queue action.
- `AppState::can_retry_translation` and a request-preservation model test cover the state boundary;
  the GTK acceptance flow verifies disabled-while-complete, enabled-after-failure, re-dispatch, and
  completion after retry.
- l10n `50688449ab16a8007f0edebabed2f8d6f0d3a90a` contains 336 catalog messages, including the two
  new Linux-only text-retry messages. Local l10n lint/tests/generation checks passed.
- Local Linux `cargo test --features demo-provider --offline` passed 121 tests with 2 ignored;
  GUI check, strict Clippy, formatting, l10n synchronization, 217-key audit, Flatpak metadata,
  and diff checks passed. The GUI all-target test binary could not link on this host because its
  system GTK libraries do not export the GTK 4 symbols required by the installed gtk-rs version;
  remote CI remains the authoritative GTK gate.

This advances Linux Text Workspace retry evidence without claiming retry classification parity,
automatic/ordered routing UI, other clients, visual/Orca review, release artifacts, or a stable
release.

## 2026-07-19 — Linux visible-string gettext coverage checkpoint

Assumption: compound summaries visible to users must localize their complete template rather than
concatenating an English prefix with data. Technical identifiers, filenames, model IDs, and
translation content remain data and are not translated.

- Linux now routes history and translation-memory metadata through
  `status.translation_entry_metadata`, document queue identifiers through
  `status.document_job_id`, and active-provider persistence mode through
  `provider.active_with_mode`. Missing provider/model values use the existing catalog-backed
  `status.unavailable` label.
- l10n revision `bd06a76bcd498748b520143c61964a92727d1b51` contains 339 messages and regenerated
  all 59 deterministic native resources plus both pseudo-locales. Non-English values remain
  explicit machine-generated drafts.
- Local `make check` in l10n, Linux formatting, 121 demo-provider tests with 2 ignored, GUI
  all-target check, strict Clippy, l10n synchronization, 219-key audit, Flatpak metadata, and
  diff checks passed. The host GTK all-target test-link limitation remains unchanged.

This closes the current Linux source-level compound-summary localization gap without claiming
human translated-copy review, Orca speech, automatic/ordered routing controls, other clients,
release artifacts, or a stable release.

## 2026-07-19 — Linux routing profile persistence checkpoint

Assumption: the first Linux routing slice should persist only validated planner metadata; provider
endpoints, credentials, and translation content remain outside the saved record.

- Linux now exposes a catalog-backed **Routing profiles** action. The worker saves, lists, and
  deletes Core `routing_planner_v1` profiles through the existing storage boundary and rejects
  those mutations while a translation is active.
- The dialog can create a bounded `linux-default` automatic, local-preferred profile from saved
  provider/model selections. It displays mode and candidate counts and provides an explicit delete
  action; no endpoint or secret is serialized into the routing profile.
- l10n `5f98f8bf760bb552c5d9e6cc7ace575e427bae10` contains 350 messages, including the 11
  Linux routing-profile labels and mode strings. Local l10n checks, Linux tests (122 passed, 2
  ignored), GUI check, strict Clippy, localization sync/audit, Flatpak metadata, and diff checks
  passed.

This establishes routing-profile persistence and editing only. Actual translation dispatch through
automatic or ordered routing, human copy review, other clients, release artifacts, and a stable
release remain open.

## 2026-07-19 — Core routing planner compatibility checkpoint

Assumption: Linux must reject a Core that does not expose the shared routing contract before
provider work starts; this pin records the contract while GTK routing controls remain a later
client slice.

- Core `d1c03ba84362c0c672c57045a59fc8092db470be` adds strict routing-profile constraint validation
  on top of schema 15 persistence and
  advertises `routing_planner_v1`.
- Linux startup now requires the feature alongside the existing exact alpha.2, ABI 1, protocol 1,
  and provider-catalog checks.
- Native and Flatpak source pins were updated to the same Core revision; full local and remote
  validation remains pending.

## 2026-07-19 — Linux image-only PDF OCR toolchain revalidation

Assumption: the opt-in OCR boundary is only claimable when the current Linux checkout can drive
the real `pdftoppm` and `tesseract` processes against a generated image-only PDF fixture.

- `bash tools/run-ocr-test.sh` passed locally on the current Linux head. The fixture was generated
  with ImageMagick, rendered with Poppler, and recognized with the installed English Tesseract
  language pack; the page text assertion completed successfully.
- The test keeps the OCR path opt-in and bounded. Ordinary Linux tests continue to cover malformed
  input and unavailable-tool fail-closed behavior without invoking external processes.

This revalidates Linux's optional image-only PDF OCR evidence without claiming pixel-identical PDF
reconstruction, non-English OCR quality, visual review, other clients, release artifacts, or a
stable release.

## 2026-07-19 — Linux cancelled document-job retry checkpoint

Assumption: retrying an interrupted document job must reuse its persisted provider/model options,
retain all still-pending segments, and never silently restart or overwrite completed work.

- The worker regression `cancelled_document_job_can_be_retried_without_losing_pending_segments`
  starts a bounded TXT job, cancels after a streamed partial event, verifies the cancelled snapshot
  still has two pending segments, then retries through the saved options and reconstructs both
  translated lines.
- The regression rejects worker action errors and failed segment events, so a false successful
  state cannot hide a retry failure.

This strengthens Linux Scenario 12 recovery evidence without claiming concurrent document execution,
physical interruption recovery, other clients, release artifacts, or a stable release.

## 2026-07-19 — Linux macro/signature package boundary checkpoint

Assumption: unsupported OOXML macro and digital-signature parts must be rejected before XML
inspection or worker persistence; preserving them silently would overstate the supported format.

- Core `14cee83a650610b3a9a79a460c7c6f54ae9d21d4` rejects `vbaProject.bin` and `_xmlsignatures/`
  package parts with `DocumentError::UnsupportedFormat` for DOCX, PPTX, and XLSX inspection and
  reconstruction.
- Core focused unit test passed locally. Linux's native wrapper now maps the same boundary to
  `TextImportError::UnsupportedFormat`; its focused regression passed locally with the sibling Core
  checkout at the exact pin.
- Local `cargo test --features demo-provider --offline` passed 119 tests with 2 ignored; GUI check,
  strict Clippy, formatting, l10n synchronization, 215-key audit, Flatpak metadata validation, and
  diff checks also passed. Core CI `29685742893` and Native SDK `29685742897` passed all jobs.
- Linux push Native `29686029877` (job `88190303661`), Foundation `29686029880` (job `88190303628`),
  and Flatpak `29686029899` (job `88190303740`) passed. PR Native `29686027664` (job `88190298389`),
  Foundation `29686027668` (job `88190298119`), and Flatpak `29686027665` (job `88190298083`) passed.

This advances Linux Scenario 15 and the Milestone 6 unsupported-format boundary without claiming
macro execution, digital-signature preservation, visual review, other clients, artifacts, or stable
release evidence.

## 2026-07-19 — GTK AT-SPI fixture cleanup checkpoint

Assumption: a successful AT-SPI assertion must also terminate its private GTK/D-Bus/Xvfb processes
within a bounded interval; a runner cancellation after the assertion is not valid gate evidence.

- Push Native run `29684324260` printed `GTK AT-SPI fixture passed` at `11:01:38Z`, but its cleanup
  remained active until the 30-minute job cancellation at `11:29:49Z`; the run is recorded as
  cancelled, not passed.
- `tools/run-gtk-atspi-test.sh` now bounds termination and reaping of the application, window manager,
  and AT-SPI launcher, escalating to `SIGKILL` after five seconds per process so later Wayland/build
  gates cannot be starved by a successful-but-leaking fixture.
- Local `bash -n tools/run-gtk-atspi-test.sh` and `git diff --check` passed. The full six remote gates
  must be rerun for this cleanup fix and the Linux PPTX worker checkpoint.

This records a validation failure and its bounded cleanup correction without claiming remote success.

## 2026-07-19 — Linux PPTX worker end-to-end checkpoint

Assumption: Linux Milestone 6 evidence must exercise persisted worker translation for PPTX, not only
the native import wrapper and shared Core reconstruction fixture; the package remains bounded and
contains no user paths or credentials.

- Linux adds `document_job_translation_reconstructs_pptx_and_preserves_notes_and_resources`, which
  persists a PPTX job, translates slide and speaker-note segments through the fake provider, rebuilds
  the completed package, and verifies the binary image part remains unchanged.
- Local `cargo test --features demo-provider --offline` passed 118 tests with 2 ignored; the focused
  worker fixture passed independently. Formatting, GUI check, strict Clippy, 215-key audit, l10n
  sync, Flatpak metadata validation, and diff checks passed.
- The six remote Native/Foundation/Flatpak push and PR gates are required for this published checkpoint.

This completes Linux worker evidence for bounded PPTX reconstruction without claiming macros/signatures,
visual review, other clients, packaging artifacts, or a stable release.

## 2026-07-19 — Linux PPTX import/reconstruction checkpoint

Assumption: the Linux document slice should exercise every currently supported OOXML family through
the native wrapper before claiming Milestone 6 coverage; the PPTX fixture is bounded and in memory.

- Linux adds `imports_pptx_and_preserves_notes_and_resources`, translating slide and speaker-note
  text while preserving the binary image part and reconstructing a valid PPTX package through Core.
- Local `cargo test --features demo-provider --offline` passed 117 tests with 2 ignored; the focused
  PPTX fixture passed independently. Formatting, GUI check, strict Clippy, 215-key audit, l10n sync,
  Flatpak metadata validation, and diff checks passed.
- The six remote Native/Foundation/Flatpak push and PR gates are required for this published checkpoint.

This advances Linux Milestone 6 OOXML evidence without claiming macro/signature behavior, visual review,
other clients, packaging artifacts, or a stable release.

## 2026-07-19 — Linux compression-ratio import boundary checkpoint

Assumption: mandatory Scenario 15 evidence must prove that the Linux native import wrapper maps the
reviewed Core compression-ratio rejection to its bounded user-facing error before worker persistence.

- Linux adds `rejects_docx_archive_with_suspicious_compression_ratio`, an in-memory DOCX fixture with
  a 512 KiB repetitive deflated resource. The wrapper returns `TextImportError::TooLarge`; no output
  path is opened and the source fixture remains in memory.
- Local `cargo test --features demo-provider --offline` passed 116 tests with 2 ignored; the focused
  fixture passed independently. Formatting, GUI check, strict Clippy, 215-key audit, l10n sync,
  Flatpak metadata validation, and diff checks passed.
- The six remote Native/Foundation/Flatpak push and PR gates are required for the published checkpoint.

This extends Scenario 15 integration evidence without claiming macro/signature behavior, visual review,
other clients, packaging artifacts, or a stable release.

## 2026-07-19 — Core OOXML compression-ratio pin checkpoint

Assumption: Linux should consume the reviewed Core archive guard through the same immutable
functional pin used by Native CI and Flatpak metadata; no Linux-local duplicate parser is added.

- Linux now pins Core `63fc0ca62e2b1d9bd168a60e6c9051ac338f6486`, whose shared DOCX/PPTX/XLSX
  archive boundary rejects entries at least 1 KiB whose uncompressed size exceeds 200 times the
  compressed size, in addition to the existing size, count, path, duplicate, encrypted, and
  symlink checks.
- The reviewed Linux functional pin and documentation are recorded at `be0bad02a16c046e49dfddc3152b98bf7f1d1bab`; Flatpak metadata consumes that immutable source.
- Core local workspace tests, strict Clippy, formatting, and locked build passed at this revision;
  Core CI run `29682666941` and Native SDK run `29682666929` completed successfully.
- Linux local validation passed: the worker OOXML tests are included in the 115-pass, 2-ignored
  demo-provider suite; formatting, GUI all-target check, strict Clippy, 215-key localization audit,
  l10n synchronization, Flatpak metadata validation, and diff checks also passed. Push Native
  `29682975678`, Foundation `29682975679`, and Flatpak `29682975675`, plus PR Native
  `29682976712`, Foundation `29682976695`, and Flatpak `29682976678`, all passed.

This checkpoint strengthens mandatory Scenario 15 archive safety without claiming macro/signature,
visual review, other clients, packaging artifacts, or a stable release.

## 2026-07-19 — Linux worker OOXML end-to-end checkpoint

Assumption: Linux document acceptance must exercise the persisted worker command path, not only
the native import wrapper or shared Core reconstruction tests; the fixtures use bounded in-memory
DOCX/XLSX packages and contain no user paths or credentials.

- Linux `9ed0557a87b5c042d38e05cad5abf4a2afe487f9` adds worker regressions that create persisted
  DOCX and XLSX jobs, translate their pending segments through the fake provider, reconstruct the
  completed packages, and verify translated text while preserving binary resources, formulas, and
  numeric cells.
- Local `cargo test --features demo-provider --offline` passed 115 tests with 2 ignored; the two
  new regressions passed independently before the full suite. `cargo fmt --all -- --check`,
  `cargo check --features gui --all-targets --offline`, strict all-feature Clippy, the 215-key
  localization audit, l10n synchronization, and `git diff --check` also passed.

This strengthens Linux evidence for mandatory Scenarios 10 and 11 without claiming macro/signature
coverage, visual review, other clients, release artifacts, or a stable release.

## 2026-07-19 — Linux built-in Ollama profile-name localization checkpoint

Assumption: built-in provider display names are user-visible Linux form values, so both the
OpenAI-compatible and native Ollama defaults must resolve through the canonical catalog while
user-edited names remain untouched.

- Linux now routes new-profile initialization and untouched preset switching through localized
  default-name helpers for both built-in providers. The source audit covers 215 literal keys.
- l10n `85b9d45569ce840c17dc0acc7d7366d6810be48e` contains 334 catalog messages and bundle SHA-256
  `028d25b3637fbc19d41d497a860b414353615b9576db6f852a9f236bcbe770ce`.

The updated Linux revision still requires its full remote gates. Machine-generated translations,
visual/RTL review, Orca speech review, and stable-release qualification remain open.

## 2026-07-19 — Linux GTK fixture localization checkpoint

Assumption: the automated GTK drag-and-drop fixture button is still user-visible UI and must
resolve through the canonical catalog, even though it is only enabled for interaction-test runs.

- Linux now resolves the drag-fixture button through `fixture.drag_file`; the source-level audit
  covers 214 literal Linux localization keys.
- l10n `3aa86232974f9a9ece8d3a45e6760dee294fca81` contains 333 catalog messages and bundle SHA-256
  `61a054d99935b256e79d5be7feb4d929fc8cf61af663a02b8fd10475745d70bd`.

The updated Linux revision still requires its full local and remote gates. Machine-generated
translations, visual/RTL review, Orca speech review, and stable-release qualification remain open.

## 2026-07-19 — Linux corrupt-database fail-closed checkpoint

Assumption: a corrupted local SQLite file must not be repaired or overwritten implicitly; the
client should report persistence failure, preserve the bytes for recovery, and keep session-only
translation available.

- Added a worker regression with a private, malformed SQLite file. Startup emits typed
  `Persistence` storage-unavailable evidence, the demo provider remains available, a session-only
  translation completes, and saved-profile deletion remains rejected.
- The test verifies the malformed database bytes are unchanged after shutdown.

Validated locally:

- `cargo fmt --all -- --check` — passed.
- `cargo check --features gui --all-targets --offline` — passed.
- `cargo clippy --all-targets --all-features --offline -- -D warnings` — passed.
- `cargo test --features demo-provider --offline` — passed: 108 tests, 2 ignored, 0 failed.
- `python3 tools/check-localization-keys.py` — passed: 213 Linux source keys.
- `bash tools/sync-l10n.sh --check` and `git diff --check` — passed.

Physical database corruption recovery, desktop accessibility review, other clients, and stable
release evidence remain open.

## 2026-07-19 — Linux output-safety alias checkpoint

Assumption: rejecting only byte-for-byte equal destination URIs is insufficient for Scenario 18,
because a save target may be a symbolic link or hard link to the imported source file.

- Linux now compares GIO file identity, canonical native paths, and Unix device/inode metadata
  before both text and binary export writes. Source aliases are rejected before asynchronous
  replacement begins, preserving the source on failure and cancellation paths.
- Added a regression covering the exact source path, a distinct target, and Unix symbolic/hard-link
  aliases. No source contents are written by the guard.

Validated locally:

- `cargo fmt --all` — passed.
- `cargo check --features gui --all-targets --offline` — passed.
- `cargo clippy --all-targets --all-features --offline -- -D warnings` — passed.
- `cargo test --features demo-provider --offline` — passed: 107 tests, 2 ignored, 0 failed.
- `python3 tools/check-localization-keys.py` — passed: 213 Linux source keys.
- `bash tools/sync-l10n.sh --check` and `git diff --check` — passed.

The GTK binary test cannot link on this host because installed GTK/libadwaita symbols are older
than the gtk-rs headers; Native and Flatpak CI remain required for the GUI regression. Physical
desktop review, Orca speech, other clients, and stable-release evidence remain open.

## 2026-07-19 — Linux plural UI wiring checkpoint

Assumption: pluralized catalog support must be exercised by a visible GTK surface, not only by
catalog unit tests; persisted document jobs represent one selected source file per job.

- The document-jobs dialog now announces its localized file count through the runtime plural API,
  while retaining the empty-list state and per-job metadata.
- Connected-provider model placeholders now resolve through the canonical catalog instead of
  inserting an untranslated literal during model discovery.

Validated locally:

- `cargo fmt --all -- --check` — passed.
- `cargo check --features gui --all-targets --offline` — passed.

The full Linux test suite and remote gates are required for this checkpoint; actual visual/Orca
review, physical offline conditions, other clients, and stable-release evidence remain open.

## 2026-07-19 — Linux offline-provider preservation checkpoint

Assumption: offline behavior must fail within a bounded user-visible interval while preserving the
last confirmed provider, model, and request path; the test uses a just-released loopback port so it
does not depend on external network availability or a live service.

- Added a worker regression that connects a confirmed fake provider, attempts a session-only
  connection to the released loopback port, requires a `Network` rejection in under five seconds,
  and then completes a translation through the previously confirmed provider.
- This extends Linux evidence for mandatory Scenario 17 (offline behavior) without claiming a
  physical network outage or third-party provider interoperability.

Validated locally:

- `cargo fmt --all -- --check` — passed.
- `cargo test --features demo-provider offline_provider_failure_is_prompt_and_keeps_confirmed_session --offline` — passed.

The full Linux test suite, native/Flatpak CI, actual offline network conditions, Orca speech,
visual review, other clients, and stable-release evidence remain open.

## 2026-07-19 — Linux gettext plural runtime checkpoint

Assumption: the pinned gettext catalogs are the runtime source of truth for plural selection, so
the GTK client must preserve every generated translation slot and apply the locale-specific rule
before replacing `{count}` or other non-sensitive placeholders.

- The MO loader now retains all NUL-separated plural translations instead of discarding every slot
  after the first. `text_plural` selects the correct slot for English, French, Russian, Arabic,
  Hindi, Brazilian Portuguese, and the one-form Chinese/Japanese/Korean catalogs, with safe
  fallback behavior for incomplete translations.
- Regression coverage exercises English singular/plural, Simplified Chinese one-form behavior,
  Russian three-form selection, and Arabic dual-form selection using the pinned Linux catalogs.

Validated locally:

- `cargo fmt --all -- --check` — passed.
- `cargo check --features gui --all-targets --offline` — passed.
- `cargo clippy --all-targets --all-features --offline -- -D warnings` — passed.
- `cargo test --features demo-provider --offline` — passed: 106 tests, 2 ignored, 0 failed.
- `python3 tools/check-localization-keys.py` — passed: 213 Linux source keys.
- `bash tools/sync-l10n.sh --check` and `git diff --check` — passed.

The catalog translations remain machine-generated or source-reviewed according to their existing
metadata; visual locale/RTL review, Orca speech, other clients, and release artifacts remain open.

## 2026-07-19 — Native Ollama `/api` GTK preset checkpoint

Assumption: the verified native Ollama worker path is ready for explicit Linux user selection, while
the independently installed daemon remains an external interoperability gate.

The GTK provider form now exposes localized OpenAI-compatible and native Ollama presets. Selecting
the native preset uses the stable `ollama`/`ollama_chat` profile pair, changes only untouched default
name and endpoint fields, and shows the native `/api` tooltip. Saved profiles restore their preset;
connecting through the form therefore exercises native `/api/tags` discovery and `/api/chat` NDJSON
streaming without a credential. The regression also checks Simplified Chinese labels and accessible
label-to-control relations. The native test remains fixture-backed and does not claim a third-party
daemon, GPU, Orca, visual review, or stable release.

Validated locally:

- `cargo check --features gui --all-targets --offline` and `cargo check --features gui --tests --offline` — passed.
- `cargo test --features demo-provider --offline` — passed: 105 tests, 2 ignored, 0 failed.
- `bash tools/sync-l10n.sh --check` — passed at l10n revision `d3d838198027e2104583296eb3e0f6fadc283e4e` (332 messages; bundle SHA-256 `0650b68a49daf27b56c95ae149cd5c29621d890ba4c7554c7c79d5690e38a05b`).

The local GTK binary test remains unavailable on this host because installed GTK/libadwaita symbols
are older than the gtk-rs headers; the CI native and Flatpak gates are required evidence.

## 2026-07-19 — Native Ollama `/api` worker checkpoint

Assumption: Linux-first local-model support needs the native Ollama `/api` contract in addition to
the already covered OpenAI-compatible `/v1/` surface; a running third-party daemon remains an
external runtime gate.

The worker creates an explicit `ollama_chat` profile for the catalog's loopback-only `ollama`
preset. It discovers `llama3.2:latest` through `/api/tags`, requires deliberate model selection,
and streams `你好，Ollama！` through `/api/chat` NDJSON without a secret. Core owns endpoint policy,
bounded response parsing, cancellation, protected-span restoration, and completion validation;
Linux supplies the profile and exercises the real worker path against the deterministic fixture.

Validated locally:

- `cargo test --features demo-provider --lib --offline` — passed: 105 tests, 2 ignored, 0 failed.
- Core workspace format, check, Clippy, and all-feature offline tests — passed at the matching Core
  worktree revision.

The fixture proves the native wire contract and does not claim a third-party daemon, GPU, Orca,
visual review, or stable release.

## 2026-07-19 — Ollama-compatible local endpoint checkpoint

Assumption: Ollama's OpenAI-compatible `/v1/` surface is the bounded Linux-first local-model
contract for this checkpoint. The native Ollama `/api` protocol and a running third-party daemon
remain outside the automated gate.

The Linux worker now exercises a deterministic fixture that returns `llama3.2:latest` from
`/v1/models`, requires deliberate model selection, and streams `你好，Ollama！` from
`/v1/chat/completions` without a credential. The test uses the existing `local-loopback` preset
and keeps Android, Windows, and macOS deferred.

The Core pin is `0d0d475d22129e8211333ee8f664a7669948ce3a`. Push validation passed in Native Linux
run `29673591852` (job `88156804870`) and Flatpak Linux run `29673591888` (job `88156804894`); the
pull-request gates also passed in Native Linux run `29673593375` (job `88156808424`) and Flatpak
Linux run `29673593421` (job `88156808624`).

The final evidence-only revision `091676d7f1f053e4a005acddbc162116c39b5407` repeated the same
checks successfully: push Native Linux `29673745733` (job `88157199361`), push Flatpak Linux
`29673745743` (job `88157199354`), pull-request Native Linux `29673747672` (job `88157204034`),
and pull-request Flatpak Linux `29673747662` (job `88157203962`).

Assumption: canonical generated PO/MO resources are synchronized and format-validated. The GTK host
now parses all twelve pinned official Linux MO catalogs at runtime, exposes BCP 47 locale choices,
switches the root direction for Arabic, and preserves the source editor buffer during a locale
switch; status summaries, partial-output markers, text-file import controls, provider-profile
controls, source/target language options, and stable worker/runtime/storage error sentences now use
the same catalogs; source-referenced key coverage is enforced by a static audit, while plural
handling and visual locale/RTL review remain open.

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
  `0d0d475d22129e8211333ee8f664a7669948ce3a`.
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
- Fourteen canonical official/pseudo PO/MO catalog pairs containing 327 messages pinned to l10n revision
  `f00b00fda307660000b0e4068c5ca1072d266df1`. Sync rejects a different revision, dirty generated
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
  `f00b00fda307660000b0e4068c5ca1072d266df1`. The revised native gate retains serialized all-target,
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
- Other third-party local-server variants beyond the verified Ollama daemon; the deterministic
  Ollama-compatible OpenAI `/v1/` loopback contract and native `/api` daemon path are covered.
- Complete canonical UI gettext coverage, plural/placeholder handling, and visual locale/RTL verification.
- Runtime database faults beyond the verified private-tmpfs `ENOSPC`, read-only-directory, and
  corrupt-database boundaries, including power loss and broader SQLite VFS failures.
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
- Synchronized Linux PO/MO resources to l10n `3f3c1a1154b66d25f2936a02b8a08d2a8fc8a878`.
- Local `cargo fmt --all --check`, all-target/all-feature `cargo check`, strict Clippy, locked
  no-default tests (64 passed, 1 ignored), demo-provider tests (102 passed, 2 ignored), OCR
  fixture, l10n sync check, shell syntax, and `git diff --check` passed.
- Native Linux run `29668533941` (job `88143262465`), Repository Foundation run `29668533939`,
  and Flatpak Linux run `29668533922` (job `88143262421`) passed. Native exercised the new OCR
  fixture after installing ImageMagick, Poppler, and Tesseract; Flatpak continued to pass its
  sandbox smoke without enabling OCR by default.

## 2026-07-19 — Linux canonical localization-key audit checkpoint

Assumption: every literal key passed to the Linux UI localization helpers must be present in the
canonical l10n catalog; dynamic keys remain intentionally outside this static check and are covered
by the existing runtime localization tests.

- Added `tools/check-localization-keys.py`, a dependency-free source audit for literal keys in
  `src/main.rs` and `src/model.rs`. It reads the sibling canonical catalog and reports missing keys
  with a non-zero exit status.
- Added the audit to Native localization validation and the Foundation required-file manifest; the
  documented command uses `python3 -B` so validation does not leave bytecode artifacts.
- Local `python3 -B tools/check-localization-keys.py`, l10n sync check, shell syntax, and diff checks
  passed. The audit covered 187 unique Linux source keys against the pinned 306-message catalog.

The audit makes source-to-catalog coverage reproducible but does not replace translated-copy,
plural, visual locale/RTL, or Orca speech review. Native, Foundation, and Flatpak CI gates remain
required for the pushed revision.

## 2026-07-19 — Linux accessible document-progress checkpoint

Assumption: persisted document-job progress is user-visible state and must be exposed through a
native GTK progress-bar role, not only a textual status label. Completed and total counts remain
bounded by the Core document-job contract.

- Added a GTK `ProgressBar` beside the document status. It exposes localized completed/total text,
  a clamped fraction, and the `ProgressBar` accessibility role; it is hidden when no document job
  is selected and no longer duplicates the partial-output label.
- Extended the GTK regression test to assert the role, hidden initial state, 2/4 fraction and
  localized text, then reset to hidden state. Existing AT-SPI and keyboard fixtures remain
  unchanged and still cover live semantic export and focus traversal.
- Local `cargo fmt --all --check`, all-target/all-feature `cargo check`, strict Clippy, locked
  no-default tests (64 passed, 1 ignored), and demo-provider tests (102 passed, 2 ignored) passed.
  The native GTK binary test remains CI-linked on this host because installed GTK symbols cannot
  link the test binary; CI is required to execute the new widget assertions.
- The first pushed revision `ba12919` failed Native run `29669906878` only because the test asserted
  the fallback English wording instead of the canonical catalog's `2 of 4 segments translated`;
  the follow-up assertion now derives the expected text through the same catalog helper.
- Follow-up revision `c5d0308` passed Native run `29669977294` (job `88147085571`), Foundation
  run `29669977297`, and Flatpak run `29669977295` (job `88147085574`). The pull-request reruns
  `29669978352`, `29669978350`, and `29669978371` also passed, including the GTK progress-role,
  bounded-fraction, localized-text, and hidden-reset assertions.

Orca speech, manual high-contrast/RTL/reduced-motion review, end-user Secret Service prompt
approval, other clients, and release artifacts remain open.

## 2026-07-19 — Linux localized diagnostics checkpoint

Assumption: the non-sensitive diagnostics panel is user-visible UI and its compatibility summary
must follow runtime locale changes without exposing source text, endpoints, or secret references.

- Routed the diagnostics summary through the canonical `diagnostics.summary` catalog template,
  including the Core ABI and protocol versions, while retaining the existing redacted state fields.
- Added official-locale coverage for the summary and a regression that verifies Simplified Chinese
  rendering and source-content exclusion.
- Local formatting, locked all-target checks, strict Clippy, no-default tests (65 passed, 1 ignored),
  demo-provider tests (103 passed, 2 ignored), localization-key audit (188 keys), l10n sync, shell
  syntax, and diff checks passed. The native GTK binary test remains CI-linked because this host's
  installed GTK symbols cannot link it.
- Linux revision `cf9c2d8` passed Native push run `29670504285` (job `88148480525`), Foundation
  push run `29670505177` (job `88148482823`), and Flatpak push run `29670505111`
  (job `88148482618`). PR reruns Native `29670505097`, Foundation `29670504255`, and Flatpak
  `29670504265` also passed.

Complete visible-string gettext coverage, translated-copy/plural review, Orca speech, manual
high-contrast/RTL/reduced-motion review, end-user Secret Service prompt approval, other clients,
and release artifacts remain open.

## 2026-07-19 — Linux diagnostics-label localization checkpoint

Assumption: the non-sensitive diagnostics panel is Linux-visible UI, so fixed field labels,
boolean values, onboarding/status/theme/locale values, and profile-storage states must resolve
through the canonical catalog while provider identifiers, paths, endpoints, and output content
remain excluded.

- Linux routes all fixed diagnostics labels and state values through the catalog, including the
  20 new Linux-only diagnostics keys. The l10n bundle now has 326 messages, checksum
  `054d6749397cbbf652e099784f2c7d0e3650779a3c17c98e68d25560d286b2d3`, and is pinned at
  `32bef261f5f0deb9f6a0426231e365d0bae72b62`; non-English values remain explicitly unreviewed
  drafts.
- Local `cargo fmt --all --check`, locked all-target checks, strict Clippy, no-default tests,
  demo-provider tests, the 208-key localization audit, l10n sync, shell syntax, and diff checks
  passed. The native GTK binary test remains CI-linked because this host's installed GTK symbols
  cannot link it.
- l10n Foundation run `29671276786` and Localization run `29671276797` passed for the pinned
  catalog revision. Linux revision `355481d937b3722e509dbd05cc1575c4e71be143` passed push Native
  `29671444706` (job `88151076586`), Foundation `29671444731` (job `88151076725`), and Flatpak
  `29671444733` (job `88151076695`); PR reruns Native `29671445475` (job `88151078773`),
  Foundation `29671445499` (job `88151078854`), and Flatpak `29671445495` (job `88151078857`)
  also passed.

Complete visible-string gettext coverage beyond the diagnostics slice, translated-copy/plural
review, Orca speech, manual high-contrast/RTL/reduced-motion review, end-user Secret Service prompt
approval, other clients, and release artifacts remain open.

## 2026-07-19 — Linux document-pause error localization checkpoint

Assumption: a document-pause command rejected by the bounded worker queue is user-visible UI and
must use the same catalog-backed error rendering as other worker failures.

- Queue-send failures from the GTK Pause action now enter the reducer's client-error path instead
  of writing raw English directly into the error label. The existing
  `error.worker.command_queue_unavailable` catalog mapping therefore applies consistently.
- Local `cargo fmt --all --check`, locked all-target checks, strict Clippy, no-default tests (65
  passed, 1 ignored), and demo-provider tests (103 passed, 2 ignored) passed. The native GTK binary
  test remains CI-linked because this host's installed GTK symbols cannot link it.

Linux revision `1d96c9825b83cdc1cd6a2783b61fdd678b89e510` passed push Native `29672046465`
(job `88152770602`), Foundation `29672046491` (job `88152770643`), and Flatpak `29672046488`
(job `88152770610`). PR reruns Native `29672047299` (job `88152772830`), Foundation
`29672047295` (job `88152772869`), and Flatpak `29672047296` (job `88152772851`) also passed.

Complete visible-string gettext coverage beyond this error path, translated-copy/plural review,
Orca speech, manual high-contrast/RTL/reduced-motion review, end-user Secret Service prompt
approval, other clients, and release artifacts remain open.

## 2026-07-19 — Linux Secret Service prompt approval checkpoint

Assumption: Secret Service `CreateItem` and `Delete` prompt paths represent an explicit user
security decision. The client must wait for `Completed`, accept only an approved prompt, map a
dismissal to localized storage guidance, and fail closed on prompt-call or timeout failures.

- Implemented `org.freedesktop.Secret.Prompt.Prompt` plus `Completed` signal handling with a
  bounded five-minute wait. Approved prompts now complete store/delete operations; dismissed
  prompts return the catalog-backed `error.storage.prompt_dismissed` message.
- Extended the isolated D-Bus fixture to cover accepted and dismissed store/delete flows. The
  prompted-flow script passed all four ignored integration tests locally.
- Pinned l10n revision `f00b00fda307660000b0e4068c5ca1072d266df1`, containing 327 messages and
  bundle checksum `53821e2397e6697b7551693c6f5787cc1f88e24d96b3077ac590645a848f1977`.
- Local `cargo fmt --all --check`, locked all-target checks, strict Clippy, no-default tests
  (65 passed, 1 ignored), demo-provider tests (103 passed, 2 ignored), 208-key localization
  audit, l10n sync, prompt fixture, shell syntax, and diff checks passed.

The first CI attempt stopped at the expected localization checkout because the workflow still
referenced the previous l10n revision. After updating that pin, push Native `29672741665`
(job `88154536172`), Foundation `29672741666` (job `88154536162`), and Flatpak `29672741675`
(job `88154536212`) passed. Pull-request reruns Native `29672743058` (job `88154539551`),
Foundation `29672742959` (job `88154539322`), and Flatpak `29672742990` (job `88154539432`)
also passed, including both prompted-flow cases. Manual Secret Service approval UX, broader
storage-fault coverage, translated-copy review, Orca speech, other clients, and release
artifacts remain open.
