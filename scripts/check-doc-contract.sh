#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

require_text() {
  file=$1
  expected=$2
  if ! rg --fixed-strings --quiet "$expected" "$root/$file"; then
    echo "missing documentation contract in $file: $expected" >&2
    exit 1
  fi
}

require_text README.md "docs/USER_GUIDE.md"
require_text README.md "docs/PROJECT_SCHEMA.md"
require_text README.md "docs/EASYEDA_HANDOFF.md"
require_text docs/USER_GUIDE.md "板体"
require_text docs/USER_GUIDE.md '稳定源对象 + 显式 `ProductionMapping`'
require_text docs/USER_GUIDE.md "Tauri 顶部“导出 EDA”按钮会选择目录"
require_text docs/PROJECT_SCHEMA.md "topSolderMaskOpen"
require_text docs/PROJECT_SCHEMA.md "背面对象不会在 schema 中预先水平镜像"
require_text docs/PROJECT_SCHEMA.md 'schema 版本为 `2`'
require_text docs/PROJECT_SCHEMA.md '`boardFill`'
require_text docs/PROJECT_SCHEMA.md "首版基材固定为 FR-4"
require_text docs/PROJECT_SCHEMA.md "没有各自实现命令语义"
require_text docs/EASYEDA_HANDOFF.md "单向下游出口"
require_text docs/EASYEDA_HANDOFF.md "不会回写"
require_text docs/EASYEDA_HANDOFF.md 'CLI 已提供 `export-easyeda`'
require_text docs/EASYEDA_HANDOFF.md "Tauri 顶部“导出 EDA”按钮已接入"

echo "documentation contract check passed"
