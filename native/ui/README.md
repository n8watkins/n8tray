# N8 Tray's native window and widget kit

The native window must look exactly like today's N8 Tray (`app/ui`), with the changes the owner approved on 2026-11-25 and 26.
This folder holds the shell (one window and its navigation), the shared look (theme tokens and widgets), one stub per view, and the gallery that shows them all.
Build views from these pieces only.

## See the kit

- `cargo run --manifest-path native/Cargo.toml -- --gallery` opens the gallery in a window (debug builds only; it needs no `N8TRAY_HOME`).
- Its "Theme" links switch light, dark and High Contrast; `N8TRAY_THEME=light|dark|contrast` picks the first one.
- `n8tray-native --gallery-png <folder> [--scale 2]` writes every family, every view's shots and the whole gallery, in each theme, as PNG files, with the software renderer and no window.
- `cargo test` compares each family with `native/tests/goldens/<family>/<theme>.png` and each view's shots with `native/tests/goldens/view-<name>/<state>-<theme>.png`.
  Refresh one folder with `N8TRAY_UPDATE_GOLDENS=view-hub cargo test`, all of them with `N8TRAY_UPDATE_GOLDENS=1`, and look at the new pictures before committing them.
- Release builds compile `ui/main.slint` alone; debug and test builds compile `ui/debug.slint`, which adds the gallery (`build.rs`).
  So the tests need debug assertions: `cargo test --release` stops with a compile error that says so.

## Ownership: who edits what

Six view lanes run in parallel.
Each edits only the files in its row; everything else is the kit, which the shell lane owns.

| Lane | Its files |
|---|---|
| Hub | `ui/views/hub.slint`, `ui/views/hub-model.slint`, `src/view/hub.rs`, `tests/goldens/view-hub/` |
| Pinned apps | `ui/views/all-apps.slint`, `ui/views/all-apps-model.slint`, `src/view/all_apps.rs`, `tests/goldens/view-all-apps/` |
| App panel | `ui/views/panel.slint`, `ui/views/panel-model.slint`, `src/view/panel.rs`, `tests/goldens/view-panel/` |
| Details | `ui/views/details.slint`, `ui/views/details-model.slint`, `src/view/details.rs`, `tests/goldens/view-details/` |
| Settings and About | `ui/views/settings.slint`, `ui/views/settings-model.slint`, `ui/views/about.slint`, `ui/views/about-model.slint`, `src/view/settings.rs`, `src/view/about.rs`, `tests/goldens/view-settings/`, `tests/goldens/view-about/` |
| History | `ui/views/history.slint`, `ui/views/history-model.slint`, `src/view/history.rs`, `tests/goldens/view-history/` |
| Shell and kit | everything else under `native/`, except `native/fixtures/` (read only) |

Every view is already wired in: its files are in `main.slint`, `shell.slint` and the gallery; its model global and module are declared; the router fills it, the window wires it, and the golden test renders its shots.
A view lane adds nothing to a shared file.

### A view's files

- `views/<name>-model.slint` holds what Rust fills: one global `<Name>Model`, and every struct and enum it uses.
  It holds no component.
  Every type it exports starts with the view's name in PascalCase (`HubLine`, `AllAppsGroup`, `PanelOpen`), so two lanes never export the same name through the chain: Slint only warns about a duplicate, and Rust then sees one of them (`view::router`'s tests check the prefix).
- `views/<name>.slint` holds one component `<Name>View` (its title bar, its `ScrollArea`, and for the hub its bottom bar), and any components only it uses.
- `src/view/<name>.rs` has `fill(ui: &dyn Models, ctx: &mut Ctx)`, called with the latest snapshot whenever the view shows or the snapshot changes, and `wire(ui: &dyn Models)`, called once per window to connect the global's callbacks.
  A callback asks for things through `view::intent` (below) and for other views through `Nav`.
  `ctx.arrived` is true on the first fill after the view began to show (opened, gone to, come back to by Back, or a new window), where today's view mounts: read a history then, or forget a "Copied" from an earlier visit; it is false on the refills of later snapshots.
  `ctx.warnings` are the recent warnings, newest first.
- `SHOTS` in the same module lists the view's golden pictures (`router::Shot`: a state name, a fixture, an app id, Compact or not, the height limit, and an optional `stage`).
  Add a state by adding a line; the golden test renders each in the shell's window, in light, dark and High Contrast.
  A state no fixture has comes from `stage`, a function in your module that changes the shot's copy of the snapshot, adds recent warnings or arms an action (`router::Stage`) before the fill; the fixtures stay as they are.
  Times in the pictures show in UTC on a 24-hour clock, so they match on every PC and runner.
