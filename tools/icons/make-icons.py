"""Makes N8 Tray's icons from the n8builds "n8" logo.

Run from the repository root:

    python3 tools/icons/make-icons.py

Needs Pillow (only to make these files; the app has no Python at run time).
Reads the original artwork, tools/icons/n8builds-logo.png (and, for the tray
on a light taskbar, tools/icons/n8-monogram-light.png), and writes
app/src-tauri/icons/ and the hub's own icon, app/ui/src/assets/n8tray.png.
Nothing is redrawn: the artwork is cropped to the mark, the logo's black
background is keyed out so the glow fades to transparent, and it is resized.
"""

from dataclasses import dataclass
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
SOURCE = HERE / "n8builds-logo.png"
ICONS = ROOT / "app/src-tauri/icons"
HUB = ROOT / "app/ui/src/assets/n8tray.png"

# The solid mark in the source (its full-brightness pixels), measured once.
MARK = (228, 134, 1306, 954)
# Brightness of the brightest channel that counts as the mark, not its glow.
SOLID = 230
# The source's "black" is a few levels above zero; below this is background.
FLOOR = 4

# The tray icon for a light taskbar: the cyan and navy monogram (cyan n,
# navy 8), whose solid mark spans these pixels (alpha above half).
LIGHT_SOURCE = HERE / "n8-monogram-light.png"
LIGHT_MARK = (219, 305, 1037, 970)
# Alpha below this is a speck of noise around the mark, not part of it.
SPECK = 8

# From this size up the mark sits on a dark rounded tile: on a light Start
# menu or Explorer window the white n and its glow vanish without one.
TILE_FROM = 48
TILE = (11, 16, 24, 255)
TILE_INNER = 0.82  # the mark's share of the tile's width

ICO_SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256]


@dataclass(frozen=True)
class Cut:
    """How the mark is cut out: the empty margin around it (source px) and
    how far its glow reaches before it fades to transparent."""

    margin: int
    glow: float


# Bare (tray and small icons): the mark fills the square as far as its wide
# shape allows, and the glow stays close so it does not smear at 16 px.
BARE = Cut(margin=40, glow=20)
# On the tile the mark is smaller, so the glow has room to spread.
TILED = Cut(margin=round((MARK[2] - MARK[0]) * (1 / TILE_INNER - 1) / 2), glow=45)


def brightest(img: Image.Image) -> Image.Image:
    r, g, b = img.split()
    return ImageChops.lighter(ImageChops.lighter(r, g), b)


def keyed(cut: Cut) -> Image.Image:
    """The artwork as a square RGBA image, black keyed out to transparent."""
    src = Image.open(SOURCE).convert("RGB")
    x0, y0, x1, y1 = MARK
    side = max(x1 - x0, y1 - y0) + 2 * cut.margin
    left = round((x0 + x1 - side) / 2)
    top = round((y0 + y1 - side) / 2)
    square = Image.new("RGB", (side, side), (0, 0, 0))
    square.paste(src, (-left, -top))
    # The glow is faded out along the mark's own outline (a blurred copy of
    # its solid shape), so it ends in a soft halo that follows the letters.
    solid = brightest(square).point(lambda v: 255 if v >= SOLID else 0)
    halo = solid.filter(ImageFilter.GaussianBlur(cut.glow)).point(lambda v: min(255, v * 3))
    # It also reaches zero before the crop and before the source's own edge,
    # so neither shows as a line.
    edge = Image.new("L", (side, side), 0)
    inside = (max(0, -left), max(0, -top), min(side, src.width - left), min(side, src.height - top))
    ImageDraw.Draw(edge).rectangle([inside[0] + 1, inside[1] + 1, inside[2] - 2, inside[3] - 2], fill=255)
    ramp = cut.margin / 2
    edge = edge.filter(ImageFilter.BoxBlur(ramp / 2)).point(lambda v: max(0, 2 * v - 255))
    keep = ImageChops.multiply(halo, edge)

    out = Image.new("RGBA", (side, side))
    sp, kp, op = square.load(), keep.load(), out.load()
    for y in range(side):
        for x in range(side):
            r, g, b = sp[x, y]
            m = max(r, g, b)
            if m <= FLOOR:
                op[x, y] = (0, 0, 0, 0)
                continue
            # The art is light added over black: alpha is the brightest
            # channel, and the colour is the pixel un-multiplied by it.
            a = (m - FLOOR) / (255 - FLOOR) * kp[x, y] / 255
            op[x, y] = (r * 255 // m, g * 255 // m, b * 255 // m, round(a * 255))
    return out


def resized(art: Image.Image, size: int) -> Image.Image:
    # Resize premultiplied so transparent pixels do not darken the edges.
    return art.convert("RGBa").resize((size, size), Image.LANCZOS).convert("RGBA")


def light_mark() -> Image.Image:
    """The monogram for a light taskbar as a square RGBA image.

    The file already has a transparent background (it looks white only in a
    viewer), so it needs no keying: specks of near-zero alpha are dropped,
    the solid colours' alpha of 253-254 is made opaque, and it is cropped.
    """
    src = Image.open(LIGHT_SOURCE).convert("RGBA")
    a = src.getchannel("A").point(lambda v: 0 if v < SPECK else 255 if v >= 250 else v)
    src.putalpha(a)
    x0, y0, x1, y1 = LIGHT_MARK
    margin = round((x1 - x0) * BARE.margin / (MARK[2] - MARK[0]))
    side = max(x1 - x0, y1 - y0) + 2 * margin
    left = round((x0 + x1 - side) / 2)
    top = round((y0 + y1 - side) / 2)
    return src.crop((left, top, left + side, top + side))


def tiled(art: Image.Image, size: int) -> Image.Image:
    s = size * 4
    tile = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    ImageDraw.Draw(tile).rounded_rectangle([0, 0, s - 1, s - 1], radius=s * 0.22, fill=TILE)
    tile = resized(tile, size)
    return Image.alpha_composite(tile, resized(art, size))


class Icons:
    def __init__(self) -> None:
        self.bare = keyed(BARE)
        self.tiled = keyed(TILED)

    def icon(self, size: int) -> Image.Image:
        if size >= TILE_FROM:
            return tiled(self.tiled, size)
        return resized(self.bare, size)


def main() -> None:
    icons = Icons()
    ICONS.mkdir(parents=True, exist_ok=True)
    for name, size in (("32x32.png", 32), ("128x128.png", 128), ("128x128@2x.png", 256), ("icon.png", 512)):
        icons.icon(size).save(ICONS / name)
    # The tray's base images, for a dark and a light taskbar; the app draws
    # the level dot over the one in use (badge.rs).
    resized(icons.bare, 64).save(ICONS / "tray.png")
    resized(light_mark(), 64).save(ICONS / "tray-light.png")
    # Each .ico entry is made at its own size, not shrunk from the largest.
    images = [icons.icon(s) for s in ICO_SIZES]
    images[-1].save(
        ICONS / "icon.ico",
        sizes=[(s, s) for s in ICO_SIZES],
        append_images=images[:-1],
        bitmap_format="png",
    )
    # The hub shows N8 Tray's own icon next to the apps' tiles (spec 3.11)
    # at up to 32 CSS px; 64 covers 200 percent.
    HUB.parent.mkdir(parents=True, exist_ok=True)
    tiled(icons.tiled, 64).save(HUB)
    print(f"wrote {ICONS.relative_to(ROOT)} and {HUB.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
