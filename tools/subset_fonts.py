"""Cuts the bundled fonts down to what Dockview draws.

Inter (the text) keeps Latin, Greek and Cyrillic (a captured window's title
can be in any of them), punctuation, arrows and currency signs, plus every
non-ASCII character that appears in the interface texts. Lucide (the icons)
keeps only the icons the interface uses, found as `\\u{eXXX}` in the sources of
`dockview-ui`.

The full fonts live in `crates/dockview-ui/assets/fonts/full/` (they are not
compiled into the program); the cut ones are written next to them, where
`text.rs` includes them. Run it again after adding an icon or a new kind of
text:

    python tools/subset_fonts.py

It needs fontTools (`pip install fonttools`). A test in `dockview-ui` checks
that every icon and every interface text has its glyphs in the cut fonts.
"""

import re
import sys
from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont

ROOT = Path(__file__).resolve().parent.parent
FONTS = ROOT / "crates" / "dockview-ui" / "assets" / "fonts"
FULL = FONTS / "full"

# What Inter keeps, besides the characters found in the texts.
INTER_RANGES = [
    (0x0020, 0x007E),  # Basic Latin
    (0x00A0, 0x024F),  # Latin-1 Supplement, Latin Extended-A and B
    (0x0370, 0x03FF),  # Greek
    (0x0400, 0x04FF),  # Cyrillic
    (0x2000, 0x206F),  # General punctuation
    (0x20A0, 0x20CF),  # Currency signs
    (0x2190, 0x21FF),  # Arrows
    (0x2212, 0x2212),  # Minus sign
]
# OpenType features the interface uses or relies on by default. `tnum` is the
# tabular figures of the statistics panel.
INTER_FEATURES = ["kern", "liga", "calt", "ccmp", "locl", "mark", "mkmk", "tnum", "case"]
INTER_FILES = ["Inter-Regular.ttf", "Inter-Medium.ttf", "Inter-SemiBold.ttf"]


def icon_codepoints() -> set[int]:
    """The private-use code points of the icons written as `\\u{eXXX}`."""
    found: set[int] = set()
    for source in (ROOT / "crates" / "dockview-ui" / "src").rglob("*.rs"):
        for match in re.finditer(r"\\u\{(e[0-9a-fA-F]{3})\}", source.read_text(encoding="utf-8")):
            found.add(int(match.group(1), 16))
    return found


def text_codepoints() -> set[int]:
    """Every non-ASCII character in the interface texts."""
    text = (ROOT / "crates" / "dockview-domain" / "src" / "i18n.rs").read_text(encoding="utf-8")
    return {ord(c) for c in text if ord(c) > 127 and not 0xE000 <= ord(c) <= 0xF8FF}


def cut(source: Path, target: Path, unicodes: set[int], features: list[str]) -> None:
    options = subset.Options()
    options.layout_features = features
    options.hinting = True  # without it the pixels of the glyphs change
    options.glyph_names = False
    options.notdef_outline = True
    # All the names: the typographic family and style (IDs 16 and 17) are how
    # "Inter Medium" is found as the Medium weight of the family "Inter".
    options.name_IDs = ["*"]
    options.legacy_kern = False
    font = subset.load_font(str(source), options)
    subsetter = subset.Subsetter(options)
    subsetter.populate(unicodes=sorted(unicodes))
    subsetter.subset(font)
    subset.save_font(font, str(target), options)
    font.close()


def main() -> int:
    if not FULL.is_dir():
        print(f"missing {FULL}", file=sys.stderr)
        return 1
    inter = set()
    for first, last in INTER_RANGES:
        inter.update(range(first, last + 1))
    inter |= text_codepoints()
    for name in INTER_FILES:
        cut(FULL / name, FONTS / name, inter, INTER_FEATURES)

    icons = icon_codepoints()
    cut(FULL / "lucide.ttf", FONTS / "lucide.ttf", icons, [])

    for name in INTER_FILES + ["lucide.ttf"]:
        before = (FULL / name).stat().st_size
        after = (FONTS / name).stat().st_size
        glyphs = len(TTFont(FONTS / name).getGlyphOrder())
        print(f"{name}: {before:>8} -> {after:>8} bytes, {glyphs} glyphs")
    print(f"icons kept: {len(icons)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