- Its tests go in its module; helpers only one view needs go in its module too, not in `format.rs`.

### Your types reach Rust

`main.slint` exports `views/hub-model.slint` whole (`export * from`), and each model file exports the next one whole: hub, all-apps, panel, details, settings, about, history.
Slint allows one `export *` per file, so this chain is how a struct you declare in your model file becomes a plain `crate::ui::<Name>` in Rust.
Keep the line `export * from "<next>-model.slint";` at the top of your model file as it is (History's has none); `view::router`'s tests check the chain.
A struct declared in a view file instead of the model file reaches Rust only as a deprecated alias, which `-D warnings` refuses.
The hub stub's `HubLine` shows the pattern, and `view::hub`'s test uses it.

### Ask, do not add

Ask the shell lane for anything in the kit:

- a new widget, a new state of a widget, a new token or glyph;
- a change to a widget's look or behaviour, or to `format.rs`, `rows.rs`, `action.rs`, `confirm.rs`, `list.rs`, `icons.rs`, `intent.rs` or `theme.rs`;
- a new global the views share, a new field in a shared struct (`RowData`, `BarAction`, `TableRow` and the others);
- navigation: a new view, Back or Escape behaving differently, the window's size or placement;
- something a view needs from the host, the OS or the clipboard that `view::intent` does not offer.

### Definition of done for a view lane

- The view looks exactly like today's in light, dark and High Contrast, with the approved differences in its spec below; the side-by-side sheets show it.
- Its `SHOTS` cover its states (at least Normal and Compact, and each state its spec names); its golden images are refreshed and looked at.
- `npm run check:native` passes on Linux and Windows.
- Every control takes the keyboard focus, shows the ring and has its role, label and state; rows move with the arrow keys.
- Its module has a test calling `view::gallery::tests::tab_through` on a shot whose list scrolls.
- Lists are stable models updated in place by app id (`view::list::sync`); actions go through `Intent::Action`, which arms those that ask to confirm (`view::confirm`); after a sync that moved rows, `view::rows::refocus(focus, focused, model)` puts the focus back by id, in the same turn, and asks nothing for an app that left the list (a request no row takes expires after 250 ms).
- No shared file changed.

## What a view may ask for: `view::intent`

Views pass only ids and keys, never a path, a URL or a command line.
The shell turns them into a host command, or into a target the snapshot or N8 Tray's own folders name, with the Tauri app's validation (`commands.rs`), and refuses anything else.
Every function here may be called straight from a Slint callback: the work runs on the shell's next turn.
A reply comes back on the UI thread to a closure that gets the window's models (`&dyn Models`); it fills the view's model and calls nothing on the shell.

| Kind | Call | Example |
|---|---|---|
| A setting, N8 Tray's or an app's | `send(Intent::Setting { key, on, app })` | `Intent::Setting { key: "pinned".into(), on: false, app: Some(id) }`; keys: `notifications`, `checkUpdates`, `betaUpdates`, `compact` (no app), and `launchAtSignIn`, refused while the installed N8 Tray owns the sign-in entry (D33); `enabled`, `autoStart`, `hosted`, `pinned` (an app) |
| The window height | `send(Intent::WindowHeight(name))` | `Intent::WindowHeight("medium".into())`; `short`, `medium` or `tall` |
| A host command | `send(Intent::HostCommand(name))` | `Intent::HostCommand("checkUpdates".into())`; or `reloadSettings` |
| The footer's sign-in switch | `send(Intent::ToggleSignIn)` | turns Start at sign-in the other way, while the native shell may (D33) |
| An app's action | `send(Intent::Action { app, action, component })` | `Intent::Action { app: id, action: "stop".into(), component: None }`; armed by the first press when it asks to confirm, shown armed on the next fill (`ctx.confirms`) |
| One of N8 Tray's targets | `send(Intent::OpenHost(target))` | `Intent::OpenHost(HostTarget::Log)`; `Log`, `Folder`, `SettingsFile`, `ReleasePage` (`HostTarget::from_name("log")`) |
| An app's item | `send(Intent::OpenItem { app, kind, key })` | `Intent::OpenItem { app: id, kind: "open".into(), key: "logs".into() }`; `kind` `open` (by key) or `menu` (by index) |
| Install an update, or try again | `send(Intent::InstallUpdate)` | opens the release page until the native shell installs in place (D27) |
| A history | `history(of, range, done)` | `history(HistoryOf::App(id), Range::Day, \|ui, reply\| fill_chart(ui, reply))`; `HistoryOf::All` is every app shown in N8 Tray, then N8 Tray; read on a worker thread; compare `reply.of` and `reply.range` with what shows now |
| Recent warnings | `ctx.warnings` | `for w in ctx.warnings { .. }` in `fill`, newest first (`intent::warnings()` in the window; a shot's `stage` sets them in a picture) |
| The status text | `status_text(snapshot)` | what Copy status copies, today's `report.rs` |
| Copy status | `copy_status(done)` | `copy_status(\|ui, ok\| ui.details().set_copied(if ok { "Copied" } else { "Could not copy" }.into()))` |
| Copy any text | `copy(text, done)` | `copy(line, \|_, _\| {})` |

Navigation is `Nav.go(View, id)`, `Nav.back()` and `Nav.close()` in Slint.

## The views

Each lane copies one of today's files (`app/ui/src/views/`) and its helpers in `model.ts` and `format.ts` (ported in `view::format`).
"Today's" means the shipped window as it draws on 2026-11-26; where a spec says otherwise, the spec wins.
Fixtures for shots: `busy`, `healthy`, `trouble`, `watching`, `updating`, `updateFailed`, `empty`, `pinned` and `compactTall` in `native/fixtures/`.
Their app icons are relative to the spec's examples (`icons/voicedock.svg`, as today's preview reads them); `gallery::fixture` makes them absolute, so a shot shows the real icons with no stage of its own.

### Hub (copies `Hub.tsx`, the `HubHead` and `HubFoot` parts and `components/UpdateBanner.tsx`)

- Title bar: logo, "N8 Tray", the level pill (`format::level_words(level)`), the summary (`format::title_summary(pinnedRunning, pinnedTotal, memory.n8Mb)`, "3/4 · 1.9 GB", the owner's short form of 2026-11-27, read in full by a screen reader from `format::title_summary_label`), Settings, Close.
  The hub's bar is tighter than a crumb's (6 px between its parts, 8 px from the logo to the title) and the summary takes the room before the buttons, so it fits on one line beside "Needs attention"; `hub::tests::the_summary_fits_beside_the_pill_on_one_line` checks every shot (`TitleProbe.summary-cut`), with Segoe UI too on Windows.
- Under it, when something is wrong, the amber line with the reason only (`AlertLine`, `reason`); the summary stays in the title bar.
- The watching note (`legacyTray`) with its own "Learn more" link, the update banner (`format::update_line(update)`), then the apps.
- The apps in the main view: `format::main_view_apps(apps)` (those with `inMainView`), each an `AppRow` from `rows::row_data`, Start or Stop as today, no action while watching.
- A manifest that could not be read: a `ProblemRow` per `problems` entry, after the apps.
- The "more" line (`MoreLine`), words only, with `more.text`, only when `more` is present; it opens All apps (`Nav.go(View.all-apps, "")`).
- The divider, then N8 Tray's own row: an `AppRow` with the id `n8tray`, so it stays in the arrow-key order, line "This app", its private memory and window memory as today; it opens the status details.
- An empty hub says today's "No apps yet." line.
- Bottom bar: the sign-in box (`Intent::ToggleSignIn`, a box in both states, disabled with today's tooltip when `signInForeign` or a settings error, and refused by the shell while `tray::SIGN_IN_EDITABLE` is false, D33), History, Log.
- Approved differences: the more line and the main-view filter (D32); Compact rows (D35); the status card is gone, its words are in the title bar.

### Pinned apps: All apps (no counterpart today; approved 2026-11-25, D32)

- Title bar: Back, "All apps", Close.
- Two groups, "In the main view" and "Not in the main view" (`GroupHeading`), from `format::all_apps_groups(apps)`: every app `pins::in_hub` keeps, by `inMainView`, in the snapshot's order; a group with no app is left out.
- Each row is an `AppRow` with its Start or Stop as in the hub, and a Pin or Unpin action (`Intent::Setting { key: "pinned", .. }`): Unpin on a pinned app, Pin on an unpinned one.
  An app pulled into the main view by trouble is in the first group with Pin.
- A row opens the app's panel; Back from the panel returns to All apps.
- Hiding never changes what runs: Unpin does not stop the app.
- Shots: `pinned` at least, Normal and Compact.

### App panel (copies `Panel.tsx`, with `PanelHead`)

- Title bar: Back, the app's small icon, its name (elided), the pill with `pending` or `words`, its Start or Stop and Restart (`format::row_action`, `format::restart_action`), Close.
- Sections as today: State (`format::row_line`, the detail, `format::panel_figures`, GPU, description, notes, the app's other actions), Components (`ComponentRow` with `format::component_actions`), Last 15 minutes (`Chart` from `format::chart`, `chart_words`, with "24 hours, 7 days" opening this app's history), Processes (`Table`, `format::table_row_label`), Open (`OpenLinks`, `Intent::OpenItem`), and the app's settings (`ToggleRow`s: `enabled`, `autoStart`, `hosted`).
- `ctx.app` names the app; the router falls back to the hub when it leaves the snapshot.
- Approved differences: none beyond the kit's look.

### Details (copies `Details.tsx`)

- Title bar: Back, "Status details", Close.
- Overall (`KeyValues`) with "Copy status" (`copy_status`, then "Copied" or "Could not copy" for 2.5 s), Apps, Problems, Recent warnings (`WarningList` from `ctx.warnings`), and the times from `format::since` (the Windows clock setting).
- No whole-PC memory (D34).
- The tray's status line opens this view (`Shell::show_view`).

### Settings and About (copies `Settings.tsx`)

- Settings: Back, "Settings", Close; the N8 Tray group (`ToggleRow`s for `notifications`, `checkUpdates`, `betaUpdates`, and Check now with `Intent::HostCommand("checkUpdates")`), "Shown in N8 Tray" (`hosted` per app), and the files links (settings file, Reload, folder, log).
  Start at sign-in shows its state from `settings.launchAtSignIn` and is disabled, with today's tooltip wording for a locked entry, while `tray::SIGN_IN_EDITABLE` is false: `Intent::ToggleSignIn` and `Intent::Setting { key: "launchAtSignIn", .. }` are both refused then, so the native build never rewrites the installed N8 Tray's sign-in entry (D33).
- New, approved (D35): Compact, a switch (`Intent::Setting { key: "compact", .. }`, Normal by default), and Window height, a three-way choice Short (400, the default), Medium (500) and Tall (600) (`Intent::WindowHeight`).
  The shell applies both from the snapshot on every fill and reveal (`Theme.compact`, `Theme.height-limit`) and fits the window; the view only draws them from `settings.compact` and `settings.windowHeight`.
- About: Back, "About N8 Tray", Close; today's version line (`N8 Tray <version>`, the end of `Settings.tsx`) on a view of its own, with the release page (`Intent::OpenHost(HostTarget::ReleasePage)`) when an update names one.

### History (copies `History.tsx`)

- Title bar: Back, "Resource history", Close.
- The range buttons "24 hours" and "7 days" (toggle `SmallButton`s), then one section per app: `HistoryOf::All` (every app shown in N8 Tray, then N8 Tray), or one app when `ctx.app` names it (from the panel).
- Each section: the app's icon and name (`SectionHeading` `plain`), then "Reading" until the reply, the `Chart` and its legend, or "Nothing recorded in the last 24 hours." / "7 days."
- Read with `intent::history` when the view arrives (`ctx.arrived`, as `History.tsx` reads on mount) and when the range changes, not on every fill; drop a reply whose `of` or `range` is not what shows.
  The reply always comes, with no items when there is nothing to read.

## Tokens

- Never write a colour, a font size or a radius in a view: use `Theme.bg`, `Theme.muted`, `Theme.text-small`, `Theme.radius-card` and the rest.
- The window applies the theme (`Theme.dark`, `Theme.contrast`, `Theme.system`, `Theme.font-family`) every time it is revealed, and Compact and the height limit on every fill and reveal; a view never sets them.
- Under High Contrast every token already maps to the system's colours as a browser does under `forced-colors`; test `Theme.contrast` in a view only where the CSS has a `forced-colors` rule for that part.
- `Theme.compact` is the Compact setting and `Theme.height-limit` the window-height setting (`height-short` 400, the default, `height-medium` 500, `height-tall` 600).
- Gaps: `Theme.gutter` (10 px at the sides of banners and sections), `Theme.section-gap` (6 px), `Theme.banner-gap` (8 px).
  A list has padding 2px 6px 4px; rows go straight into it.
- Single lines of text use `Label` (a CSS line box, elided); wrapping text uses `WrapLabel`.
  Give a text in a row a least height (`min-height`), never a fixed `height`: a fixed height caps the whole row at one line.
- The font stack is "Segoe UI Variable Text", then "Segoe UI" (`view::theme::font`).
  Its digits are tabular except "1", which is narrower: figures that change are right-aligned in a column at least 50 px wide (`AppRow`, `ComponentRow`), so only their left edge can move, never what is beside them.

## Widgets

- `TitleBar`: the hub's (logo, "N8 Tray", level pill, summary from `view::format::title_summary`, Settings, Close) and the crumbs' (`back`, an optional small app icon, the title elided, a pill, `actions` as `BarAction`s from `view::action`).
- `BottomBar`: the sign-in box (`switch-*`) and `links` as `BarLink`s.
- `AlertLine`: the one amber line under the title bar when something is wrong; only what is wrong.
- `RowList` (a stable `[RowData]` model, rows built by `view::rows::row_data`), `AppRow`, `ProblemRow` (a manifest that could not be read; one line in Compact), `MoreLine`.
- `Pill`, `Tag`, `AppIcon` (28, 24 small, 18 compact, with the level dot), `ProgressBar`.
- `IconButton`, `SmallButton` (plain, primary, or a toggle that looks primary when `checked`), `LinkButton`, `LinkSwitch`, `TextLink`, `HeadLink`; armed buttons show `armed`, which Rust sets.
- `Banner` (`BannerKind` info, attention or problem; glyph, text, detail, progress, two buttons, a link that opens more text).
  With a `link`, pass the paragraph's `words` too (`format::words(text)`, set once, not on every fill): the words flow in 1.45 line boxes and the link follows the last one on its line, as today's inline "Learn more".
- `Section` (a labelled region), `SectionHeading` (with `has-links` for links at the right, or `plain` with an app icon for the history), `GroupHeading`, `Divider`, `Switch`, `ToggleRow`.
- `Say` with `SayLine`s, `ActionsRow`, `ComponentRow`, `OpenLinks`, `KeyValues` (values wrap), `WarningList`, `Table` (text cells wrap, number columns keep their width; row labels from `view::format::table_row_label`).
  A first cell's `small` follows the name on its line while both fit, as today's inline `<small>`, and goes under the name when not; the first column's least width is the longer of the two longest words, never more than the room the other columns leave in a section (`card-width`), so the table never runs past its card (a word wider than that breaks inside).
- `Spark`, `Chart` and `ChartLegend`: path commands from `view::format::spark` and `chart`, words from `chart_words`.
- `ScrollArea`: the one scrolling part of a view.
- `FocusProbe`: where the focused control is, for the keyboard test only; the headless platform's clock moves only when a test calls `gallery::advance`, so a smooth scroll or a timer ends exactly when the test says.
- `TooltipLayer` is the window's; any control or `TipArea` with a `tooltip` shows it once per hover, disabled controls included.

## Behaviour the kit gives every view

- Every open by the tray icon shows the hub, as today; the tray's status line opens the status details; either way Back leads only to the hub.
- A new view puts the keyboard focus at its top, where a screen reader says its title (`Nav.title`, which the router sets), and Tab goes on to its first control.
- Keyboard: every control takes the focus from Tab and shows the ring; `AppRow` moves with Up, Down, Home, End, Left and Right as `keyboard.ts` does (`RowFocus`), and Rust can focus a row by app id (`RowFocus.to-id`), even a row a reorder just made anew.
- Focusing a control in the list scrolls it into view, by the least distance that shows it; the title bar's and bottom bar's controls never scroll the list.
  The scroll is Slint's own (a newly focused item scrolls every `Flickable` around it into view, smoothly unless Windows' animations are off); the kit adds none, so there is one scroll, never two that add up.
- An action that asks to confirm is armed by the first click for 4 seconds, held by app, action and component in Rust, so a refresh or a reordered list keeps it on its app; any view change and hiding the window disarm everything.
- A panel (or one app's history) whose app leaves the snapshot falls back to the hub, and Back forgets views of apps that are gone.
- Escape goes back, or in the hub hides the window; Back returns to the view before.

## Rules for every view

- Words and numbers come from Rust (`view::format`, ported from `format.ts` and `model.ts` with today's rounding), never formatted in Slint.
- Every control is in a bar (the title bar or the bottom bar, `in-bar: true`) or inside the view's `ScrollArea` (`tab_through` checks it).
  Room below a list's last row (padding, the flyout's 6 px) goes inside the `ScrollArea`, as today's, so the last row scrolls up to its own room above the bottom bar.
- Do not use whole-PC memory (`memory.freeGb`, `memory.totalGb`, `memory.level`, `memory.system`): it is gone from N8 Tray (D34).
- Show the real N8 icon (`view::icons::n8tray()`, or `app/src-tauri/icons/128x128.png` in Slint) wherever the app's own icon shows.
