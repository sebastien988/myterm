//! Pure geometry and hit-testing for the tab strip and scrollbar.
//!
//! These decisions used to live inline in [`crate::ui`], which made the
//! awkward cases (tiny windows, a full strip of tabs, a click on the close
//! button) hard to test. They are kept free of egui so a table of inputs is
//! enough.

pub const TAB_HEIGHT: f32 = 26.0;
pub const TAB_GAP: f32 = 4.0;
pub const TAB_MAX_WIDTH: f32 = 220.0;
pub const TAB_MIN_WIDTH: f32 = 28.0;
pub const ICON_SIZE: f32 = 26.0;
pub const SCROLLBAR_WIDTH: f32 = 10.0;
/// Close is hidden below this width so the title (and the activity dot)
/// stay hittable.
pub const TAB_CLOSE_MIN_WIDTH: f32 = 70.0;
const THUMB_MIN: f32 = 24.0;

/// Space kept clear on the right of the strip for the new-tab and settings
/// buttons, including the gaps around them.
pub fn tab_bar_reserved() -> f32 {
    ICON_SIZE * 2.0 + TAB_GAP * 3.0
}

/// Width of each tab given the space left of the chrome buttons.
pub fn tab_width(available: f32, count: usize) -> f32 {
    let n = count.max(1) as f32;
    let usable = available.max(TAB_MIN_WIDTH);
    ((usable - TAB_GAP * (n - 1.0)) / n).clamp(TAB_MIN_WIDTH, TAB_MAX_WIDTH)
}

