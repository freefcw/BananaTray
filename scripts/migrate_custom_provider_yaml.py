#!/usr/bin/env python3
"""Safely migrate legacy BananaTray custom provider YAML to schema v2.

The script intentionally uses only the Python standard library. It understands
the legacy top-level fields used by BananaTray and refuses to guess when a file
contains duplicate, unknown, or mixed legacy/v2 structures.
"""

from __future__ import annotations

import argparse
from collections import Counter
import os
from pathlib import Path
import re
import shutil
import stat
import sys
import tempfile


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
TOP_LEVEL_KEY_PATTERN = re.compile(
    r"^(?:\"(?P<double>[^\"]+)\"|'(?P<single>[^']+)'|(?P<plain>[A-Za-z_][A-Za-z0-9_-]*))\s*:"
)
MAPPING_ENTRY_PATTERN = re.compile(
    r"^(?P<indent> *)(?:\"(?P<double>[^\"]+)\"|'(?P<single>[^']+)'|"
    r"(?P<plain>[A-Za-z_][A-Za-z0-9_-]*))\s*:(?P<value>.*)$"
)
SCHEMA_VERSION_PATTERN = re.compile(r"^schema_version\s*:\s*(?P<version>\d+)\s*(?:#.*)?$")
SIMPLE_SCALAR_PATTERN = re.compile(
    r"^\s*(?P<quote>['\"]?)(?P<value>[A-Za-z_][A-Za-z0-9_-]*)(?P=quote)"
    r"\s*(?:#.*)?$"
)
LEGACY_HTTP_TYPE_PATTERN = re.compile(
    r"^(?P<indent>\s*)type\s*:\s*(?P<quote>['\"]?)"
    r"(?P<kind>http_get|http_post)(?P=quote)"
    r"(?:(?P<comment_space>[ \t]+)(?P<comment>#.*))?$"
)
LEGACY_HTTP_TYPE_TOKEN_PATTERN = re.compile(r"\bhttp_(?:get|post)\b")


class MigrationError(ValueError):
    """The input cannot be migrated without guessing."""


def top_level_key(line: str) -> str | None:
    """Return a top-level mapping key or reject unsupported top-level syntax."""
    if not line or line[0].isspace():
        return None

    stripped = line.strip()
    if not stripped or stripped.startswith("#") or stripped in {"---", "..."}:
        return None

    match = TOP_LEVEL_KEY_PATTERN.match(line)
    if match is None:
        raise MigrationError(f"unsupported top-level YAML syntax: {stripped!r}")
    return next(value for value in match.groupdict().values() if value is not None)


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


def validate_top_level_keys(blocks: list[tuple[str | None, list[str]]]) -> None:
    keys = [key for key, _ in blocks if key is not None]
    duplicates = sorted(key for key, count in Counter(keys).items() if count > 1)
    if duplicates:
        raise MigrationError(f"duplicate top-level key(s): {', '.join(duplicates)}")

    unknown = sorted(set(keys) - TOP_LEVEL_KEYS)
    if unknown:
        raise MigrationError(f"unknown top-level key(s): {', '.join(unknown)}")


def schema_version(block: list[str]) -> int:
    first_line = block[0].rstrip("\r\n")
    match = SCHEMA_VERSION_PATTERN.fullmatch(first_line)
    if match is None:
        raise MigrationError("schema_version must be an unquoted integer")
    return int(match.group("version"))


def line_ending(line: str) -> str:
    for ending in ("\r\n", "\n", "\r"):
        if line.endswith(ending):
            return ending
    return ""


def preferred_newline(text: str) -> str:
    for line in text.splitlines(keepends=True):
        if ending := line_ending(line):
            return ending
    return "\n"


def ensure_block_terminated(block: list[str], newline: str) -> list[str]:
    if not block or block[-1].endswith(("\n", "\r")):
        return block
    return [*block[:-1], block[-1] + newline]


def replace_schema_version(block: list[str], default_newline: str) -> list[str]:
    newline = line_ending(block[0]) or default_newline
    return [f"schema_version: 2{newline}", *block[1:]]


def indent_block(block: list[str], spaces: int) -> list[str]:
    """Indent non-blank lines by `spaces` spaces. Blank lines stay blank."""
    prefix = " " * spaces
    return [prefix + line if line.strip() else line for line in block]


def line_content(line: str) -> str:
    return line.rstrip("\r\n")


def mapping_entry(line: str) -> tuple[int, str, str] | None:
    match = MAPPING_ENTRY_PATTERN.fullmatch(line_content(line))
    if match is None:
        return None
    key = next(
        value
        for value in (
            match.group("double"),
            match.group("single"),
            match.group("plain"),
        )
        if value is not None
    )
    return len(match.group("indent")), key, match.group("value")


def indentation(line: str, context: str) -> int:
    content = line_content(line)
    prefix = content[: len(content) - len(content.lstrip(" \t"))]
    if "\t" in prefix:
        raise MigrationError(f"{context} uses unsupported tab indentation")
    return len(prefix)


