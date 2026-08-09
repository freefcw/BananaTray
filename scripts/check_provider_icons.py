#!/usr/bin/env python3
from __future__ import annotations

import argparse
import copy
import re
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ICON_DIR = ROOT / "src" / "icons"
PREVIEW_PATH = ROOT / "docs" / "design-references" / "provider-icons.svg"
SVG_NS = "http://www.w3.org/2000/svg"
ET.register_namespace("", SVG_NS)


def qname(name: str) -> str:
    return f"{{{SVG_NS}}}{name}"


GRAPHIC_TAGS = {"path", "circle", "rect", "ellipse", "line", "polyline", "polygon"}
ALLOWED_ATTRIBUTES = {
    "svg": {"width", "height", "viewBox", "fill"},
    "g": {
        "fill",
        "stroke",
        "stroke-width",
        "stroke-linecap",
        "stroke-linejoin",
        "fill-rule",
        "clip-rule",
        "fill-opacity",
        "stroke-opacity",
        "transform",
    },
    "path": {
        "d",
        "fill",
        "stroke",
        "stroke-width",
        "stroke-linecap",
        "stroke-linejoin",
        "fill-rule",
        "clip-rule",
        "fill-opacity",
        "stroke-opacity",
        "transform",
    },
    "circle": {
        "cx",
        "cy",
        "r",
        "fill",
        "stroke",
        "stroke-width",
        "stroke-linecap",
        "stroke-linejoin",
        "fill-opacity",
        "stroke-opacity",
        "transform",
    },
    "rect": {
        "x",
        "y",
        "width",
        "height",
        "rx",
        "ry",
        "fill",
        "stroke",
        "stroke-width",
        "stroke-linecap",
        "stroke-linejoin",
        "fill-opacity",
        "stroke-opacity",
        "transform",
    },
    "ellipse": {
        "cx",
        "cy",
        "rx",
        "ry",
        "fill",
        "stroke",
        "stroke-width",
        "fill-opacity",
        "stroke-opacity",
        "transform",
    },
    "line": {
        "x1",
        "y1",
        "x2",
        "y2",
        "stroke",
        "stroke-width",
        "stroke-linecap",
        "stroke-opacity",
        "transform",
    },
    "polyline": {
        "points",
        "fill",
        "stroke",
        "stroke-width",
        "stroke-linecap",
        "stroke-linejoin",
        "fill-opacity",
        "stroke-opacity",
        "transform",
    },
    "polygon": {
        "points",
        "fill",
        "stroke",
        "stroke-width",
        "stroke-linecap",
        "stroke-linejoin",
        "fill-opacity",
        "stroke-opacity",
        "transform",
    },
}
TRANSFORM_RE = re.compile(
    r"^(?:(?:matrix|translate|scale|rotate|skewX|skewY)\([0-9eE+.,\s-]+\)\s*)+$"
)
ICON_NAME_RE = re.compile(r"^provider-[a-z0-9]+(?:-[a-z0-9]+)*\.svg$")
SOURCE_ICON_RE = re.compile(r'"(src/icons/provider-[a-z0-9-]+\.svg)"')

PREVIEW_WIDTH = 800
PREVIEW_HEADER_HEIGHT = 84
PREVIEW_ROW_HEIGHT = 68
PREVIEW_FOOTER_HEIGHT = 18
PREVIEW_LABEL_X = 24
PREVIEW_PANEL_TOP_INSET = 6
PREVIEW_PANEL_HEIGHT = 56
PREVIEW_SECTION_WIDTH = 252
PREVIEW_LIGHT_X = 180
PREVIEW_DARK_X = 452
PREVIEW_MUTED_X = 724
PREVIEW_MUTED_CENTER_X = 752
PREVIEW_BOX_X_OFFSET = 190
PREVIEW_BOX_CENTER_OFFSET = 218
PREVIEW_ICON_COLUMNS = ((22, 15, "15"), (66, 16, "16"), (110, 20, "20"), (158, 32, "32"))
PREVIEW_THEMES = (
    (PREVIEW_LIGHT_X, "LIGHT", "#30343b", "#ffffff", "#e8eaed"),
    (PREVIEW_DARK_X, "DARK", "#f1f2f4", "#17191d", "#2b2e34"),
)


