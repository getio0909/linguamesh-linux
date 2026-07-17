# Testing and Validation

## Host prerequisites

Rust 1.93.0 is pinned by `rust-toolchain.toml`. A sibling `../linguamesh-core` checkout is required
because the client deliberately uses typed path dependencies instead of copying shared behavior.
Its functional source must match approved revision
`fbf3e9b5927049dccaa19f8c36013495ffebba12`. This revision changes file-backed Core storage to add
SQLite's `SQLITE_OPEN_NOFOLLOW` flag and includes its Unix symlink-rejection regression test. On
Linux's default Unix VFS, any symbolic-link path component is rejected. A clean documentation-only
descendant is acceptable
for local path builds when the compiled source tree is unchanged; validate it with:

```sh
git -C ../linguamesh-core cat-file -e fbf3e9b5927049dccaa19f8c36013495ffebba12^{commit}
git -C ../linguamesh-core diff --quiet \
  fbf3e9b5927049dccaa19f8c36013495ffebba12..HEAD -- \
  Cargo.toml Cargo.lock rust-toolchain.toml rustfmt.toml crates assets migrations
test -z "$(git -C ../linguamesh-core status --porcelain)"
```

A sibling `../linguamesh-l10n` checkout at the revision pinned by `tools/sync-l10n.sh` is required
to verify the checked-in PO catalogs.

Validate localization provenance and gettext syntax with:

```sh
bash tools/sync-l10n.sh --check
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
```

The no-default suite contains 38 reducer tests. It covers the disconnected initial state, atomic
sorted restoration of multiple profiles without activation, duplicate/missing/default/session-ref
snapshot rejection, form-only selection, exact pending deletion, connected-row removal that keeps
the runtime session, pending and active canonical profiles, exact stale-result rejection, atomic
rollback, deliberate model selection, per-ID credential-free upserts, session switches that
preserve every restart row/default, active-ID model updates while another row is displayed,
saved-model restoration only when available, ordered events, partial output, all Core alpha.2 error
categories, and diagnostics that omit content, endpoints, IDs, model IDs, and secret references.

The `demo-provider` suite contains 62 tests in total. Its worker tests validate the exact Core
compatibility contract, prove that fake-service readiness does not auto-connect, require explicit
Connect and model selection, exercise real loopback HTTP/SSE streaming, consume an authenticated
session secret through the bounded typed host-secret broker, and fail closed for unavailable
session or persistent secrets. Persistence coverage creates two profiles with independent models,
restores the full list and active ID without provider requests, reconnects after explicit credential
re-entry, proves two credential values remain isolated, scans SQLite side files for both credential
and `session:` canaries, deletes inactive/missing/connected rows, keeps a deleted connected runtime
usable without recreating it, verifies exact `0700`/`0600` permissions, rejects a permissive parent,
symbolic ancestor, and hard-linked database without following static unsafe paths, preserves every
restart row/default across session switches, failed persistent changes, and public connection
cancellation, and keeps session mode usable after storage initialization fails. The suite also
covers immediate connection cancellation, translation cancellation with partial output, active,
queued, and full-command-queue shutdown, translation terminal delivery during shutdown,
delete rejection during translation, saved-model behavior, and failed-switch rollback to the
previous Core `ProviderManager` and model.

The GTK Rust source can be checked without native linking as a limited diagnostic:

```sh
DOCS_RS=1 cargo check --all-targets --all-features --locked
DOCS_RS=1 cargo clippy --all-targets --all-features --locked -- -D warnings
```

These commands bypass sys-crate discovery and do not validate headers, ABI, linking, launch, or
display behavior. Their cached sys-crate output can also make a later ordinary Cargo check look
successful. The `pkg-config` commands below are therefore mandatory native-gate prerequisites;
run `cargo clean -p graphene-sys` before rechecking native discovery after a source-only check.
Do not extend this shortcut to `cargo test --no-run`: the GTK binary test still requires native
linking.

## Native GUI validation

On Debian or Ubuntu, install the native development headers:

```sh
sudo apt-get install libgtk-4-dev libadwaita-1-dev dbus-daemon gettext pkg-config xauth xvfb
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
cargo build --all-targets --all-features --locked
```

Run the development slice with:

```sh
cargo run --features gui
```

The all-feature binary test creates real GTK/libadwaita widgets, verifies the initial disconnected
state, waits for fake-endpoint readiness without auto-connect, clears a session credential from the
form immediately after Connect, explicitly selects a discovered model, preserves the active
provider/model after a failed switch, and completes a streamed translation. It also injects a
two-profile startup snapshot, verifies persisted-active prefill without activation, browses another
row without changing the runtime/default, preserves a persistent secret reference so the real
Connect path fails closed, rejects a disabled saved row without re-enabling it, checks
delete-pending control blocking, applies an exact deletion result, and verifies a fresh random
draft ID. A private D-Bus session and Xvfb provide the runtime environment; tests are serialized
because GTK owns process-global state. This is not comprehensive UI automation, accessibility, or
Wayland coverage.

The GitHub Actions native workflow pins Core revision
`fbf3e9b5927049dccaa19f8c36013495ffebba12`, installs the headers plus D-Bus/Xvfb support, and runs
the complete gate. Functional multi-profile revision
`c88d37a5de2f03c2ae5d2940c4d25e5d998c301d` passed Native Linux run `29577918335` (job
`87876528763`): strict all-feature Clippy, 62 library tests, the real GTK binary test, and the
all-target all-feature build all succeeded. Repository-foundation run `29577918346` also passed.
The preceding functional persistence revision
`c58a54c2479045773358bd9c456b45a958e98e1e` passed Native Linux run `29574265570` (job
`87865028892`): strict all-feature Clippy, 50 library tests, the real GTK binary test, and the
all-target all-feature build all succeeded. Repository-foundation run `29574265553` also passed. A
local host without `gtk4.pc` or `libadwaita-1.pc` must not substitute this remote result for an
unexecuted local GUI build, launch, or GTK test.

## Repository foundation check

```sh
set -euo pipefail
required_files="README.md LICENSE AGENTS.md REPOSITORY_ROLE.md GLOBAL_GOAL.md SECURITY.md CONTRIBUTING.md CODE_OF_CONDUCT.md THIRD_PARTY_NOTICES.md IMPLEMENTATION_STATUS.md Cargo.toml Cargo.lock rust-toolchain.toml rustfmt.toml src/lib.rs src/model.rs src/worker.rs src/main.rs docs/architecture.md docs/testing.md docs/releasing.md tools/sync-l10n.sh l10n/compatibility.json l10n/manifest.json .gitignore .github/workflows/foundation.yml .github/workflows/native.yml"
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

## Unimplemented validation

Broader GTK component/UI automation, accessibility inspection, Wayland/X11 smoke tests, a native
Secret Service backend and secure-credential persistence tests, broader XDG and portal tests,
third-party local-server interoperability, Flatpak smoke tests, runtime localization behavior,
runtime database I/O fault injection after successful startup, dependency/license automation, and
release builds remain required before a supported release.