def direct_mapping_entries(
    block: list[str], block_name: str
) -> tuple[int, list[tuple[int, str, str]]]:
    """Read only the direct fields of a conservative block-style mapping."""
    header = mapping_entry(block[0])
    if header is None or header[0] != 0 or header[1] != block_name:
        raise MigrationError(f"cannot determine top-level {block_name} mapping")
    if header[2].strip() and not header[2].lstrip().startswith("#"):
        raise MigrationError(f"top-level {block_name} must use block mapping syntax")

    structural_lines: list[tuple[int, int, str]] = []
    for index, line in enumerate(block[1:], start=1):
        content = line_content(line)
        stripped = content.strip()
        if not stripped or stripped.startswith("#"):
            continue
        structural_lines.append((index, indentation(line, block_name), content))

    if not structural_lines:
        raise MigrationError(f"top-level {block_name} mapping is empty")

    direct_indent = min(indent for _, indent, _ in structural_lines)
    if direct_indent == 0:
        raise MigrationError(f"cannot determine direct {block_name} fields")

    entries: list[tuple[int, str, str]] = []
    for index, indent, _ in structural_lines:
        if indent != direct_indent:
            continue
        entry = mapping_entry(block[index])
        if entry is None:
            raise MigrationError(f"cannot determine direct {block_name} fields")
        entries.append((index, entry[1], entry[2]))

    duplicates = sorted(
        key for key, count in Counter(key for _, key, _ in entries).items() if count > 1
    )
    if duplicates:
        fields = ", ".join(f"{block_name}.{key}" for key in duplicates)
        raise MigrationError(f"duplicate direct field(s): {fields}")
    return direct_indent, entries


def source_type(block: list[str]) -> tuple[int, str]:
    _, entries = direct_mapping_entries(block, "source")
    candidates = [(index, value) for index, key, value in entries if key == "type"]
    if len(candidates) != 1:
        raise MigrationError("source.type must be one unambiguous direct field")

    index, value = candidates[0]
    match = SIMPLE_SCALAR_PATTERN.fullmatch(value)
    if match is None:
        raise MigrationError("source.type must be a simple scalar")
    return index, match.group("value")


def migrate_source_block(block: list[str], default_newline: str) -> list[str]:
    """Rename http_get/http_post to unified http plus an explicit method."""
    type_index, source_kind = source_type(block)
    if source_kind not in {"http_get", "http_post"}:
        return block

    _, entries = direct_mapping_entries(block, "source")
    if any(key == "method" for _, key, _ in entries):
        raise MigrationError("legacy source.type cannot be combined with source.method")

    line = block[type_index]
    newline = line_ending(line)
    content = line[: -len(newline)] if newline else line
    match = LEGACY_HTTP_TYPE_PATTERN.fullmatch(content)
    if match is None:
        stripped = content.strip()
        if LEGACY_HTTP_TYPE_TOKEN_PATTERN.search(stripped):
            raise MigrationError(f"unsupported legacy source.type syntax: {stripped!r}")
        raise MigrationError("cannot determine legacy source.type")

    line_indent = match.group("indent")
    method = "get" if source_kind == "http_get" else "post"
    comment = match.group("comment")
    comment_suffix = f" {comment}" if comment else ""
    output_newline = newline or default_newline
    replacement = [
        f"{line_indent}type: http{comment_suffix}{output_newline}",
        f"{line_indent}method: {method}{output_newline}",
    ]
    migrated = [*block[:type_index], *replacement, *block[type_index + 1 :]]
    return migrated


def validate_plan_block(block: list[str]) -> None:
    direct_indent, entries = direct_mapping_entries(block, "plan")
    steps = [(index, value) for index, key, value in entries if key == "steps"]
    if len(steps) != 1:
        raise MigrationError("plan.steps must be one non-empty block sequence")

    steps_index, value = steps[0]
    if value.strip() and not value.lstrip().startswith("#"):
        raise MigrationError("plan.steps must be a non-empty block sequence")

    step_lines: list[tuple[int, str]] = []
    for line in block[steps_index + 1 :]:
        content = line_content(line)
        stripped = content.strip()
        if not stripped or stripped.startswith("#"):
            continue
        line_indent = indentation(line, "plan.steps")
        if line_indent <= direct_indent:
            break
        step_lines.append((line_indent, stripped))

    if not step_lines:
        raise MigrationError("plan.steps must be a non-empty block sequence")

    item_indent = min(indent for indent, _ in step_lines)
    direct_items = [text for indent, text in step_lines if indent == item_indent]
    if not direct_items or any(
        not (text == "-" or text.startswith("- ")) for text in direct_items
    ):
        raise MigrationError("plan.steps must be a non-empty block sequence")