def local_name(name: str) -> str:
    return name.rsplit("}", 1)[-1]


def parse_number(value: str) -> float:
    return float(value.strip())


def validate_icon(path: Path) -> tuple[ET.Element | None, list[str]]:
    errors: list[str] = []
    relative = path.relative_to(ROOT)

    if not ICON_NAME_RE.fullmatch(path.name):
        errors.append(f"{relative}: filename must match provider-<id>.svg")

    try:
        root = ET.parse(path).getroot()
    except (ET.ParseError, OSError) as error:
        return None, [f"{relative}: invalid XML: {error}"]

    if root.tag != qname("svg"):
        errors.append(f"{relative}: root must be an SVG element in the SVG namespace")
        return root, errors

    if root.get("width") != "24" or root.get("height") != "24":
        errors.append(f'{relative}: root width and height must both be "24"')

    view_box = root.get("viewBox", "").replace(",", " ").split()
    try:
        parsed_view_box = [parse_number(part) for part in view_box]
    except ValueError:
        parsed_view_box = []
    if parsed_view_box != [0.0, 0.0, 24.0, 24.0]:
        errors.append(f'{relative}: viewBox must be "0 0 24 24"')

    if root.get("fill") != "none":
        errors.append(f'{relative}: root fill must be "none"')

    graphic_count = 0
    uses_current_color = False

    for element in root.iter():
        tag = local_name(element.tag)
        allowed = ALLOWED_ATTRIBUTES.get(tag)
        if allowed is None:
            errors.append(f"{relative}: unsupported <{tag}> element")
            continue

        for raw_attribute, value in element.attrib.items():
            attribute = local_name(raw_attribute)
            if attribute not in allowed:
                errors.append(f"{relative}: unsupported {attribute!r} attribute on <{tag}>")
                continue

            if attribute in {"fill", "stroke"}:
                if value not in {"none", "currentColor"}:
                    errors.append(
                        f"{relative}: {attribute} must use currentColor or none, got {value!r}"
                    )
                uses_current_color = uses_current_color or value == "currentColor"

            if attribute in {"fill-opacity", "stroke-opacity"}:
                try:
                    opacity = parse_number(value)
                except ValueError:
                    opacity = -1.0
                if not 0.0 <= opacity <= 1.0:
                    errors.append(f"{relative}: {attribute} must be between 0 and 1")

            if attribute == "stroke-width":
                try:
                    stroke_width = parse_number(value)
                except ValueError:
                    stroke_width = 0.0
                if not 1.5 <= stroke_width <= 2.0:
                    errors.append(
                        f"{relative}: stroke-width must stay within the 1.5-2.0 optical range"
                    )

            if attribute == "transform" and not TRANSFORM_RE.fullmatch(value):
                errors.append(f"{relative}: transform contains unsupported syntax")

            if attribute == "fill-rule" and value not in {"evenodd", "nonzero"}:
                errors.append(f"{relative}: unsupported fill-rule {value!r}")

            if attribute == "clip-rule" and value not in {"evenodd", "nonzero"}:
                errors.append(f"{relative}: unsupported clip-rule {value!r}")

            if attribute == "stroke-linecap" and value not in {"butt", "round", "square"}:
                errors.append(f"{relative}: unsupported stroke-linecap {value!r}")

            if attribute == "stroke-linejoin" and value not in {"miter", "round", "bevel"}:
                errors.append(f"{relative}: unsupported stroke-linejoin {value!r}")

        if tag in GRAPHIC_TAGS:
            graphic_count += 1
            if "fill" not in element.attrib and "stroke" not in element.attrib:
                errors.append(f"{relative}: <{tag}> must declare fill or stroke explicitly")
            if tag == "path" and not element.get("d", "").strip():
                errors.append(f"{relative}: path data must not be empty")

    if graphic_count == 0:
        errors.append(f"{relative}: icon must contain at least one graphical element")
    if not uses_current_color:
        errors.append(f"{relative}: icon must render through currentColor")

    return root, errors


