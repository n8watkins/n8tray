//! Where the window opens: 2 px from the tray icon, away from the taskbar,
//! inside the work area of the icon's monitor, on every taskbar edge, with an
//! auto-hidden taskbar and for an icon in the overflow flyout. The same rules
//! as the PowerShell tray (`Get-N8PopupPlacement`), as one pure function.
//! Every figure is in physical pixels of one coordinate space.

use serde::Serialize;

/// The gap between the window and the icon, the taskbar and the screen edges.
pub const GAP: i32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    pub fn right(&self) -> i32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.height
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }
}

/// The monitor edge the taskbar sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Edge {
    Bottom,
    Top,
    Left,
    Right,
}

/// Where the window goes, and the tallest it may be there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub x: i32,
    pub y: i32,
    pub edge: Edge,
    /// The height of the region the window must stay in.
    pub max_height: i32,
}

/// The taskbar's edge. A taskbar that is not auto-hidden takes exactly one
/// side off the work area, wherever the icon sits on it (a side taskbar's
/// icons sit near the bottom corner, where "nearest edge" would say bottom).
/// An auto-hidden taskbar (work area = monitor) falls back to the monitor
/// edge nearest the icon.
pub fn taskbar_edge(anchor: Rect, monitor: Rect, work: Rect) -> Edge {
    let insets = [
        (Edge::Bottom, monitor.bottom() - work.bottom()),
        (Edge::Top, work.y - monitor.y),
        (Edge::Right, monitor.right() - work.right()),
        (Edge::Left, work.x - monitor.x),
    ];
    let mut inset = insets.iter().filter(|(_, d)| *d > 0);
    if let (Some((edge, _)), None) = (inset.next(), inset.next()) {
        return *edge;
    }
    let distances = [
        (Edge::Bottom, monitor.bottom() - anchor.bottom()),
        (Edge::Top, anchor.y - monitor.y),
        (Edge::Right, monitor.right() - anchor.right()),
        (Edge::Left, anchor.x - monitor.x),
    ];
    distances
        .iter()
        .min_by_key(|(_, d)| *d)
        .map(|(e, _)| *e)
        .unwrap_or(Edge::Bottom)
}

