#!/usr/bin/env python3
"""为 Linux 原生 CI 二进制生成可复核的校验和与 SPDX SBOM。"""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import re
import tomllib


def parse_args() -> argparse.Namespace:
    """解析原生证据生成所需的固定输入。"""
    parser = argparse.ArgumentParser(description="Create checksum and SPDX evidence for a native Linux binary.")
    parser.add_argument("--binary", type=pathlib.Path, required=True)
    parser.add_argument("--cargo-lock", type=pathlib.Path, required=True)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--linux-revision", required=True)
    parser.add_argument("--core-revision", required=True)
    parser.add_argument("--localization-revision", required=True)
    return parser.parse_args()


def package_id(name: str, version: str, index: int) -> str:
    """生成稳定且符合 SPDX 标识符规则的包 ID。"""
    safe_name = re.sub(r"[^A-Za-z0-9.-]+", "-", name).strip("-") or "package"
    safe_version = re.sub(r"[^A-Za-z0-9.-]+", "-", version).strip("-") or "version"
    return f"SPDXRef-Package-{index}-{safe_name}-{safe_version}"


def validate_revision(value: str, label: str) -> str:
    """验证证据中使用的提交固定为完整的小写 SHA。"""
    if not re.fullmatch(r"[0-9a-f]{40}", value):
        raise SystemExit(f"{label} must be a lowercase 40-character commit SHA.")
    return value


def write_rollback_record(
    output_dir: pathlib.Path,
    linux_revision: str,
    core_revision: str,
    localization_revision: str,
) -> None:
    """写出不冒充稳定发布的回滚操作记录。"""
    content = f"""# Linux native CI evidence rollback record

Artifact status: unsigned release-mode prerelease evidence; not a stable release.
Linux revision: {linux_revision}
Core revision: {core_revision}
Localization revision: {localization_revision}

If a future signed release built from this revision is promoted, roll back by:

1. Stop distributing the current release and retain its checksum, signature, and incident record.
2. Restore the previous release-manifest component and its previously verified artifact.
3. Verify the restored artifact checksum and signature before distribution resumes.
4. Re-run the compatibility, security, and packaging gates, then record the rollback decision.

This CI evidence contains no previous stable revision, signing key, or release authorization.
"""
    (output_dir / "ROLLBACK.md").write_text(content, encoding="utf-8")


def main() -> int:
    """写出 Linux 原生二进制的 SHA-256 与 SPDX 2.3 证据。"""
    args = parse_args()
    linux_revision = validate_revision(args.linux_revision, "Linux revision")
    core_revision = validate_revision(args.core_revision, "Core revision")
    localization_revision = validate_revision(args.localization_revision, "Localization revision")
    binary = args.binary.resolve()
    lock_path = args.cargo_lock.resolve()
    output_dir = args.output_dir.resolve()
    if not binary.is_file() or binary.stat().st_size == 0:
        raise SystemExit(f"Native evidence input is missing or empty: {binary}")

    lock = tomllib.loads(lock_path.read_text(encoding="utf-8"))
    packages = [
        {
            "SPDXID": "SPDXRef-Package-LinguaMesh-Linux",
            "name": "linguamesh-linux",
            "versionInfo": "CI release-mode checkout",
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

    digest = hashlib.sha256(binary.read_bytes()).hexdigest()
    output_dir.mkdir(parents=True, exist_ok=True)
    (output_dir / "SHA256SUMS").write_text(f"{digest}  {binary.name}\n", encoding="utf-8")
    document = {
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": "LinguaMesh Linux native CI evidence",
        "documentNamespace": f"https://github.com/getio0909/linguamesh-linux/native/{digest}",
        "creationInfo": {
            "created": "1970-01-01T00:00:00Z",
            "creators": ["Tool: create-native-evidence.py"],
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
    write_rollback_record(output_dir, linux_revision, core_revision, localization_revision)
    print(f"Native evidence created: {output_dir} ({len(packages)} SPDX packages).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
