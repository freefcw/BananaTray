#!/usr/bin/env python3
"""Regression tests for migrate_custom_provider_yaml.py."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SCRIPT_PATH = Path(__file__).with_name("migrate_custom_provider_yaml.py")
SPEC = importlib.util.spec_from_file_location("migrate_custom_provider_yaml", SCRIPT_PATH)
assert SPEC is not None and SPEC.loader is not None
MIGRATOR = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MIGRATOR)


LEGACY_YAML = """\
id: "legacy:http"
schema_version: 1
base_url: "https://example.com"
metadata:
  display_name: "Legacy"
  brand_name: "Legacy"
source:
  type: http_get
  url: "/api/usage"
parser:
  format: regex
  quotas:
    - label: "Usage"
      pattern: '(\\d+)/(\\d+)'
"""

V2_YAML = """\
id: "current"
schema_version: 2
metadata:
  display_name: "Current"
  brand_name: "Current"
plan:
  mode: first_success
  steps:
    - name: "default"
      required: true
      source:
        type: placeholder
        reason: "Not configured"
"""


class MigrateTextTests(unittest.TestCase):
    def test_replaces_existing_schema_version_without_duplicating_plan(self) -> None:
        migrated, changed = MIGRATOR.migrate_text(LEGACY_YAML)

        self.assertTrue(changed)
        self.assertEqual(migrated.count("schema_version:"), 1)
        self.assertEqual(migrated.count("plan:"), 1)
        self.assertIn("schema_version: 2", migrated)
        self.assertIn("      source:\n        type: http\n        method: get", migrated)

    def test_rejects_existing_plan_combined_with_legacy_fields(self) -> None:
        text = LEGACY_YAML + "plan:\n  steps: []\n"

        with self.assertRaisesRegex(MIGRATOR.MigrationError, "plan"):
            MIGRATOR.migrate_text(text)

    def test_rejects_unknown_top_level_field(self) -> None:
        text = LEGACY_YAML.replace(
            "source:\n", "custom_timeout: 42\nsource:\n", 1
        )

        with self.assertRaisesRegex(MIGRATOR.MigrationError, "custom_timeout"):
            MIGRATOR.migrate_text(text)

    def test_rejects_duplicate_schema_version(self) -> None:
        text = LEGACY_YAML.replace(
            "schema_version: 1\n", "schema_version: 1\nschema_version: 2\n", 1
        )

        with self.assertRaisesRegex(MIGRATOR.MigrationError, "duplicate"):
            MIGRATOR.migrate_text(text)

    def test_rejects_future_schema_version(self) -> None:
        text = LEGACY_YAML.replace("schema_version: 1", "schema_version: 3", 1)

        with self.assertRaisesRegex(MIGRATOR.MigrationError, "schema_version: 3"):
            MIGRATOR.migrate_text(text)

    def test_rejects_future_schema_without_source_or_plan(self) -> None:
        text = """\
id: "future"
schema_version: 3
metadata:
  display_name: "Future"
"""

        with self.assertRaisesRegex(MIGRATOR.MigrationError, "schema_version: 3"):
            MIGRATOR.migrate_text(text)

    def test_rejects_incomplete_v1_without_source(self) -> None:
        text = """\
id: "incomplete"
schema_version: 1
metadata:
  display_name: "Incomplete"