def validate_references(
    icon_paths: list[Path], source_dir: Path = ROOT / "src"
) -> list[str]:
    errors: list[str] = []
    available = {f"src/icons/{path.name}" for path in icon_paths}
    referenced: set[str] = set()

    for source in source_dir.rglob("*.rs"):
        for line in source.read_text(encoding="utf-8").splitlines():
            code = line.split("//", 1)[0]
            referenced.update(SOURCE_ICON_RE.findall(code))

    for reference in sorted(referenced):
        if reference not in available:
            errors.append(f"{reference}: referenced by Rust code but the asset does not exist")

    return errors


def add_text(parent: ET.Element, x: float, y: float, text: str, **attributes: str) -> None:
    base = {
        "x": f"{x:g}",
        "y": f"{y:g}",
        "fill": "#5a606a",
        "font-family": "-apple-system, BlinkMacSystemFont, Segoe UI, sans-serif",
        "font-size": "11",
    }
    base.update(attributes)
    element = ET.SubElement(parent, qname("text"), base)
    element.text = text


def add_rect(
    parent: ET.Element,
    x: float,
    y: float,
    width: float,
    height: float,
    **attributes: str,
) -> None:
    base = {
        "x": f"{x:g}",
        "y": f"{y:g}",
        "width": f"{width:g}",
        "height": f"{height:g}",
    }
    base.update(attributes)
    ET.SubElement(parent, qname("rect"), base)


def add_icon(
    parent: ET.Element,
    source: ET.Element,
    center_x: float,
    center_y: float,
    size: float,
    color: str,
) -> None:
    x = center_x - size / 2
    y = center_y - size / 2
    group = ET.SubElement(
        parent,
        qname("g"),
        {
            "color": color,
            "transform": f"translate({x:g} {y:g}) scale({size / 24:g})",
        },
    )
    for child in source:
        group.append(copy.deepcopy(child))


