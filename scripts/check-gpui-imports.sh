#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

# 覆盖私有/公开、空格、grouped 和跨行形式：
# `use gpui::*`、`pub use gpui :: *`、`use gpui::{App, *}`。
pattern='(?m)^[[:space:]]*(?:pub(?:[[:space:]]*\([^)]*\))?[[:space:]]+)?use[[:space:]]+gpui[[:space:]]*::[[:space:]]*(?:(?:[[:alpha:]_][[:alnum:]_]*[[:space:]]*::[[:space:]]*)*\*|\{[^;]*\*)[^;]*;'

# 优先使用 rg，回退到 Perl（grep 无法可靠匹配跨行 grouped import）。
if command -v rg >/dev/null 2>&1; then
  # --multiline 覆盖 `use gpui::{App,\n *};`。
  matches=$(rg -n --multiline --type rust "$pattern" src --no-heading || true)
else
  matches=$(
    find src -type f -name '*.rs' -print0 |
      xargs -0 perl -0777 -ne '
        while (/^[[:space:]]*(?:pub(?:[[:space:]]*\([^)]*\))?[[:space:]]+)?use[[:space:]]+gpui[[:space:]]*::[[:space:]]*(?:(?:[[:alpha:]_][[:alnum:]_]*[[:space:]]*::[[:space:]]*)*\*|\{[^;]*\*)[^;]*;/mg) {
          $prefix = substr($_, 0, $-[0]);
          $line = 1 + ($prefix =~ tr/\n//);
          $matched = $&;
          $matched =~ s/\n/ /g;
          print "$ARGV:$line:$matched\n";
        }
      ' || true
  )
fi

if [ -n "$matches" ]; then
  echo "$matches"
  echo
  echo "error: forbidden GPUI glob import found in src/"
  echo "use explicit gpui imports instead"
  exit 1
fi

echo "GPUI glob import check passed"
