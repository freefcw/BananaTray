#!/usr/bin/env python3
"""One-time text migration for BananaTray custom provider YAML files.

This script intentionally uses only the Python standard library. It migrates the
legacy top-level custom provider schema to schema_version 2:

    availability/source/parser/preprocess -> plan.steps[0]

It is conservative and designed for BananaTray's existing YAML examples and user
files. Review the result after running with --write.
"""

from __future__ import annotations

import argparse
from pathlib import Path

TOP_LEVEL_KEYS = {
    "id",
    "schema_version",
    "base_url",
    "metadata",
    "availability",
    "source",
    "parser",
    "preprocess",
    "plan",
}

MOVABLE_KEYS = {"availability", "source", "parser", "preprocess"}


def top_level_key(line: str) -> str | None:
    if not line or line[0].isspace() or ":" not in line:
        return None
    key = line.split(":", 1)[0].strip()
    return key if key in TOP_LEVEL_KEYS else None


def split_blocks(lines: list[str]) -> list[tuple[str | None, list[str]]]:
    blocks: list[tuple[str | None, list[str]]] = []
    current_key: str | None = None
    current: list[str] = []

    for line in lines:
        key = top_level_key(line)
        if key is not None:
            if current:
                blocks.append((current_key, current))
            current_key = key
            current = [line]
        else:
            current.append(line)

    if current:
        blocks.append((current_key, current))
    return blocks


def indent_block(block: list[str], spaces: int) -> list[str]:
    """Indent non-blank lines by `spaces` spaces. Blank lines are left unchanged."""
    prefix = " " * spaces
    return [prefix + line if line.strip() else line for line in block]


def migrate_source_block(block: list[str]) -> list[str]:
    """Rename http_get/http_post to unified http + method field.

    Only these two source types need renaming; cli and placeholder are unchanged.
    """
    migrated: list[str] = []
    for line in block:
        stripped = line.strip()
        if stripped == "type: http_get":
            migrated.append(line.replace("type: http_get", "type: http"))
            migrated.append(line[: len(line) - len(line.lstrip())] + 'method: get\n')
        elif stripped == "type: http_post":
            migrated.append(line.replace("type: http_post", "type: http"))
            migrated.append(line[: len(line) - len(line.lstrip())] + 'method: post\n')
        else:
            migrated.append(line)
    return migrated


def migrate_text(text: str) -> tuple[str, bool]:
    lines = text.splitlines(keepends=True)
    if any(line.strip() == "schema_version: 2" for line in lines) and any(
        line.startswith("plan:") for line in lines
    ):
        return text, False

    blocks = split_blocks(lines)
    movable: dict[str, list[str]] = {}
    output_blocks: list[tuple[str | None, list[str]]] = []
    inserted_schema_version = False

    for key, block in blocks:
        if key in MOVABLE_KEYS:
            movable[key] = block
            continue
        output_blocks.append((key, block))
        if key == "id":
            output_blocks.append(("schema_version", ["schema_version: 2\n"]))
            inserted_schema_version = True

    if "source" not in movable:
        return text, False

    if not inserted_schema_version:
        output_blocks.insert(0, ("schema_version", ["schema_version: 2\n"]))

    step_lines = [
        "plan:\n",
        "  mode: first_success\n",
        "  steps:\n",
        '    - name: "default"\n',
        "      required: true\n",
    ]

    for key in ("availability", "source", "preprocess", "parser"):
        block = movable.get(key)
        if not block:
            continue
        if key == "source":
            block = migrate_source_block(block)
        step_lines.extend(indent_block(block, 6))

    output_blocks.append(("plan", step_lines))
    return "".join(line for _, block in output_blocks for line in block), True


def iter_yaml_files(paths: list[Path]) -> list[Path]:
    files: list[Path] = []
    for path in paths:
        if path.is_dir():
            files.extend(sorted(path.glob("*.yaml")))
            files.extend(sorted(path.glob("*.yml")))
        else:
            files.append(path)
    return files


def migrate_file(path: Path, *, write: bool, backup: bool) -> bool:
    text = path.read_text(encoding="utf-8")
    migrated, changed = migrate_text(text)
    if not changed:
        return False

    if write:
        if backup:
            path.with_suffix(path.suffix + ".bak").write_text(text, encoding="utf-8")
        path.write_text(migrated, encoding="utf-8")
    else:
        print(f"--- {path}")
        print(migrated, end="" if migrated.endswith("\n") else "\n")
    return True


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Migrate BananaTray legacy custom provider YAML to schema_version 2.",
    )
    parser.add_argument("paths", nargs="+", type=Path, help="YAML files or directories")
    parser.add_argument("--write", action="store_true", help="write migrated YAML in place")
    parser.add_argument(
        "--no-backup",
        action="store_true",
        help="do not create .bak files when using --write",
    )
    args = parser.parse_args()

    changed = 0
    for path in iter_yaml_files(args.paths):
        if migrate_file(path, write=args.write, backup=not args.no_backup):
            changed += 1
    print(f"migrated {changed} file(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
