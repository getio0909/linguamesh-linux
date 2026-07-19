#!/usr/bin/env bash
set -euo pipefail

temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/linguamesh-ocr-fixture.XXXXXX")"
trap 'rm -rf "$temp_dir"' EXIT

# 生成只包含文字图像的 PDF，避免把可提取文字误认为 OCR 输入。
convert \
  -size 1200x300 \
  xc:white \
  -fill black \
  -font DejaVu-Sans \
  -pointsize 72 \
  -gravity center \
  -annotate +0+0 'LinguaMesh OCR fixture' \
  "$temp_dir/page.png"
convert "$temp_dir/page.png" "$temp_dir/input.pdf"
chmod 600 "$temp_dir/input.pdf"

LINGUAMESH_OCR_FIXTURE="$temp_dir/input.pdf" \
  cargo test --no-default-features --locked --lib \
  ocr::tests::recognizes_the_external_fixture -- \
  --ignored --exact --nocapture

printf '%s\n' 'OCR fixture passed.'
