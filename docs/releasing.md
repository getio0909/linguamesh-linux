# Releasing

## Current state

An unreleased native application target and a pinned Flatpak packaging scaffold now exist. The
GNOME 49 SDK workflow builds a prerelease CI bundle from pinned sources and runs the bounded
Xvfb/private-D-Bus sandbox smoke. The native gate verifies the real document-portal lease lifecycle,
the real interactive `xdg-desktop-portal-gtk` FileChooser backend, the application-level GTK
FileDialog callback, a real source-editor drag/drop gesture, and visible desktop-shell delivery to a
real `dunst` notification daemon under Xvfb, but no physical compositor/GPU rendering, signed artifact, or
distributable release has been verified. The
vertical slice must not be tagged or published as a product release, and no packaging claim beyond
the recorded CI build is valid. Its bundled fake provider is development-only behavior. The optional
OpenAI-compatible endpoint form accepts a one-shot session credential, clears the field
immediately, and never persists the credential value. A saved-profile dropdown and explicit remember
checkbox can create, update, activate, switch, and remove multiple rows containing provider names,
endpoints, model preferences, and persistent Secret Service references in the XDG user data SQLite database. The
private application directory is `0700` and the database is `0600`. Removing the connected row
leaves its already validated runtime session active but no longer persistent. Core's no-follow
SQLite open behavior on Linux's default Unix VFS remains required. Startup prefills the last
persistently activated row but never auto-connects, so a credential must be entered again when
required. A derived Provider setup card guides configuration, explicit connection, and deliberate
model selection without storing a completion flag, distinguishes worker failure from startup, and
shows the confirmed next-request stable ID/model identity.
The routing-profile dialog now persists an explicit Core mode (`Manual`, `Ordered`, or `Automatic`);
approved fallback remains a separate opt-in checkbox that defaults off, and candidate checkboxes limit
the saved provider/model pairs included in a profile. This is configuration-surface evidence only and
does not satisfy the complete candidate-management release gate.
The external-provider path includes deterministic Ollama-compatible OpenAI `/v1/` and native
`/api/` loopback fixtures; they do not claim interoperability with a running third-party daemon.
The GTK form now exposes the native Ollama preset and its `ollama_chat` adapter, while persistent
secret references use the Linux GIO Secret Service adapter and fail closed when the desktop keyring
is unavailable or requires an interactive prompt. The native workflow
pins reviewed Core functional revision
`9926d0f9bf6394c6011c6cc886d142bfeb54e10f`, whose storage delta adds
`SQLITE_OPEN_NOFOLLOW`, rejects suspicious OOXML compression ratios and unsupported macro/signature
parts before XML inspection, and whose
text path adds protected-span, request-level glossary, and bounded long-text restoration, and the
bounded SRT/WebVTT/CSV/JSON/HTML document contract, rather than checking
out a floating branch. Functional revision
`7d7eba9960b657f0460fb0daaaaebaaa609f39b1` passed Native Linux run `29604269568` (job
`87963611054`) and repository-foundation run `29604269516`; it includes the no-credential
OpenAI-compatible loopback regression, secure onboarding fixtures, strict Clippy, both display
gates, and the all-feature build. Earlier functional revision
`9729b23ce1a4280ebb434339e880010103b4859d` passed Native Linux run `29580444723` (job
`87884607879`) with 65 library tests and the first real GTK flow. Wayland-gate revision
`10b31a040fd3c44ecbaef31eb5c66c0c8e5cb620` passed Native Linux run `29582513061` (job
`87891382469`) with the same GTK binary test succeeding under both X11/Xvfb and forced
Wayland/headless Weston. Neither validation creates a distributable artifact or satisfies the
future release gate below.

The current Linux gate consumes Core `9926d0f9bf6394c6011c6cc886d142bfeb54e10f` and l10n
`85b9d45569ce840c17dc0acc7d7366d6810be48e` (334 catalog messages, bundle SHA-256
`028d25b3637fbc19d41d497a860b414353615b9576db6f852a9f236bcbe770ce`). Request-level glossary rules, bounded CSV,
interchange are implemented in the Linux slice; persistent glossary libraries and TBX import
remain outside the release claim.

The Linux document slice now persists bounded TXT/Markdown/CSV/JSON/HTML/SRT/WebVTT/DOCX/PPTX/XLSX/EPUB/PDF jobs, translates pending prose segments
sequentially, and restores completed or cancelled snapshots into the editor. Validated non-secret
provider/model/glossary options are persisted and reused by Resume and Retry after restart only when
the active runtime matches. This is not yet a release-ready multi-job queue: archive workflows and
the document-job execution path still require the native Linux validation gate below. DOCX/PPTX/XLSX/EPUB export preserves
non-text package resources and rewrites only supported OOXML text parts; malformed, encrypted, traversal,
DTD-bearing, or over-limit packages are rejected. EPUB also requires a first `mimetype` entry,
preserves navigation and resources, translates XHTML/HTML text, and updates OPF language metadata.
Text PDFs retain page association and available coordinates; reliable ASCII streams are rewritten in
place, while unsupported encodings use a page-aware HTML alternative. Image-only PDF OCR is an
optional Linux development capability: it requires separately installed `pdftoppm` and `tesseract`,
is explicitly enabled by the user, and produces page-marked TXT rather than a reconstructed PDF.
It is not a pixel-identical output claim and must not be enabled by packaging defaults.
The Linux UI surfaces Core's bounded page-level warnings for those limitations and uncertain reading
order without treating them as OCR or pixel-fidelity guarantees.
Subtitle imports retain cue IDs and timestamps and surface configurable line-length and reading-speed
warnings; timing is never rewritten automatically.

The current native gate also includes a real post-startup `ENOSPC` regression for persistent model
updates, profile deletion, and provider switching. Runtime-storage functional revision
`c37702c76c3b1a2f9cec805cf9e219721ef7b5ce` passed Native Linux run `29586532049` (job
`87904787338`) and repository-foundation run `29586531915` (job `87904787120`). Ubuntu exercised
the controlled mount fallback and proved exact rejection, continued use of the prior session,
session-only post-fault model selection, and restart recovery of only pre-fault state. This is not
evidence for corruption, power-loss recovery, read-only media, or every storage-failure path.

## Future release gate

A Linux release may be prepared only after:

1. pinned Rust, GTK, GLib/GIO, and packaging toolchains build and test successfully on documented environments;
2. LinguaMesh Core, protocol, provider catalog, persistence, and localization versions match the central release manifest;
3. real desktop Secret Service CRUD/cleanup, the session-only fallback, complete SecretRef-backed profile lifecycle, multi-profile management, bounded native text import (including source-editor drag-and-drop), XDG paths, document-portal leases and interactive file workflows, accessibility, Wayland, practical X11 support, desktop notification delivery, migrations, and packaging smoke tests are verified;
4. dependency and LGPL compliance review, third-party notices, privacy/security review, changelog, checksums, source archive, and rollback information are complete;
5. protected release infrastructure produces reproducible artifacts without exposing credentials.

Flatpak is the primary intended packaging format. Additional AppImage, DEB, or RPM artifacts require documented reproducibility and dependency handling. Never promote a prerelease to stable until the central release train records compatible tested versions. Do not imply distribution endorsement or platform support without executed evidence.