/// Places a `width` x `height` window for the tray icon `anchor`.
///
/// Horizontal taskbars: the window's bottom-right corner (top-right at the
/// top) sits `gap` left of the icon, `gap` off the taskbar, so it grows
/// upward (downward at the top). Vertical taskbars: beside the taskbar,
/// ending `gap` above the icon. Always clamped into the work area less the
/// gap; a window too big for it keeps its top-left inside.
pub fn place(
    width: i32,
    height: i32,
    anchor: Rect,
    monitor: Rect,
    work: Rect,
    gap: i32,
) -> Placement {
    let edge = taskbar_edge(anchor, monitor, work);
    let mut left = work.x + gap;
    let mut top = work.y + gap;
    let mut right = work.right() - gap;
    let mut bottom = work.bottom() - gap;

    // An auto-hidden taskbar is not outside the work area, so when the icon
    // sits in its band (close to the edge) the band is taken as twice the
    // icon's distance from the edge plus the icon, since the icon is centered
    // in it. An icon in the overflow flyout is farther away, and the window
    // then simply clears the icon itself.
    match edge {
        Edge::Bottom => {
            let distance = monitor.bottom() - anchor.bottom();
            let band = if distance <= anchor.height {
                anchor.y - distance
            } else {
                anchor.y
            };
            bottom = bottom.min(band - gap);
        }
        Edge::Top => {
            let distance = anchor.y - monitor.y;
            let band = if distance <= anchor.height {
                anchor.bottom() + distance
            } else {
                anchor.bottom()
            };
            top = top.max(band + gap);
        }
        Edge::Right => {
            let distance = monitor.right() - anchor.right();
            let band = if distance <= anchor.width {
                anchor.x - distance
            } else {
                anchor.x
            };
            right = right.min(band - gap);
        }
        Edge::Left => {
            let distance = anchor.x - monitor.x;
            let band = if distance <= anchor.width {
                anchor.right() + distance
            } else {
                anchor.right()
            };
            left = left.max(band + gap);
        }
    }

    let (x, y) = match edge {
        Edge::Bottom => (anchor.x - gap - width, bottom - height),
        Edge::Top => (anchor.x - gap - width, top),
        Edge::Right => (right - width, anchor.y - gap - height),
        Edge::Left => (left, anchor.y - gap - height),
    };
    let x = x.min(right - width).max(left);
    let y = y.min(bottom - height).max(top);
    Placement {
        x,
        y,
        edge,
        max_height: (bottom - top).max(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A 1920 x 1080 monitor; the taskbar is 48 px (icons 24 px, centered).
    const MONITOR: Rect = Rect::new(0, 0, 1920, 1080);
    const W: i32 = 380;
    const H: i32 = 500;

    fn check(p: Placement, work: Rect) {
        assert!(
            p.x >= work.x + GAP && p.x + W <= work.right() - GAP,
            "{p:?}"
        );
        assert!(
            p.y >= work.y + GAP && p.y + H <= work.bottom() - GAP,
            "{p:?}"
        );
    }

    #[test]
    fn bottom_taskbar_opens_above_and_left_of_the_icon() {
        let work = Rect::new(0, 0, 1920, 1032);
        let icon = Rect::new(1700, 1044, 24, 24);
        let p = place(W, H, icon, MONITOR, work, GAP);
        assert_eq!(p.edge, Edge::Bottom);
        assert_eq!(p.x + W, icon.x - GAP, "right edge 2 px left of the icon");
        assert_eq!(
            p.y + H,
            work.bottom() - GAP,
            "bottom 2 px above the taskbar"
        );
        assert_eq!(p.max_height, 1032 - 2 * GAP);
        check(p, work);
    }

    #[test]
    fn top_taskbar_opens_below() {
        let work = Rect::new(0, 48, 1920, 1032);
        let icon = Rect::new(1700, 12, 24, 24);
        let p = place(W, H, icon, MONITOR, work, GAP);
        assert_eq!(p.edge, Edge::Top);
        assert_eq!(p.x + W, icon.x - GAP);
        assert_eq!(p.y, 48 + GAP);
        check(p, work);
    }

    #[test]
    fn right_taskbar_opens_beside_it_ending_above_the_icon() {
        let work = Rect::new(0, 0, 1872, 1080);
        let icon = Rect::new(1884, 900, 24, 24);
        let p = place(W, H, icon, MONITOR, work, GAP);
        assert_eq!(p.edge, Edge::Right);
        assert_eq!(p.x + W, 1872 - GAP);
        assert_eq!(p.y + H, icon.y - GAP);
        check(p, work);
    }

    #[test]
    fn left_taskbar_opens_beside_it() {
        let work = Rect::new(48, 0, 1872, 1080);
        let icon = Rect::new(12, 900, 24, 24);
        let p = place(W, H, icon, MONITOR, work, GAP);
        assert_eq!(p.edge, Edge::Left);
        assert_eq!(p.x, 48 + GAP);
        assert_eq!(p.y + H, icon.y - GAP);
        check(p, work);
    }

    #[test]
    fn icon_near_the_top_of_a_side_taskbar_is_clamped_into_the_work_area() {
        let work = Rect::new(0, 0, 1872, 1080);
        let icon = Rect::new(1884, 100, 24, 24);
        let p = place(W, H, icon, MONITOR, work, GAP);
        assert_eq!(p.y, GAP);
        check(p, work);
    }

    #[test]
    fn auto_hidden_bottom_taskbar_clears_its_band() {
        let icon = Rect::new(1700, 1044, 24, 24);
        let p = place(W, H, icon, MONITOR, MONITOR, GAP);
        assert_eq!(p.edge, Edge::Bottom);
        // The band is the icon plus twice its distance from the edge.
        let band_top = icon.y - (1080 - icon.bottom());
        assert_eq!(p.y + H, band_top - GAP);
    }

    #[test]
    fn auto_hidden_side_taskbars_use_the_nearest_edge() {
        let left = place(W, H, Rect::new(12, 900, 24, 24), MONITOR, MONITOR, GAP);
        assert_eq!(left.edge, Edge::Left);
        assert_eq!(left.x, 12 + 24 + 12 + GAP);
        let top = place(W, H, Rect::new(1700, 12, 24, 24), MONITOR, MONITOR, GAP);
        assert_eq!(top.edge, Edge::Top);
        assert_eq!(top.y, 12 + 24 + 12 + GAP);
    }

    #[test]
    fn overflow_flyout_icon_opens_above_and_left_of_it() {
        // The flyout sits above the taskbar; its icon is inside the work area.
        let work = Rect::new(0, 0, 1920, 1032);
        let icon = Rect::new(1640, 960, 32, 32);
        let p = place(W, H, icon, MONITOR, work, GAP);
        assert_eq!(p.edge, Edge::Bottom);
        assert_eq!(p.x + W, icon.x - GAP);
        assert_eq!(p.y + H, icon.y - GAP, "above the flyout icon");
    }

    #[test]
    fn a_second_monitor_left_of_the_primary_keeps_its_own_space() {
        let monitor = Rect::new(-2560, 0, 2560, 1440);
        let work = Rect::new(-2560, 0, 2560, 1392);
        let icon = Rect::new(-200, 1404, 24, 24);
        let p = place(W, H, icon, monitor, work, GAP);
        assert_eq!(p.x + W, icon.x - GAP);
        assert_eq!(p.y + H, 1392 - GAP);
    }

    #[test]
    fn an_icon_at_the_far_left_still_fits_on_screen() {
        let work = Rect::new(0, 0, 1920, 1032);
        let icon = Rect::new(100, 1044, 24, 24);
        let p = place(W, H, icon, MONITOR, work, GAP);
        assert_eq!(p.x, GAP);
        check(p, work);
    }

    #[test]
    fn a_window_taller_than_the_space_keeps_its_top_inside() {
        let work = Rect::new(0, 0, 1920, 1032);
        let icon = Rect::new(1700, 1044, 24, 24);
        let p = place(W, 2000, icon, MONITOR, work, GAP);
        assert_eq!(p.y, GAP);
    }
}
