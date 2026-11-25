# Icons

N8 Tray's icons are made from the owner's n8builds artwork; nothing is redrawn (D30 in `docs/DECISIONS.md`).
Each source is only cut out and resized:

| Source | Feeds | Treatment |
| --- | --- | --- |
| `n8-flow-app.png` (1254 x 1254, the Flow monogram on its bordered tile) | the app icon: `icon.ico`, `icon.png`, `32x32.png`, `128x128.png`, `128x128@2x.png`, and the hub's `app/ui/src/assets/n8tray.png` | the black outside the rounded corners made transparent |
| `n8builds-logo.png` (1536 x 1024, the glowing logo) | `tray.png`, the tray on a dark taskbar | cropped to the mark, black keyed out, glow kept close to the letters |
| `n8-monogram-light.png` (1254 x 1254, cyan and navy, already transparent) | `tray-light.png`, the tray on a light taskbar | cropped |

To make them again, from the repository root, with Pillow installed:

```sh
python3 tools/icons/make-icons.py
```

It writes `app/src-tauri/icons/` and the hub's icon:

- `icon.ico`: entries at 16, 20, 24, 32, 40, 48, 64, 128 and 256 px, each made at its own size;
- `32x32.png`, `128x128.png`, `128x128@2x.png` and `icon.png` (512 px), the files `tauri.conf.json` lists;
- `tray.png` and `tray-light.png` (64 px), the tray's base images, over which the app draws the level dot when something needs a look (`app/src-tauri/src/badge.rs`);
- `app/ui/src/assets/n8tray.png` (64 px), N8 Tray's own icon in the hub.

Python and Pillow are needed only to make these files; the app reads the PNG and ICO files and has no Python at run time.
