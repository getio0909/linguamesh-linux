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

The no-default suite contains 29 reducer tests. It covers the disconnected initial state, restoring
a saved profile without activating it, rejecting a saved session reference, pending and active
canonical profiles, exact stale-result rejection, atomic rollback, deliberate model selection,
credential-free saved-profile commits, session switches that preserve restart state, saved-model
restoration only when available, ordered events, partial output, all Core alpha.2 error categories,
and diagnostics that omit content, endpoints, model IDs, and secret references.

The `demo-provider` suite contains 50 tests in total. Its worker tests validate the exact Core
compatibility contract, prove that fake-service readiness does not auto-connect, require explicit
Connect and model selection, exercise real loopback HTTP/SSE streaming, consume an authenticated
session secret through the bounded typed host-secret broker, and fail closed for unavailable
session or persistent secrets. Persistence coverage saves and restores a non-secret profile and
model across worker restarts, proves restart performs no provider request, reconnects after explicit
credential re-entry, scans the SQLite side files for credential and `session:` canaries, verifies
exact `0700`/`0600` permissions, rejects a permissive parent, symbolic ancestor, and hard-linked
database without following static unsafe paths, preserves restart state across session switches,
failed persistent changes, and public connection cancellation, and keeps session mode usable after
storage initialization fails. The suite also
covers immediate connection cancellation, translation cancellation with partial output, active,
queued, and full-command-queue shutdown, translation terminal delivery during shutdown,
saved-model behavior, and failed-switch rollback to the previous Core `ProviderManager` and model.

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
restored profile event and verifies the GTK form prefills its name and endpoint, enables the
remember checkbox, remains disconnected, leaves the model list unselected, and is not overwritten
by fake-provider readiness. A private D-Bus session and Xvfb provide the runtime environment; tests
are serialized because GTK owns process-global state. This is not comprehensive UI automation,
accessibility, or Wayland coverage.

The GitHub Actions native workflow pins Core revision
`fbf3e9b5927049dccaa19f8c36013495ffebba12`, installs the headers plus D-Bus/Xvfb support, and runs
the complete gate. Native CI for the current non-secret persistence and restart revision is pending.
A local host without `gtk4.pc` or `libadwaita-1.pc` must not claim that the GUI build, launch, or GTK
button test passed.

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
