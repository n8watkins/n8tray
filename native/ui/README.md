# N8 Tray's native window and widget kit

The native window must look exactly like today's N8 Tray (`app/ui`), with the changes the owner approved on 2026-11-25 and 26.
This folder holds the shell (one window and its navigation), the shared look (theme tokens and widgets), one stub per view, and the gallery that shows them all.
Build views from these pieces only.

## See the kit

- `cargo run --manifest-path native/Cargo.toml -- --gallery` opens the gallery in a window (debug builds only; it needs no `N8TRAY_HOME`).
- Its "Theme" links switch light, dark and High Contrast; `N8TRAY_THEME=light|dark|contrast` picks the first one.
- `n8tray-native --gallery-png <folder> [--scale 2]` writes every family, every view and the whole gallery, in each theme, as PNG files, with the software renderer and no window.
- `cargo test` compares each family and view with its golden images in `native/tests/goldens/<family>/<theme>.png`.
  Refresh one family with `N8TRAY_UPDATE_GOLDENS=view-hub cargo test`, all of them with `N8TRAY_UPDATE_GOLDENS=1`, and look at the new pictures before committing them.

## Ownership: who edits what

Five view lanes run in parallel.
Each edits only the files in its row; everything else is the kit, which the shell lane owns.

| Lane | Its files |
|---|---|
| Hub | `ui/views/hub.slint`, `src/view/hub.rs`, `tests/goldens/view-hub/` |
| Pinned apps | `ui/views/all-apps.slint`, `src/view/all_apps.rs`, `tests/goldens/view-all-apps/` |
| App panel | `ui/views/panel.slint`, `src/view/panel.rs`, `tests/goldens/view-panel/` |
| Details | `ui/views/details.slint`, `src/view/details.rs`, `tests/goldens/view-details/` |
| Settings and About | `ui/views/settings.slint`, `ui/views/about.slint`, `src/view/settings.rs`, `src/view/about.rs`, `tests/goldens/view-settings/`, `tests/goldens/view-about/` |
| History | `ui/views/history.slint`, `src/view/history.rs`, `tests/goldens/view-history/` |
| Shell and kit | everything else under `native/ui/` and `native/src/view/`, `src/window.rs`, `tests/goldens/` outside the view folders |

Every view is already wired in: its `.slint` file is in `main.slint`, `shell.slint` and the gallery; its model global and module are declared; the router fills it and the window wires it.
A view lane adds nothing to a shared file.

A view's `.slint` file exports one global `<Name>Model` (what Rust fills) and one component `<Name>View` (its title bar, its `ScrollArea`, and for the hub its bottom bar).
Its Rust module has `fill(ui: &dyn Models, ctx: &mut Ctx)`, called with the latest snapshot whenever it shows or the snapshot changes, and `wire(ui: &dyn Models)`, called once per window to connect the global's callbacks (each callback hands its work to the shell with `crate::shell::defer`).
Its tests go in its module, its golden images in its folder; the gallery shows the view on the busy fixture.
Helpers only one view needs go in its own module, not in `format.rs`.

### Ask, do not add

Ask the shell lane for anything in the kit:

- a new widget, a new state of a widget, a new token or glyph;
- a change to a widget's look or behaviour, or to `format.rs`, `rows.rs`, `action.rs`, `confirm.rs`, `list.rs`, `icons.rs` or `theme.rs`;
- a new global the views share, a new field in a shared struct (`RowData`, `BarAction`, `TableRow` and the others);
- navigation: a new view, Back or Escape behaving differently, the window's size or placement.

### Definition of done for a view lane

- The view looks exactly like today's in light, dark and High Contrast, with the approved changes; the side-by-side sheets show it.
- Its golden images are refreshed and looked at; `npm run check:native` passes on Linux and Windows.
- Every control takes the keyboard focus, shows the ring and has its role, label and state; rows move with the arrow keys.
- Lists are stable models updated in place by app id (`view::list::sync`); armed actions come from `view::confirm` through `Hub::press_action`; the focus is put back by id after a reorder (`view::rows::refocus`).
- No shared file changed.

## Tokens

