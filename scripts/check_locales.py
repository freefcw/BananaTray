#!/usr/bin/env python3
"""Validate that all rust-i18n locale files under locales/ stay aligned.

Uses only the Python standard library. Locale files are parsed as a
conservative YAML subset (block mappings, quoted/plain one-line scalars)
and flattened to dotted key paths, so flat keys like `tray.menu.open` and
nested mappings compare equal. rust-i18n resolves missing keys only at
runtime, so key drift between locale files would otherwise stay silent.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LOCALE_DIR = ROOT / "locales"

ENTRY_PATTERN = re.compile(
    r"^(?P<indent> *)"
    r"(?:\"(?P<double>(?:[^\"\\]|\\.)*)\"|'(?P<single>(?:[^']|'')*)'"
    r"|(?P<plain>[^:\s#][^:#]*?))"
    r"\s*:(?:[ \t]+(?P<value>\"(?:[^\"\\]|\\.)*\"|'(?:[^']|'')*'|[^\s#][^#]*?)"
    r"\s*(?:#.*)?|\s*(?:#.*)?)$"
)
# rust-i18n 占位符语法为 %{name}
PLACEHOLDER_PATTERN = re.compile(r"%\{([^{}]*)\}")


class LocaleParseError(ValueError):
    """Locale file uses YAML syntax outside the supported subset."""


def structural_lines(path: Path) -> list[tuple[int, int, str, str | None]]:
    items: list[tuple[int, int, str, str | None]] = []
    for lineno, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        content = raw.strip()
        if not content or content.startswith("#"):
            continue
        match = ENTRY_PATTERN.fullmatch(raw)
        if match is None:
            raise LocaleParseError(
                f"{path}:{lineno}: unsupported YAML syntax: {content!r}"
            )
        key = next(
            value
            for value in (
                match.group("double"),
                match.group("single"),
                match.group("plain"),
            )
            if value is not None
        )
        items.append((lineno, len(match.group("indent")), key, match.group("value")))
    return items


def parse_block(
    path: Path,
    items: list[tuple[int, int, str, str | None]],
    pos: int,
    indent: int,
    prefix: str,
    entries: dict[str, str],
) -> int:
    while pos < len(items):
        lineno, line_indent, key, value = items[pos]
        if line_indent < indent:
            return pos
        if line_indent > indent:
            raise LocaleParseError(f"{path}:{lineno}: unexpected indentation level")

        full_key = f"{prefix}.{key}" if prefix else key
        pos += 1
        if value is None:
            if pos >= len(items) or items[pos][1] <= line_indent:
                raise LocaleParseError(
                    f"{path}:{lineno}: key {full_key!r} has neither value nor nested keys"
                )
            pos = parse_block(path, items, pos, items[pos][1], full_key, entries)
            continue

        if full_key in entries:
            raise LocaleParseError(f"{path}:{lineno}: duplicate key: {full_key}")
        entries[full_key] = value
    return pos


def parse_locale(path: Path) -> dict[str, str]:
    items = structural_lines(path)
    entries: dict[str, str] = {}
    if items:
        pos = parse_block(path, items, 0, items[0][1], "", entries)
        if pos != len(items):
            raise LocaleParseError(f"{path}: trailing unparseable content")
    if not entries:
        raise LocaleParseError(f"{path}: no translation keys found")
    return entries


def alignment_errors(parsed: dict[str, dict[str, str]]) -> list[str]:
    errors: list[str] = []
    all_keys: dict[str, list[str]] = {}
    for name, entries in parsed.items():
        for key in entries:
            all_keys.setdefault(key, []).append(name)

    for key in sorted(all_keys):
        present = ", ".join(all_keys[key])
        for name in parsed:
            if key not in parsed[name]:
                errors.append(f'locales/{name}: missing key "{key}" (present in {present})')

    for key in sorted(all_keys):
        placeholders = {
            name: set(PLACEHOLDER_PATTERN.findall(entries[key]))
            for name, entries in parsed.items()
            if key in entries
        }
        if len({frozenset(found) for found in placeholders.values()}) <= 1:
            continue
        detail = ", ".join(
            f"{name}={{{', '.join(sorted(found))}}}"
            for name, found in sorted(placeholders.items())
        )
        errors.append(f'key "{key}" placeholders differ: {detail}')
    return errors


def main() -> int:
    locale_paths = sorted(LOCALE_DIR.glob("*.yml")) + sorted(LOCALE_DIR.glob("*.yaml"))
    if len(locale_paths) < 2:
        print(f"Locale alignment requires at least two locale files in {LOCALE_DIR}")
        return 0

    parsed: dict[str, dict[str, str]] = {}
    errors: list[str] = []
    for path in locale_paths:
        try:
            parsed[path.name] = parse_locale(path)
        except LocaleParseError as error:
            errors.append(str(error))
    if not errors:
        errors.extend(alignment_errors(parsed))

    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        print(f"Locale alignment failed with {len(errors)} error(s)", file=sys.stderr)
        return 1

    key_count = len(set().union(*(set(entries) for entries in parsed.values())))
    names = ", ".join(parsed)
    print(f"Validated locale alignment across {names} ({key_count} keys)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
