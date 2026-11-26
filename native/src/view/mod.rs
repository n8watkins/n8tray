//! What the screens share: the words and numbers of today's window
//! (`format`), the palette a window draws with (`theme`), and the widget
//! gallery of debug and test builds (`gallery`). The widgets themselves are
//! in `ui/widgets.slint` (see `ui/README.md`).

pub mod format;
#[cfg(any(debug_assertions, test))]
pub mod gallery;
pub mod theme;
