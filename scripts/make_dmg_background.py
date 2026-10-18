#!/usr/bin/env python3
"""Generate the DMG background image (620x440) with no external assets."""
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

W, H = 620, 500
BG = (13, 17, 23)
PANEL = (22, 27, 34)
LINE = (42, 52, 68)
TEXT = (230, 237, 243)
MUTED = (139, 152, 169)
ACCENT = (88, 166, 255)
ACCENT2 = (76, 195, 138)

FONT_PATHS = [
    "/System/Library/Fonts/Helvetica.ttc",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
    "/System/Library/Fonts/SFNS.ttf",
]


def font(size: int, bold: bool = False) -> ImageFont.FreeTypeFont:
    for path in FONT_PATHS:
        if not Path(path).exists():
            continue
        try:
            # Helvetica.ttc contains several faces; index 1 is bold on macOS.
            return ImageFont.truetype(path, size, index=1 if bold else 0)
        except OSError:
            continue
    return ImageFont.load_default()


def main() -> int:
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("assets/dmg-background.png")
    out.parent.mkdir(parents=True, exist_ok=True)

    img = Image.new("RGB", (W, H), BG)
    d = ImageDraw.Draw(img)

    # Soft top band.
    d.rectangle([0, 0, W, 96], fill=PANEL)
    d.line([0, 96, W, 96], fill=LINE, width=1)

    d.text((34, 24), "OPS", font=font(34, True), fill=TEXT)
    d.text((102, 38), "Open Poker Solver", font=font(15), fill=MUTED)
    d.text((34, 66), "Drag OPS into Applications to install", font=font(13), fill=ACCENT)

    # Card-drop icons hint: two rounded tiles under the app position.
    d.rounded_rectangle([112, 150, 208, 246], radius=14, outline=LINE, width=2, fill=PANEL)
    d.text((138, 184), "OPS", font=font(17, True), fill=TEXT)
    d.text((132, 208), "app", font=font(11), fill=MUTED)

    # Arrow.
    y = 198
    d.line([236, y, 374, y], fill=ACCENT, width=4)
    d.polygon([(374, y - 11), (398, y), (374, y + 11)], fill=ACCENT)

    d.rounded_rectangle([410, 150, 508, 246], radius=14, outline=LINE, width=2, fill=PANEL)
    d.text((427, 188), "Apps", font=font(15, True), fill=TEXT)
    d.text((424, 209), "folder", font=font(11), fill=MUTED)

    d.text((34, 470), "Runs locally on 127.0.0.1 · no data leaves your Mac",
           font=font(11), fill=ACCENT2)

    img.save(out, "PNG")
    print(f"wrote {out} ({W}x{H})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
