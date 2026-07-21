#!/usr/bin/env python3
"""为 Linux Flatpak CI bundle 生成可复核的校验和与 SPDX SBOM。"""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import re
import tomllib


def parse_args() -> argparse.Namespace:
    """解析证据生成所需的固定输入。"""
    parser = argparse.ArgumentParser(description="Create checksum and SPDX evidence for a Flatpak bundle.")
    parser.add_argument("--bundle", type=pathlib.Path, required=True)
    parser.add_argument("--manifest", type=pathlib.Path, required=True)
    parser.add_argument("--cargo-lock", type=pathlib.Path, required=True)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    return parser.parse_args()


def package_id(name: str, version: str, index: int) -> str:
    """生成稳定且符合 SPDX 标识符规则的包 ID。"""
    safe_name = re.sub(r"[^A-Za-z0-9.-]+", "-", name).strip("-") or "package"
    safe_version = re.sub(r"[^A-Za-z0-9.-]+", "-", version).strip("-") or "version"
    return f"SPDXRef-Package-{index}-{safe_name}-{safe_version}"


def validate_revision(value: str, label: str) -> str:
    """验证 Flatpak 来源固定为完整的小写 SHA。"""
    if not re.fullmatch(r"[0-9a-f]{40}", value):
        raise SystemExit(f"{label} must be a lowercase 40-character commit SHA.")
    return value


def write_rollback_record(output_dir: pathlib.Path, linux_revision: str, core_revision: str) -> None:
    """写出不冒充稳定发布的回滚操作记录。"""
    content = f"""# Linux Flatpak CI evidence rollback record

Artifact status: unsigned Flatpak prerelease evidence; not a stable release.
Linux revision: {linux_revision}
Core revision: {core_revision}

If a future signed release built from this revision is promoted, roll back by:

1. Stop distributing the current release and retain its checksum, signature, and incident record.
2. Restore the previous release-manifest component and its previously verified Flatpak bundle.
3. Verify the restored bundle checksum and signature before distribution resumes.
4. Re-run the compatibility, security, and packaging gates, then record the rollback decision.

This CI evidence contains no previous stable revision, signing key, or release authorization.
"""
    (output_dir / "ROLLBACK.md").write_text(content, encoding="utf-8")


def main() -> int:
    """写出 Flatpak bundle 的 SHA-256 与 SPDX 2.3 文档。"""
    args = parse_args()
    bundle = args.bundle.resolve()
    manifest_path = args.manifest.resolve()
    lock_path = args.cargo_lock.resolve()
    output_dir = args.output_dir.resolve()
    if not bundle.is_file() or bundle.stat().st_size == 0:
        raise SystemExit(f"Flatpak evidence input is missing or empty: {bundle}")

    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    lock = tomllib.loads(lock_path.read_text(encoding="utf-8"))
    app_id = manifest.get("app-id")
    if not isinstance(app_id, str) or not app_id:
        raise SystemExit("Flatpak evidence manifest has no app-id.")
    source_module = next(
        module for module in manifest.get("modules", []) if module.get("name") == "linguamesh"
    )
    git_sources = [
        source
        for source in source_module.get("sources", [])
        if isinstance(source, dict) and source.get("type") == "git"
    ]
    linux_revision = validate_revision(
        next(source["commit"] for source in git_sources if source.get("dest") == "linguamesh-linux"),
        "Linux revision",
    )
    core_revision = validate_revision(
        next(source["commit"] for source in git_sources if source.get("dest") == "linguamesh-core"),
        "Core revision",
    )
    packages = [
        {
            "SPDXID": "SPDXRef-Package-LinguaMesh-Linux",
            "name": "linguamesh-linux",
            "versionInfo": "CI checkout",
            "downloadLocation": "NOASSERTION",
            "filesAnalyzed": False,
        }
    ]
    for index, package in enumerate(
        sorted(lock.get("package", []), key=lambda value: (value["name"], value["version"])),
        start=1,
    ):
        packages.append(
            {
                "SPDXID": package_id(package["name"], package["version"], index),
                "name": package["name"],
                "versionInfo": package["version"],
                "downloadLocation": "NOASSERTION",
                "filesAnalyzed": False,
            }
        )
    digest = hashlib.sha256(bundle.read_bytes()).hexdigest()
    output_dir.mkdir(parents=True, exist_ok=True)
    (output_dir / "SHA256SUMS").write_text(f"{digest}  {bundle.name}\n", encoding="utf-8")
    document = {
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": f"{app_id} Flatpak CI evidence",
        "documentNamespace": f"https://github.com/getio0909/linguamesh-linux/flatpak/{digest}",
        "creationInfo": {
            "created": "1970-01-01T00:00:00Z",
            "creators": ["Tool: create-flatpak-evidence.py"],
        },
        "packages": packages,
        "relationships": [
            {
                "spdxElementId": "SPDXRef-DOCUMENT",
                "relationshipType": "DESCRIBES",
                "relatedSpdxElement": "SPDXRef-Package-LinguaMesh-Linux",
            }
        ],
    }
    (output_dir / "SBOM.spdx.json").write_text(
        json.dumps(document, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    write_rollback_record(output_dir, linux_revision, core_revision)
    print(f"Flatpak evidence created: {output_dir} ({len(packages)} SPDX packages).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
