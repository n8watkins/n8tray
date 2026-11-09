"""Draws N8 Tray's icons: a rounded tile with an "8" made of two rings.

Run from the repository root: python3 scripts/make-icons.py
Writes app/src-tauri/icons/. Needs Pillow. The tray's level badge is drawn
at run time by the app over tray.png, so it is not part of these files.
"""

from pathlib import Path

from PIL import Image, ImageDraw

OUT = Path("app/src-tauri/icons")
TILE = (31, 41, 55, 255)  # slate
INK = (255, 255, 255, 255)


def draw(size: int, tile: bool = True) -> Image.Image:
    scale = 4
    s = size * scale
    image = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(image)
    if tile:
        radius = int(s * 0.22)
        d.rounded_rectangle([0, 0, s - 1, s - 1], radius=radius, fill=TILE)
    # Two stacked rings: the "8" of N8.
    stroke = max(int(s * 0.11), scale)
    cx = s / 2
    top_r = s * 0.17
    bottom_r = s * 0.205
    top_cy = s * 0.33
    bottom_cy = s * 0.64
    for cy, r in ((top_cy, top_r), (bottom_cy, bottom_r)):
        d.ellipse([cx - r, cy - r, cx + r, cy + r], outline=INK, width=stroke)
    return image.resize((size, size), Image.LANCZOS)


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    for name, size in (("32x32.png", 32), ("128x128.png", 128), ("128x128@2x.png", 256), ("icon.png", 512)):
        draw(size).save(OUT / name)
    draw(64).save(OUT / "tray.png")
    draw(256).save(OUT / "icon.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