/// The close button only appears when it can be hit comfortably.
pub fn show_tab_close(width: f32, is_active: bool, hovered: bool) -> bool {
    width > TAB_CLOSE_MIN_WIDTH && (is_active || hovered)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabClick {
    Activate,
    Close,
}

/// Decide what a pointer press on a tab should do.
///
/// Middle-click always closes. A primary click on the close glyph closes;
/// anywhere else on the tab activates it.
pub fn tab_click(
    middle: bool,
    primary: bool,
    close_visible: bool,
    pointer_in_close: bool,
) -> Option<TabClick> {
    if middle || (primary && close_visible && pointer_in_close) {
        Some(TabClick::Close)
    } else if primary {
        Some(TabClick::Activate)
    } else {
        None
    }
}

/// Next tab after a wheel tick over the strip. `None` when there is nothing
/// to cycle (one tab, or a movement too small to count as a notch).
pub fn tab_after_scroll(active: usize, count: usize, scroll_y: f32) -> Option<usize> {
    if count <= 1 || scroll_y.abs() < 1.0 {
        return None;
    }
    let delta = if scroll_y > 0.0 { -1 } else { 1 };
    Some((active as i64 + delta).rem_euclid(count as i64) as usize)
}

/// Title shown on a tab: the shell's OSC title, unless it is empty.
pub fn tab_display_title<'a>(title: Option<&'a str>, label: &'a str) -> &'a str {
    match title {
        Some(title) if !title.trim().is_empty() => title,
        _ => label,
    }
}

/// Hover text for a tab: the full title, plus an exit status when the shell
/// has already died.
pub fn tab_hover_text(title: &str, exited: Option<i32>) -> String {
    match exited {
        Some(code) => format!("{title} — exited with code {code}"),
        None => title.to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollbarThumb {
    pub height: f32,
    /// Distance from the top of the track to the top of the thumb.
    pub top: f32,
}

/// Map scrollback state onto a thumb inside a track of `track_height`.
///
/// `offset` is alacritty's `display_offset`: 0 is the live bottom of the
/// buffer, `history` is scrolled all the way up. Returns `None` when there
/// is no history (or no track) to represent.
pub fn scrollbar_thumb(
    history: usize,
    offset: usize,
    screen: usize,
    track_height: f32,
) -> Option<ScrollbarThumb> {
    if history == 0 || track_height <= 0.0 {
        return None;
    }
    let total = (history + screen.max(1)) as f32;
    let min_thumb = THUMB_MIN.min(track_height);
    let height = (track_height * screen.max(1) as f32 / total).clamp(min_thumb, track_height);
    let travel = track_height - height;
    let offset = offset.min(history);
    let position = 1.0 - offset as f32 / history as f32;
    Some(ScrollbarThumb {
        height,
        top: travel * position,
    })
}

/// Pointer Y relative to the track top → a `display_offset`.
pub fn scrollbar_offset_from_pointer(
    history: usize,
    track_height: f32,
    thumb_height: f32,
    pointer_y: f32,
) -> usize {
    if history == 0 {
        return 0;
    }
    let travel = (track_height - thumb_height).max(1.0);
    let wanted = ((pointer_y - thumb_height / 2.0) / travel).clamp(0.0, 1.0);
    ((1.0 - wanted) * history as f32).round() as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_tab_uses_the_max_width_when_there_is_room() {
        assert_eq!(tab_width(800.0, 1), TAB_MAX_WIDTH);
        assert_eq!(tab_width(100.0, 1), 100.0);
    }

    #[test]
    fn many_tabs_share_the_strip_and_never_go_below_the_minimum() {
        let width = tab_width(400.0, 3);
        let expected = (400.0 - TAB_GAP * 2.0) / 3.0;
        assert!((width - expected).abs() < f32::EPSILON);
        assert_eq!(tab_width(40.0, 8), TAB_MIN_WIDTH);
        assert!(tab_width(500.0, 0) >= TAB_MIN_WIDTH);
    }

    #[test]
    fn reserved_chrome_leaves_room_for_both_icon_buttons() {
        assert!(tab_bar_reserved() > ICON_SIZE * 2.0);
    }

    #[test]
    fn close_only_shows_when_the_tab_is_wide_enough_and_in_play() {
        assert!(!show_tab_close(70.0, true, true));
        assert!(show_tab_close(70.1, true, false));
        assert!(show_tab_close(120.0, false, true));
        assert!(!show_tab_close(120.0, false, false));
    }

    #[test]
    fn middle_click_and_the_close_glyph_close_the_tab() {
        assert_eq!(tab_click(true, false, false, false), Some(TabClick::Close));
        assert_eq!(tab_click(false, true, true, true), Some(TabClick::Close));
        assert_eq!(
            tab_click(false, true, true, false),
            Some(TabClick::Activate)
        );
        assert_eq!(
            tab_click(false, true, false, true),
            Some(TabClick::Activate)
        );
        assert_eq!(tab_click(false, false, true, true), None);
    }

    #[test]
    fn wheel_over_the_strip_walks_and_wraps() {
        assert_eq!(tab_after_scroll(0, 3, 4.0), Some(2));
        assert_eq!(tab_after_scroll(2, 3, -3.0), Some(0));
        assert_eq!(tab_after_scroll(1, 3, 0.2), None);
        assert_eq!(tab_after_scroll(0, 1, 10.0), None);
        assert_eq!(tab_after_scroll(0, 0, 10.0), None);
    }

    #[test]
    fn display_title_falls_back_when_the_shell_has_nothing_to_say() {
        assert_eq!(tab_display_title(Some("vim"), "bash"), "vim");
        assert_eq!(tab_display_title(Some("   "), "bash"), "bash");
        assert_eq!(tab_display_title(None, "bash"), "bash");
    }

    #[test]
    fn hover_text_mentions_an_exit_status() {
        assert_eq!(tab_hover_text("bash", None), "bash");
        assert_eq!(
            tab_hover_text("bash", Some(127)),
            "bash — exited with code 127"
        );
    }

    #[test]
    fn scrollbar_thumb_sits_at_the_bottom_when_following_the_live_output() {
        let thumb = scrollbar_thumb(100, 0, 20, 200.0).expect("thumb");
        assert!((thumb.top + thumb.height - 200.0).abs() < 0.01);
    }

    #[test]
    fn scrollbar_thumb_sits_at_the_top_when_scrolled_all_the_way_up() {
        let thumb = scrollbar_thumb(100, 100, 20, 200.0).expect("thumb");
        assert!(thumb.top.abs() < 0.01);
    }

    #[test]
    fn scrollbar_is_hidden_without_history() {
        assert!(scrollbar_thumb(0, 0, 20, 200.0).is_none());
        assert!(scrollbar_thumb(10, 0, 20, 0.0).is_none());
    }

    #[test]
    fn a_tiny_track_does_not_panic_when_clamping_the_thumb() {
        let thumb = scrollbar_thumb(50, 0, 10, 10.0).expect("thumb");
        assert!(thumb.height <= 10.0);
        assert!(thumb.height > 0.0);
    }

    #[test]
    fn dragging_the_track_maps_to_the_matching_offset() {
        let thumb = scrollbar_thumb(100, 0, 20, 200.0).expect("thumb");
        assert_eq!(
            scrollbar_offset_from_pointer(100, 200.0, thumb.height, 0.0),
            100
        );
        assert_eq!(
            scrollbar_offset_from_pointer(100, 200.0, thumb.height, 200.0),
            0
        );
        assert_eq!(
            scrollbar_offset_from_pointer(0, 200.0, thumb.height, 10.0),
            0
        );
    }
}
