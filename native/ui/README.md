# N8 Tray's native widget kit

The native window must look exactly like today's N8 Tray (`app/ui`), with the changes the owner approved on 2026-11-25 and 26.
This folder holds the shared look every screen is built from: the theme tokens, the widgets and the gallery that shows them.
Build screens from these pieces only; if a screen needs something the kit lacks, add it to the kit (and the gallery) rather than drawing it in the screen.

## See the kit

- `cargo run --manifest-path native/Cargo.toml -- --gallery` opens the gallery in a window (debug builds only; on Windows it needs no `N8TRAY_HOME`).
- Its "Theme" links switch light, dark and High Contrast; `N8TRAY_THEME=light|dark|contrast` picks the first one.
- `n8tray-native --gallery-png <folder> [--scale 2]` writes every family and the whole gallery, in each theme, as PNG files, with the software renderer and no window.
- `cargo test` compares the whole gallery with `native/tests/goldens`; refresh them with `N8TRAY_UPDATE_GOLDENS=1 cargo test` when a change is meant, and look at the new pictures before committing them.

## Files

- `theme.slint`: the `Theme` global (every colour, size, radius and gap of `app/ui/src/style.css`), the `Level` and `Tone` enums and `SystemColors`.
- `widgets.slint`: import everything from here; the widgets themselves are in `widgets/`.
- `glyphs/`: the window's 16 x 16 glyphs (`Glyph.tsx`) as SVG, recoloured with `colorize`.
- `main.slint`: what the shell's Rust code sees (`crate::ui`).
- `window.slint`: the placeholder window of lane N0, until the hub screen replaces it.
- `gallery.slint`: the gallery, compiled into its own module and included only in debug and test builds.

## Tokens

- Never write a colour, a font size or a radius in a screen: use `Theme.bg`, `Theme.muted`, `Theme.text-small`, `Theme.radius-card` and the rest.
- Rust sets `Theme.dark`, `Theme.contrast` and `Theme.system` when a window is created, with `crate::view::theme::apply!(ui.global::<Theme>(), SystemColors, crate::view::theme::for_window(ui.window()))`.
- Under High Contrast every token already maps to the system's colours as a browser does under `forced-colors`: do not test `Theme.contrast` in a screen unless the CSS has a `forced-colors` rule for that part.
- `Theme.compact` is the Compact setting; widgets read it as the default of their own `compact` property, so a screen sets it once from the setting.
- The window is `Theme.window-width` (380) wide; the window-height setting's presets are `Theme.height-short` (400, the default), `height-medium` (500) and `height-tall` (600).
- Single lines of text use `Label` (a CSS line box: the font size times 1.4, elided); wrapping text uses `WrapLabel` with `line-height-factor: Theme.line-factor`.

## Widgets

- `TitleBar`: the hub's (logo, "N8 Tray", level pill, summary "3 of 3 · 1.9 GB" from `view::format::title_summary`, Settings, Close) and the crumbs' (`back: true`, an optional small app icon, the title elided, a pill); a panel's action buttons go in as children.
- `AlertLine`: the one amber line under the title bar when something is wrong.
- `AppRow`: an app's row, Normal or Compact, with its `RowAction` button; `static: true` for a manifest problem.
- `MoreLine`: "2 more · 1 running, 38 MB" from the snapshot's `more.text`, opening All apps.
- `Pill`, `Tag`, `AppIcon` (28, 24 small, 18 compact, with the level dot), `ProgressBar`.
- `IconButton`, `SmallButton` (both with `confirm: true` for a second click within 4 seconds), `LinkButton` and `LinkSwitch` (the bottom bar's), `HeadLink` (a section heading's link).
- `Banner` (`BannerKind` info, attention or problem; glyph, text, detail, progress, two buttons, an inline link that opens more text).
- `Section`, `SectionHeading`, `GroupHeading`, `Divider`, `Switch`, `ToggleRow`.
- `Spark` and `Chart` with `ChartLegend`: give them the path commands from `view::format::spark` and `view::format::chart`.
- `ScrollArea`: the one scrolling part of a window, with today's thin scrollbar; it reports `content-height` so the window can be as tall as its content up to the height limit.
- `TooltipLayer`: put one as the last child of every window, at its top left, filling it; any control with a `tooltip` then shows it.

## Rules for screens

- One window per screen file, in `ui/screens/<name>.slint`, named `<Name>Screen`.
- Each screen lane adds exactly one line to `main.slint` (`export { HubScreen } from "screens/hub.slint";`) and one line to `src/shell.rs` (the call that creates or fills its screen); everything else lives in the lane's own files.
- Names: components in PascalCase, properties and callbacks in kebab-case, one Rust module per screen under `src/view/`.
- Stack banners and sections inside a gutter of 10 px at the sides; sections are 6 px apart (CSS margins that collapse), banners 8 px.
- A list (`.list`) has padding 2px 6px 4px; rows go straight into it.
- Text comes from Rust: words and numbers through `view::format` (ported from `format.ts` and `model.ts`, with today's rounding), never formatted in Slint.
- Every control a screen adds must take the keyboard focus, show the focus ring and set `accessible-role`, `accessible-label` and its state: use `Pressable` inside it, as the kit's widgets do.
- Do not use `memory.freeGb`, `totalGb`, `level`, `system` or `text`: whole-PC memory is gone from N8 Tray.
- Show the real N8 icon (`app/src-tauri/icons/128x128.png`) wherever the app's own icon shows.
