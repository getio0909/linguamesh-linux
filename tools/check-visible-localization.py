#!/usr/bin/env python3
"""检查 Linux GTK 可见控件是否绕过本地化目录直接使用文字。"""

from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SOURCE_FILES = (ROOT / "src/main.rs", ROOT / "src/model.rs")

# 这些调用的非空字符串参数会直接出现在 GTK 控件或文件对话框中。
VISIBLE_CALL_PATTERN = re.compile(
    r"(?:"
    r"gtk::(?:Button|CheckButton)::with_(?:label|mnemonic)"
    r"|gtk::Label::new\(\s*Some"
    r"|\.set_(?:label|title|tooltip_text|placeholder_text)"
    r"|\.accept_label"
    r"|\.cancel_label"
    r")\s*\(\s*(?:Some\(\s*)?&?\s*\"([^\"\\]*(?:\\.[^\"\\]*)*)\""
)

# 下拉列表或字符串模型中的裸字符串同样会成为用户可见选项。
LIST_LITERAL_PATTERN = re.compile(
    r"(?:gtk::DropDown::from_strings|gtk::StringList::new)"
    r"\(\s*&\s*\[\s*(?:&\s*)?\"([^\"\\]*(?:\\.[^\"\\]*)*)\""
)


def source_line(source: str, offset: int) -> int:
    """返回匹配所在的 1-based 源码行号。"""
    return source.count("\n", 0, offset) + 1


def main() -> int:
    """执行可见控件硬编码文本审计。"""
    violations: list[tuple[Path, int, str]] = []
    calls = 0
    for path in SOURCE_FILES:
        try:
            source = path.read_text(encoding="utf-8")
        except OSError as error:
            print(f"Visible localization audit could not read {path}: {error}", file=sys.stderr)
            return 2
        for match in VISIBLE_CALL_PATTERN.finditer(source):
            calls += 1
            value = match.group(1)
            if value:
                violations.append((path, source_line(source, match.start()), value))
        for match in LIST_LITERAL_PATTERN.finditer(source):
            calls += 1
            value = match.group(1)
            if value:
                violations.append((path, source_line(source, match.start()), value))

    if violations:
        print("Visible localization audit found hard-coded GTK strings:", file=sys.stderr)
        for path, line, value in violations:
            print(f"- {path.relative_to(ROOT)}:{line}: {value}", file=sys.stderr)
        return 1
    print(
        "Visible localization audit passed: no non-empty hard-coded GTK strings "
        f"({calls} empty/reset call sites inspected)."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