def generate_preview(icons: list[tuple[Path, ET.Element]]) -> str:
    height = (
        PREVIEW_HEADER_HEIGHT
        + PREVIEW_ROW_HEIGHT * len(icons)
        + PREVIEW_FOOTER_HEIGHT
    )
    root = ET.Element(
        qname("svg"),
        {
            "width": str(PREVIEW_WIDTH),
            "height": str(height),
            "viewBox": f"0 0 {PREVIEW_WIDTH} {height}",
        },
    )
    add_rect(root, 0, 0, PREVIEW_WIDTH, height, fill="#eef0f3")
    add_text(
        root,
        PREVIEW_LABEL_X,
        27,
        "BananaTray provider icon optical review",
        fill="#20242a",
        **{"font-size": "16", "font-weight": "600"},
    )
    for section_x, label, _, _, _ in PREVIEW_THEMES:
        add_text(root, section_x, 52, label, fill="#4e555f", **{"font-weight": "600"})
    add_text(root, PREVIEW_MUTED_X, 52, "MUTED", fill="#4e555f", **{"font-weight": "600"})

    for section_x, _, _, _, _ in PREVIEW_THEMES:
        for offset, _, label in PREVIEW_ICON_COLUMNS:
            add_text(
                root,
                section_x + offset,
                73,
                label,
                fill="#737983",
                **{"font-size": "9", "text-anchor": "middle"},
            )
        add_text(
            root,
            section_x + PREVIEW_BOX_CENTER_OFFSET,
            73,
            "BOX",
            fill="#737983",
            **{"font-size": "9", "text-anchor": "middle"},
        )
    add_text(
        root,
        PREVIEW_MUTED_CENTER_X,
        73,
        "16",
        fill="#737983",
        **{"font-size": "9", "text-anchor": "middle"},
    )

    for index, (path, source) in enumerate(icons):
        row_y = PREVIEW_HEADER_HEIGHT + index * PREVIEW_ROW_HEIGHT
        center_y = row_y + PREVIEW_ROW_HEIGHT / 2
        if index % 2 == 1:
            add_rect(
                root,
                12,
                row_y,
                PREVIEW_WIDTH - 24,
                PREVIEW_ROW_HEIGHT,
                fill="#e8eaed",
                rx="4",
            )

        add_text(
            root,
            PREVIEW_LABEL_X,
            center_y + 4,
            path.stem.removeprefix("provider-"),
            fill="#30353c",
            **{"font-size": "12", "font-weight": "500"},
        )

        for section_x, _, color, panel_bg, box_bg in PREVIEW_THEMES:
            add_rect(
                root,
                section_x,
                row_y + PREVIEW_PANEL_TOP_INSET,
                PREVIEW_SECTION_WIDTH,
                PREVIEW_PANEL_HEIGHT,
                fill=panel_bg,
                rx="4",
            )
            for offset, size, _ in PREVIEW_ICON_COLUMNS:
                add_icon(root, source, section_x + offset, center_y, size, color)
            add_rect(
                root,
                section_x + PREVIEW_BOX_X_OFFSET,
                row_y + PREVIEW_PANEL_TOP_INSET,
                PREVIEW_PANEL_HEIGHT,
                PREVIEW_PANEL_HEIGHT,
                fill=box_bg,
                rx="8",
            )
            add_icon(
                root,
                source,
                section_x + PREVIEW_BOX_CENTER_OFFSET,
                center_y,
                32,
                color,
            )

        add_rect(
            root,
            PREVIEW_MUTED_X,
            row_y + PREVIEW_PANEL_TOP_INSET,
            PREVIEW_PANEL_HEIGHT,
            PREVIEW_PANEL_HEIGHT,
            fill="#ffffff",
            rx="4",
        )
        add_icon(root, source, PREVIEW_MUTED_CENTER_X, center_y, 16, "#8a9099")

    ET.indent(root, space="  ")
    return ET.tostring(root, encoding="unicode", xml_declaration=False) + "\n"


def preview_sync_error(preview: str, preview_path: Path = PREVIEW_PATH) -> str | None:
    if not preview_path.exists():
        return f"{preview_path}: missing; run with --write-preview"
    if preview_path.read_text(encoding="utf-8") != preview:
        return (
            "provider icon preview is stale; run "
            "`python3 scripts/check_provider_icons.py --write-preview`"
        )
    return None


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate BananaTray provider SVG assets")
    preview_group = parser.add_mutually_exclusive_group()
    preview_group.add_argument(
        "--write-preview",
        action="store_true",
        help="regenerate the committed provider icon contact sheet",
    )
    preview_group.add_argument(
        "--check-preview",
        action="store_true",
        help="fail when the committed contact sheet is stale",
    )
    args = parser.parse_args()

    icon_paths = sorted(ICON_DIR.glob("provider-*.svg"))
    if not icon_paths:
        print("No provider icons found", file=sys.stderr)
        return 1

    parsed_icons: list[tuple[Path, ET.Element]] = []
    errors: list[str] = []
    for path in icon_paths:
        root, icon_errors = validate_icon(path)
        errors.extend(icon_errors)
        if root is not None:
            parsed_icons.append((path, root))
    errors.extend(validate_references(icon_paths))

    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        print(f"Provider icon validation failed with {len(errors)} error(s)", file=sys.stderr)
        return 1

    preview = generate_preview(parsed_icons)
    if args.write_preview:
        PREVIEW_PATH.parent.mkdir(parents=True, exist_ok=True)
        PREVIEW_PATH.write_text(preview, encoding="utf-8")
        print(f"Updated {PREVIEW_PATH.relative_to(ROOT)}")
    elif args.check_preview:
        if error := preview_sync_error(preview):
            print(f"ERROR: {error}", file=sys.stderr)
            return 1

    print(f"Validated {len(icon_paths)} provider icons")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