"""

        with self.assertRaisesRegex(MIGRATOR.MigrationError, "source"):
            MIGRATOR.migrate_text(text)

    def test_migrates_quoted_legacy_http_type(self) -> None:
        text = LEGACY_YAML.replace("type: http_get", 'type: "http_get"', 1)

        migrated, changed = MIGRATOR.migrate_text(text)

        self.assertTrue(changed)
        self.assertIn("        type: http\n        method: get", migrated)
        self.assertNotIn("http_get", migrated)

    def test_migrates_commented_legacy_http_type(self) -> None:
        text = LEGACY_YAML.replace(
            "type: http_get", "type: http_get # legacy endpoint", 1
        )

        migrated, changed = MIGRATOR.migrate_text(text)

        self.assertTrue(changed)
        self.assertIn(
            "        type: http # legacy endpoint\n        method: get", migrated
        )
        self.assertNotIn("http_get", migrated)

    def test_migrates_legacy_http_post_type(self) -> None:
        text = LEGACY_YAML.replace("type: http_get", "type: http_post", 1)

        migrated, changed = MIGRATOR.migrate_text(text)

        self.assertTrue(changed)
        self.assertIn("        type: http\n        method: post", migrated)
        self.assertNotIn("http_post", migrated)

    def test_rejects_legacy_http_type_with_existing_method(self) -> None:
        text = LEGACY_YAML.replace(
            "  type: http_get\n", "  type: http_get\n  method: get\n", 1
        )

        with self.assertRaisesRegex(MIGRATOR.MigrationError, "source.method"):
            MIGRATOR.migrate_text(text)

    def test_preserves_legacy_type_text_inside_source_block_scalar(self) -> None:
        text = LEGACY_YAML.replace(
            '  url: "/api/usage"\n',
            '  url: "/api/usage"\n  body: |\n    type: http_get\n',
            1,
        )

        migrated, changed = MIGRATOR.migrate_text(text)

        self.assertTrue(changed)
        self.assertIn("        type: http\n        method: get", migrated)
        self.assertIn("        body: |\n          type: http_get", migrated)
        self.assertEqual(migrated.count("method: get"), 1)

    def test_rejects_multiple_direct_source_types(self) -> None:
        text = LEGACY_YAML.replace(
            "  type: http_get\n", "  type: http_get\n  type: http_post\n", 1
        )

        with self.assertRaisesRegex(MIGRATOR.MigrationError, "source.type"):
            MIGRATOR.migrate_text(text)

    def test_rejects_inline_source_that_cannot_be_migrated_safely(self) -> None:
        text = LEGACY_YAML.replace(
            'source:\n  type: http_get\n  url: "/api/usage"\n',
            'source: { type: http_get, url: "/api/usage" }\n',
            1,
        )

        with self.assertRaisesRegex(MIGRATOR.MigrationError, "source"):
            MIGRATOR.migrate_text(text)

    def test_rejects_non_placeholder_legacy_source_without_parser(self) -> None:
        text = LEGACY_YAML.split("parser:\n", 1)[0]

        with self.assertRaisesRegex(MIGRATOR.MigrationError, "parser"):
            MIGRATOR.migrate_text(text)

    def test_allows_placeholder_legacy_source_without_parser(self) -> None:
        text = """\
id: "placeholder"
schema_version: 1
metadata:
  display_name: "Placeholder"
source:
  type: placeholder
  reason: "Not configured"
"""

        migrated, changed = MIGRATOR.migrate_text(text)

        self.assertTrue(changed)
        self.assertIn("schema_version: 2", migrated)
        self.assertIn("        type: placeholder", migrated)

    def test_separates_generated_plan_after_final_block_without_newline(self) -> None:
        text = (
            'id: "placeholder"\n'
            "source:\n"
            "  type: placeholder\n"
            '  reason: "Not configured"\n'
            "metadata:\n"
            '  display_name: "Placeholder"'
        )  # intentionally no trailing newline

        migrated, changed = MIGRATOR.migrate_text(text)

        self.assertTrue(changed)
        self.assertIn('  display_name: "Placeholder"\nplan:\n', migrated)

    def test_generated_lines_preserve_crlf_style(self) -> None:
        text = LEGACY_YAML.replace("\n", "\r\n")

        migrated, changed = MIGRATOR.migrate_text(text)

        self.assertTrue(changed)
        self.assertNotIn("\n", migrated.replace("\r\n", ""))
        self.assertIn("schema_version: 2\r\n", migrated)
        self.assertIn("plan:\r\n", migrated)

    def test_valid_v2_plan_is_unchanged(self) -> None:
        migrated, changed = MIGRATOR.migrate_text(V2_YAML)

        self.assertFalse(changed)
        self.assertEqual(migrated, V2_YAML)

    def test_rejects_v2_plan_without_required_provider_fields(self) -> None:
        missing_fields = {
            "id": V2_YAML.replace('id: "current"\n', "", 1),
            "metadata": V2_YAML.replace(
                'metadata:\n  display_name: "Current"\n  brand_name: "Current"\n',
                "",
                1,
            ),
        }

        for field, text in missing_fields.items():
            with self.subTest(field=field):
                with self.assertRaisesRegex(MIGRATOR.MigrationError, field):
                    MIGRATOR.migrate_text(text)

    def test_rejects_v2_plan_without_steps(self) -> None:
        text = """\