- Never write a colour, a font size or a radius in a view: use `Theme.bg`, `Theme.muted`, `Theme.text-small`, `Theme.radius-card` and the rest.
- The window applies the theme (`Theme.dark`, `Theme.contrast`, `Theme.system`, `Theme.font-family`) every time it is revealed; a view never sets it.
- Under High Contrast every token already maps to the system's colours as a browser does under `forced-colors`; test `Theme.contrast` in a view only where the CSS has a `forced-colors` rule for that part.
- `Theme.compact` is the Compact setting and `Theme.height-limit` the window-height setting (`height-short` 400, the default, `height-medium` 500, `height-tall` 600).
- Gaps: `Theme.gutter` (10 px at the sides of banners and sections), `Theme.section-gap` (6 px), `Theme.banner-gap` (8 px).
  A list has padding 2px 6px 4px; rows go straight into it.
- Single lines of text use `Label` (a CSS line box, elided); wrapping text uses `WrapLabel`.
- The font stack is "Segoe UI Variable Text", then "Segoe UI" (`view::theme::font`).
  Its digits are tabular except "1", which is narrower: figures that change are right-aligned in a column at least 50 px wide (`AppRow`, `ComponentRow`), so only their left edge can move, never what is beside them.

## Widgets

- `TitleBar`: the hub's (logo, "N8 Tray", level pill, summary from `view::format::title_summary`, Settings, Close) and the crumbs' (`back`, an optional small app icon, the title elided, a pill, `actions` as `BarAction`s from `view::action`).
- `BottomBar`: the sign-in box (`switch-*`) and `links` as `BarLink`s.
- `AlertLine`: the one amber line under the title bar when something is wrong; only what is wrong.
- `RowList` (a stable `[RowData]` model, rows built by `view::rows::row_data`), `AppRow`, `ProblemRow` (a manifest that could not be read), `MoreLine`.
- `Pill`, `Tag`, `AppIcon` (28, 24 small, 18 compact, with the level dot), `ProgressBar`.
- `IconButton`, `SmallButton` (plain, primary, or a toggle that looks primary when `checked`), `LinkButton`, `LinkSwitch`, `TextLink`, `HeadLink`; armed buttons show `armed`, which Rust sets.
- `Banner` (`BannerKind` info, attention or problem; glyph, text, detail, progress, two buttons, a link that opens more text).
- `Section` (a labelled region), `SectionHeading` (with `has-links` for links at the right, or `plain` with an app icon for the history), `GroupHeading`, `Divider`, `Switch`, `ToggleRow`.
- `Say` with `SayLine`s, `ActionsRow`, `ComponentRow`, `OpenLinks`, `KeyValues`, `WarningList`, `Table` (row labels from `view::format::table_row_label`).
- `Spark`, `Chart` and `ChartLegend`: path commands from `view::format::spark` and `chart`, words from `chart_words`.
- `ScrollArea`: the one scrolling part of a view.
- `TooltipLayer` is the window's; any control or `TipArea` with a `tooltip` shows it once per hover, disabled controls included.

## Behaviour the kit gives every view

- Keyboard: every control takes the focus from Tab and shows the ring; `AppRow` moves with Up, Down, Home, End, Left and Right as `keyboard.ts` does (`RowFocus`), and Rust can focus a row by app id (`RowFocus.to-id`).
- Focusing a control in the list scrolls it into view; the title bar's and bottom bar's controls never scroll the list.
- An action that asks to confirm is armed by the first click for 4 seconds, held by app and action id in Rust, so a refresh or a reordered list keeps it on its app.
- Escape goes back, or in the hub hides the window; Back returns to the view before.

## Rules for every view

- Words and numbers come from Rust (`view::format`, ported from `format.ts` and `model.ts` with today's rounding), never formatted in Slint.
- Do not use whole-PC memory (`memory.freeGb`, `memory.totalGb`, `memory.level`, `memory.system`, `memory.text`): it is gone from N8 Tray.
- Show the real N8 icon (`view::icons::n8tray()`, or `app/src-tauri/icons/128x128.png` in Slint) wherever the app's own icon shows.
