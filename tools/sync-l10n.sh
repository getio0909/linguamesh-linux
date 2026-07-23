#!/usr/bin/env bash
set -euo pipefail

# 将消费端固定到已通过本地化 CI 的不可变提交。
expected_revision="d8d9084cdf0448039ad0aa7612e8725c6c875036"
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
l10n_root="${LINGUAMESH_L10N_DIR:-$(dirname "$project_root")/linguamesh-l10n}"
source_root="$l10n_root/generated/linux"
destination_root="$project_root/l10n/linux"
mode="${1:---check}"

if [[ "$mode" != "--check" && "$mode" != "--write" ]]; then
    printf '%s\n' 'Usage: tools/sync-l10n.sh [--check|--write]' >&2
    exit 2
fi
if [[ ! -d "$source_root" || ! -f "$l10n_root/generated/manifest.json" || ! -f "$l10n_root/compatibility.json" ]]; then
    printf '%s\n' 'Localization sync failed: generated localization inputs are unavailable.' >&2
    exit 1
fi
if ! git -C "$l10n_root" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    printf '%s\n' 'Localization sync failed: localization source is not a Git worktree.' >&2
    exit 1
fi
actual_revision="$(git -C "$l10n_root" rev-parse HEAD)"
if [[ "$actual_revision" != "$expected_revision" ]]; then
    printf 'Localization sync failed: expected revision %s, found %s.\n' \
        "$expected_revision" "$actual_revision" >&2
    exit 1
fi
if [[ -n "$(git -C "$l10n_root" status --porcelain -- generated compatibility.json)" ]]; then
    printf '%s\n' 'Localization sync failed: pinned localization artifacts have uncommitted changes.' >&2
    exit 1
fi

copy_or_check() {
    local source_file="$1"
    local destination_file="$2"
    if [[ "$mode" == "--write" ]]; then
        install -D -m 0644 "$source_file" "$destination_file"
    elif ! cmp -s "$source_file" "$destination_file"; then
        printf 'Localization sync failed: stale or missing file: %s\n' "$destination_file" >&2
        exit 1
    fi
}

for catalog_name in linguamesh.po linguamesh.mo; do
    mapfile -t source_files < <(find "$source_root" -mindepth 3 -maxdepth 3 -type f -name "$catalog_name" | sort)
    if [[ "${#source_files[@]}" -ne 14 ]]; then
        printf 'Localization sync failed: expected 14 %s catalogs, found %s.\n' "$catalog_name" "${#source_files[@]}" >&2
        exit 1
    fi
    for source_file in "${source_files[@]}"; do
        locale="$(basename "$(dirname "$(dirname "$source_file")")")"
        if [[ ! "$locale" =~ ^[A-Za-z0-9-]+$ ]]; then
            printf 'Localization sync failed: unsafe locale identifier: %s\n' "$locale" >&2
            exit 1
        fi
        copy_or_check "$source_file" "$destination_root/$locale/LC_MESSAGES/$catalog_name"
    done
done

copy_or_check "$l10n_root/generated/manifest.json" "$project_root/l10n/manifest.json"
copy_or_check "$l10n_root/compatibility.json" "$project_root/l10n/compatibility.json"

# 拒绝遗留目录，确保消费端集合与规范生成集合完全一致。
if [[ -d "$destination_root" ]]; then
    mapfile -t destination_files < <(find "$destination_root" -mindepth 3 -maxdepth 3 -type f -name linguamesh.po | sort)
    if [[ "${#destination_files[@]}" -ne 14 ]]; then
        printf 'Localization sync failed: destination contains an unexpected PO catalog set.\n' >&2
        exit 1
    fi
    mapfile -t destination_files < <(find "$destination_root" -mindepth 3 -maxdepth 3 -type f -name linguamesh.mo | sort)
    if [[ "${#destination_files[@]}" -ne 14 ]]; then
        printf 'Localization sync failed: destination contains an unexpected MO catalog set.\n' >&2
        exit 1
    fi
fi

printf 'Localization resources are synchronized at revision %s.\n' "$expected_revision"
