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
    return parser.parse_args()


def package_id(name: str, version: str, index: int) -> str:
    """生成稳定且符合 SPDX 标识符规则的包 ID。"""
    safe_name = re.sub(r"[^A-Za-z0-9.-]+", "-", name).strip("-") or "package"
    safe_version = re.sub(r"[^A-Za-z0-9.-]+", "-", version).strip("-") or "version"
    return f"SPDXRef-Package-{index}-{safe_name}-{safe_version}"


def main() -> int:
    """写出 Linux 原生二进制的 SHA-256 与 SPDX 2.3 证据。"""
    args = parse_args()
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
    print(f"Native evidence created: {output_dir} ({len(packages)} SPDX packages).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
