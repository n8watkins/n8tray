# Icons

N8 Tray's icons are made from the n8builds logo, `n8builds-logo.png` (1536 x 1024, the original artwork).
`make-icons.py` crops it to the "n8" mark, keys the black background out to transparency and resizes it; nothing is redrawn (D30 in `docs/DECISIONS.md`).
The tray on a light taskbar uses the cyan and navy monogram, `n8-monogram-light.png` (1254 x 1254, already transparent), cropped the same way.

To make them again, from the repository root, with Pillow installed:

```sh
python3 tools/icons/make-icons.py
```

It writes:

- `app/src-tauri/icons/icon.ico`: entries at 16, 20, 24, 32, 40, 48, 64, 128 and 256 px, each made at its own size;
- `app/src-tauri/icons/32x32.png`, `128x128.png`, `128x128@2x.png` and `icon.png` (512 px), the files `tauri.conf.json` lists;
- `app/src-tauri/icons/tray.png` and `tray-light.png` (64 px), the tray's base images for a dark and a light taskbar, over which the app draws the level dot when something needs a look (`app/src-tauri/src/badge.rs`);
- `app/ui/src/assets/n8tray.png` (64 px), N8 Tray's own icon in the hub.

Below 48 px the mark is bare on a transparent background; from 48 px up it sits on a dark rounded tile.
Python and Pillow are needed only to make these files; the app reads the PNG and ICO files and has no Python at run time.