def migrate_text(text: str) -> tuple[str, bool]:
    newline = preferred_newline(text)
    blocks = split_blocks(text.splitlines(keepends=True))
    validate_top_level_keys(blocks)
    by_key = {key: block for key, block in blocks if key is not None}
    movable_keys = MOVABLE_KEYS.intersection(by_key)

    version = (
        schema_version(by_key["schema_version"])
        if "schema_version" in by_key
        else None
    )
    if version is not None and version not in {1, 2}:
        raise MigrationError(f"unsupported legacy schema_version: {version}")

    missing_required = {"id", "metadata"} - by_key.keys()
    if missing_required:
        raise MigrationError(
            f"provider is missing required top-level key(s): {', '.join(sorted(missing_required))}"
        )

    if "plan" in by_key:
        if movable_keys:
            fields = ", ".join(sorted(movable_keys))
            raise MigrationError(
                f"plan already exists alongside legacy field(s): {fields}"
            )
        if version != 2:
            raise MigrationError("existing plan requires exactly schema_version: 2")
        validate_plan_block(by_key["plan"])
        return text, False

    if "source" not in by_key:
        if movable_keys:
            fields = ", ".join(sorted(movable_keys))
            raise MigrationError(f"legacy field(s) require a top-level source: {fields}")
        raise MigrationError("custom provider requires a top-level source or plan")

    _, source_kind = source_type(by_key["source"])
    if source_kind != "placeholder" and "parser" not in by_key:
        raise MigrationError("non-placeholder legacy source requires a top-level parser")

    output_blocks: list[tuple[str | None, list[str]]] = []

    for key, block in blocks:
        if key in MOVABLE_KEYS:
            continue
        if key == "schema_version":
            block = replace_schema_version(block, newline)
        output_blocks.append((key, ensure_block_terminated(block, newline)))
        if key == "id" and version is None:
            output_blocks.append(
                ("schema_version", [f"schema_version: 2{newline}"])
            )

    step_lines = [
        f"plan:{newline}",
        f"  mode: first_success{newline}",
        f"  steps:{newline}",
        f'    - name: "default"{newline}',
        f"      required: true{newline}",
    ]
    for key in ("availability", "source", "preprocess", "parser"):
        block = by_key.get(key)
        if block is None:
            continue
        if key == "source":
            block = migrate_source_block(block, newline)
        step_lines.extend(
            indent_block(ensure_block_terminated(block, newline), 6)
        )

    output_blocks.append(("plan", step_lines))
    migrated = "".join(line for _, block in output_blocks for line in block)
    return migrated, migrated != text


def iter_yaml_files(paths: list[Path]) -> list[Path]:
    files: list[Path] = []
    seen: set[Path] = set()
    for path in paths:
        candidates = [path]
        if path.is_dir():
            candidates = [*sorted(path.glob("*.yaml")), *sorted(path.glob("*.yml"))]
        for candidate in candidates:
            identity = candidate.resolve()
            if identity not in seen:
                seen.add(identity)
                files.append(candidate)
    return files


def prepare_migrations(paths: list[Path]) -> list[tuple[Path, str]]:
    migrations: list[tuple[Path, str]] = []
    for path in iter_yaml_files(paths):
        original = path.read_text(encoding="utf-8")
        try:
            migrated, changed = migrate_text(original)
        except MigrationError as error:
            raise MigrationError(f"{path}: {error}") from error
        if changed:
            migrations.append((path, migrated))
    return migrations


def backup_path(path: Path) -> Path:
    return path.with_suffix(path.suffix + ".bak")


def validate_backup_targets(migrations: list[tuple[Path, str]]) -> None:
    for path, _ in migrations:
        backup = backup_path(path)
        if backup.exists():
            raise MigrationError(f"refusing to overwrite existing backup: {backup}")


def write_text_atomically(path: Path, text: str) -> None:
    mode = stat.S_IMODE(path.stat().st_mode)
    descriptor, temp_name = tempfile.mkstemp(
        prefix=f".{path.name}.", suffix=".tmp", dir=path.parent
    )
    temp_path = Path(temp_name)
    try:
        os.fchmod(descriptor, mode)
        with os.fdopen(descriptor, "w", encoding="utf-8", newline="") as handle:
            descriptor = -1
            handle.write(text)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temp_path, path)
    finally:
        if descriptor >= 0:
            os.close(descriptor)
        temp_path.unlink(missing_ok=True)


def apply_migrations(migrations: list[tuple[Path, str]], *, backup: bool) -> None:
    if backup:
        validate_backup_targets(migrations)
    for path, migrated in migrations:
        if backup:
            shutil.copy2(path, backup_path(path))
        write_text_atomically(path, migrated)


def print_migrations(migrations: list[tuple[Path, str]]) -> None:
    for path, migrated in migrations:
        print(f"--- {path}")
        print(migrated, end="" if migrated.endswith("\n") else "\n")


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

    try:
        migrations = prepare_migrations(args.paths)
        if args.write:
            apply_migrations(migrations, backup=not args.no_backup)
        else:
            print_migrations(migrations)
    except (MigrationError, OSError, UnicodeError) as error:
        print(f"migration failed: {error}", file=sys.stderr)
        return 1

    print(f"migrated {len(migrations)} file(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
