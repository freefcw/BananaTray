from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from scripts.check_provider_icons import (
    ROOT,
    generate_preview,
    preview_sync_error,
    validate_icon,
    validate_references,
)


VALID_ICON = """\
<svg width="24" height="24" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
  <path d="M4 4H20V20H4Z" fill="currentColor"/>
</svg>
"""


class ProviderIconValidationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp_dir = tempfile.TemporaryDirectory(dir=ROOT)
        self.icon_path = Path(self.temp_dir.name) / "provider-test.svg"

    def tearDown(self) -> None:
        self.temp_dir.cleanup()

    def validate(self, content: str) -> list[str]:
        self.icon_path.write_text(content, encoding="utf-8")
        _, errors = validate_icon(self.icon_path)
        return errors

    def test_accepts_normalized_current_color_icon(self) -> None:
        self.assertEqual(self.validate(VALID_ICON), [])

    def test_rejects_nonstandard_view_box(self) -> None:
        errors = self.validate(VALID_ICON.replace('viewBox="0 0 24 24"', 'viewBox="0 0 32 32"'))
        self.assertTrue(any("viewBox" in error for error in errors))

    def test_rejects_hardcoded_visible_color(self) -> None:
        errors = self.validate(VALID_ICON.replace('fill="currentColor"', 'fill="white"'))
        self.assertTrue(any("currentColor or none" in error for error in errors))

    def test_rejects_unsupported_elements(self) -> None:
        errors = self.validate(VALID_ICON.replace("</svg>", "  <script>bad()</script>\n</svg>"))
        self.assertTrue(any("unsupported <script>" in error for error in errors))

    def test_rejects_missing_rust_icon_reference(self) -> None:
        self.icon_path.write_text(VALID_ICON, encoding="utf-8")
        source_dir = Path(self.temp_dir.name) / "src"
        source_dir.mkdir()
        (source_dir / "provider.rs").write_text(
            'const ICON: &str = "src/icons/provider-missing.svg";\n',
            encoding="utf-8",
        )

        errors = validate_references([self.icon_path], source_dir)

        self.assertTrue(any("provider-missing.svg" in error for error in errors))

    def test_detects_stale_preview(self) -> None:
        preview_path = Path(self.temp_dir.name) / "provider-icons.svg"
        preview_path.write_text("old preview", encoding="utf-8")

        error = preview_sync_error("new preview", preview_path)

        self.assertIsNotNone(error)
        self.assertIn("stale", error or "")

    def test_accepts_current_preview(self) -> None:
        preview_path = Path(self.temp_dir.name) / "provider-icons.svg"
        preview_path.write_text("current preview", encoding="utf-8")

        self.assertIsNone(preview_sync_error("current preview", preview_path))

    def test_generates_preview_for_valid_icon(self) -> None:
        self.icon_path.write_text(VALID_ICON, encoding="utf-8")
        root, errors = validate_icon(self.icon_path)
        self.assertEqual(errors, [])
        self.assertIsNotNone(root)
        assert root is not None

        preview = generate_preview([(self.icon_path, root)])

        self.assertIn("BananaTray provider icon optical review", preview)
        self.assertIn(">test</text>", preview)


if __name__ == "__main__":
    unittest.main()
