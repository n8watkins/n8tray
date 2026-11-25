"""Makes N8 Tray's icons from the owner's n8builds artwork.

Run from the repository root:

    python3 tools/icons/make-icons.py

Needs Pillow (only to make these files; the app has no Python at run time).
Nothing is redrawn; each source is only cut out and resized:

- n8-flow-app.png (the Flow monogram on its bordered tile) is the app icon:
  icon.ico, icon.png, 32x32.png, 128x128.png, 128x128@2x.png and the hub's
  own icon, app/ui/src/assets/n8tray.png. The black outside its rounded
  corners is made transparent.
- n8builds-logo.png (the glowing logo) is the tray on a dark taskbar,
  tray.png: cropped to the mark, its black background keyed out.
- n8-monogram-light.png (cyan and navy) is the tray on a light taskbar,
  tray-light.png: already transparent, so only cropped.
"""

from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
ICONS = ROOT / "app/src-tauri/icons"
HUB = ROOT / "app/ui/src/assets/n8tray.png"

APP_SOURCE = HERE / "n8-flow-app.png"
# Outside the tile's border the source is black; its brightest channel stays
# below this there, while the blue border is far brighter.
OUTSIDE = 40
# The width (source px) of the border's anti-aliased edge into the black.
EDGE = 4

DARK_SOURCE = HERE / "n8builds-logo.png"
# The solid mark in the logo (its full-brightness pixels), measured once.
MARK = (228, 134, 1306, 954)
# The margin (source px) around the mark, and how far its glow reaches
# before it fades out: close, so it does not smear at 16 px.
MARGIN = 40
GLOW = 20
# Brightness of the brightest channel that counts as the mark, not its glow.
SOLID = 230
# The logo's "black" is a few levels above zero; below this is background.
FLOOR = 4

LIGHT_SOURCE = HERE / "n8-monogram-light.png"
# Its solid mark (alpha above half).
LIGHT_MARK = (219, 305, 1037, 970)
# Alpha below this is a speck of noise around the mark, not part of it.
SPECK = 8

ICO_SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256]


def brightest(img: Image.Image) -> Image.Image:
    r, g, b = img.convert("RGB").split()
    return ImageChops.lighter(ImageChops.lighter(r, g), b)


def unmultiply(rgb: tuple[int, int, int]) -> tuple[int, int, int, int]:
    """A pixel of light over black as colour and alpha: alpha is the
    brightest channel, the colour the pixel divided by it."""
    r, g, b = rgb
    m = max(r, g, b)
    if m <= FLOOR:
        return (0, 0, 0, 0)
    return (r * 255 // m, g * 255 // m, b * 255 // m, m)


def app_tile() -> Image.Image:
    """The Flow monogram with the black outside its rounded corners made
    transparent, following the tile's own edge."""
    src = Image.open(APP_SOURCE).convert("RGB")
    w, h = src.size
    # The outside is the dark area reached from the corners; the border
    # stops it, so the dark inside of the tile is never touched.
    dark = brightest(src).point(lambda v: 255 if v < OUTSIDE else 0)
    for corner in ((0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)):
        if dark.getpixel(corner) == 255:
            ImageDraw.floodfill(dark, corner, 128)
    outside = dark.point(lambda v: 255 if v == 128 else 0)
    # The border's anti-aliased edge is light over black too: there alpha
    # comes from the pixel itself, so no black fringe is left.
    edge = outside.filter(ImageFilter.MaxFilter(2 * EDGE + 1))
    out = src.convert("RGBA")
    sp, ep, op = src.load(), edge.load(), out.load()
    for y in range(h):
        for x in range(w):
            if ep[x, y]:
                op[x, y] = unmultiply(sp[x, y])
    return out


def dark_mark() -> Image.Image:
    """The glowing logo as a square RGBA image, black keyed out."""
    src = Image.open(DARK_SOURCE).convert("RGB")
    x0, y0, x1, y1 = MARK
    side = max(x1 - x0, y1 - y0) + 2 * MARGIN
    left = round((x0 + x1 - side) / 2)
    top = round((y0 + y1 - side) / 2)
    square = Image.new("RGB", (side, side), (0, 0, 0))
    square.paste(src, (-left, -top))
    # The glow is faded out along the mark's own outline (a blurred copy of
    # its solid shape), so it ends in a soft halo that follows the letters.
    solid = brightest(square).point(lambda v: 255 if v >= SOLID else 0)
    halo = solid.filter(ImageFilter.GaussianBlur(GLOW)).point(lambda v: min(255, v * 3))
    # It also reaches zero before the crop and before the source's own edge,
    # so neither shows as a line.
    edge = Image.new("L", (side, side), 0)
    inside = (max(0, -left), max(0, -top), min(side, src.width - left), min(side, src.height - top))
    ImageDraw.Draw(edge).rectangle([inside[0] + 1, inside[1] + 1, inside[2] - 2, inside[3] - 2], fill=255)
    edge = edge.filter(ImageFilter.BoxBlur(MARGIN / 4)).point(lambda v: max(0, 2 * v - 255))
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


def light_mark() -> Image.Image:
    """The cyan and navy monogram as a square RGBA image.

    The file already has a transparent background (it looks white only in a
    viewer), so it needs no keying: specks of near-zero alpha are dropped,
    the solid colours' alpha of 253-254 is made opaque, and it is cropped.
    """
    src = Image.open(LIGHT_SOURCE).convert("RGBA")
    a = src.getchannel("A").point(lambda v: 0 if v < SPECK else 255 if v >= 250 else v)
    src.putalpha(a)
    x0, y0, x1, y1 = LIGHT_MARK
    margin = round((x1 - x0) * MARGIN / (MARK[2] - MARK[0]))
    side = max(x1 - x0, y1 - y0) + 2 * margin
    left = round((x0 + x1 - side) / 2)
    top = round((y0 + y1 - side) / 2)
    return src.crop((left, top, left + side, top + side))


def resized(art: Image.Image, size: int) -> Image.Image:
    # Resize premultiplied so transparent pixels do not darken the edges.
    return art.convert("RGBa").resize((size, size), Image.LANCZOS).convert("RGBA")


def main() -> None:
    app = app_tile()
    ICONS.mkdir(parents=True, exist_ok=True)
    for name, size in (("32x32.png", 32), ("128x128.png", 128), ("128x128@2x.png", 256), ("icon.png", 512)):
        resized(app, size).save(ICONS / name)
    # Each .ico entry is made at its own size, not shrunk from the largest.
    images = [resized(app, s) for s in ICO_SIZES]
    images[-1].save(
        ICONS / "icon.ico",
        sizes=[(s, s) for s in ICO_SIZES],
        append_images=images[:-1],
        bitmap_format="png",
    )
    # The hub shows N8 Tray's own icon next to the apps' tiles (spec 3.11)
    # at up to 32 CSS px; 64 covers 200 percent.
    HUB.parent.mkdir(parents=True, exist_ok=True)
    resized(app, 64).save(HUB)
    # The tray's base images, for a dark and a light taskbar; the app draws
    # the level dot over the one in use (badge.rs).
    resized(dark_mark(), 64).save(ICONS / "tray.png")
    resized(light_mark(), 64).save(ICONS / "tray-light.png")
    print(f"wrote {ICONS.relative_to(ROOT)} and {HUB.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