id: "current"
schema_version: 2
metadata:
  display_name: "Current"
plan:
  mode: first_success
"""

        with self.assertRaisesRegex(MIGRATOR.MigrationError, "steps"):
            MIGRATOR.migrate_text(text)

    def test_rejects_v2_plan_with_empty_inline_steps(self) -> None:
        text = """\
id: "current"
schema_version: 2
metadata:
  display_name: "Current"
plan:
  mode: first_success
  steps: []
"""

        with self.assertRaisesRegex(MIGRATOR.MigrationError, "steps"):
            MIGRATOR.migrate_text(text)


class MigrationCliTests(unittest.TestCase):
    def test_write_creates_backup_and_preserves_content_and_permissions(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            provider = Path(directory) / "provider.yaml"
            provider.write_text(LEGACY_YAML, encoding="utf-8")
            provider.chmod(0o640)
            expected, _ = MIGRATOR.migrate_text(LEGACY_YAML)

            result = subprocess.run(
                [sys.executable, str(SCRIPT_PATH), str(provider), "--write"],
                capture_output=True,
                check=False,
                text=True,
            )

            backup = provider.with_suffix(".yaml.bak")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("migrated 1 file(s)", result.stdout)
            self.assertEqual(provider.read_text(encoding="utf-8"), expected)
            self.assertEqual(backup.read_text(encoding="utf-8"), LEGACY_YAML)
            self.assertEqual(provider.stat().st_mode & 0o777, 0o640)
            self.assertEqual(backup.stat().st_mode & 0o777, 0o640)

    def test_write_preflight_failure_leaves_every_file_unchanged(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            valid = root / "valid.yaml"
            invalid = root / "invalid.yaml"
            valid.write_text(LEGACY_YAML, encoding="utf-8")
            invalid_text = LEGACY_YAML.replace(
                "source:\n", "unknown_setting: true\nsource:\n", 1
            )
            invalid.write_text(invalid_text, encoding="utf-8")

            result = subprocess.run(
                [sys.executable, str(SCRIPT_PATH), str(root), "--write"],
                capture_output=True,
                check=False,
                text=True,
            )

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("unknown_setting", result.stderr)
            self.assertEqual(valid.read_text(encoding="utf-8"), LEGACY_YAML)
            self.assertEqual(invalid.read_text(encoding="utf-8"), invalid_text)
            self.assertFalse(valid.with_suffix(".yaml.bak").exists())

    def test_existing_backup_rejects_entire_batch(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            first = root / "first.yaml"
            second = root / "second.yaml"
            first.write_text(LEGACY_YAML, encoding="utf-8")
            second.write_text(LEGACY_YAML, encoding="utf-8")
            second_backup = second.with_suffix(".yaml.bak")
            second_backup.write_text("existing backup\n", encoding="utf-8")

            result = subprocess.run(
                [sys.executable, str(SCRIPT_PATH), str(root), "--write"],
                capture_output=True,
                check=False,
                text=True,
            )

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("refusing to overwrite existing backup", result.stderr)
            self.assertEqual(first.read_text(encoding="utf-8"), LEGACY_YAML)
            self.assertEqual(second.read_text(encoding="utf-8"), LEGACY_YAML)
            self.assertFalse(first.with_suffix(".yaml.bak").exists())
            self.assertEqual(
                second_backup.read_text(encoding="utf-8"), "existing backup\n"
            )

    def test_no_backup_writes_without_creating_backup(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            provider = Path(directory) / "provider.yaml"
            provider.write_text(LEGACY_YAML, encoding="utf-8")
            expected, _ = MIGRATOR.migrate_text(LEGACY_YAML)

            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT_PATH),
                    str(provider),
                    "--write",
                    "--no-backup",
                ],
                capture_output=True,
                check=False,
                text=True,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(provider.read_text(encoding="utf-8"), expected)
            self.assertFalse(provider.with_suffix(".yaml.bak").exists())


if __name__ == "__main__":
    unittest.main()
