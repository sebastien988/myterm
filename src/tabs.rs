//! Tab bookkeeping.
//!
//! Generic over the per-tab payload so the index arithmetic — the part that is
//! easy to get subtly wrong — can be exercised without spawning a PTY.

/// Title shown before the shell reports one of its own.
pub const DEFAULT_TITLE: &str = "shell";

/// How much of a tab title stays on screen. Shell titles are often full paths
/// or command lines; without a cap a single tab pushes the rest of the strip,
/// and the `+` button, out of the window.
pub const MAX_TITLE_CHARS: usize = 24;

pub type TabId = u64;

pub struct Tab<T> {
    pub id: TabId,
    pub title: String,
    pub payload: T,
}

pub struct Tabs<T> {
    tabs: Vec<Tab<T>>,
    active: usize,
    next_id: TabId,
}

impl<T> Default for Tabs<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Tabs<T> {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active: 0,
            next_id: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn active(&self) -> Option<&Tab<T>> {
        self.tabs.get(self.active)
    }

    pub fn active_mut(&mut self) -> Option<&mut Tab<T>> {
        self.tabs.get_mut(self.active)
    }

    pub fn get(&self, index: usize) -> Option<&Tab<T>> {
        self.tabs.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut Tab<T>> {
        self.tabs.get_mut(index)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Tab<T>> {
        self.tabs.iter()
    }

    pub fn index_of(&self, id: TabId) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id == id)
    }

    /// Builds a payload from a freshly allocated id, appends it as the active
    /// tab and returns the id.
    ///
    /// The id has to exist before the payload does — a [`TerminalBackend`] is
    /// constructed with the id it will tag its events with — so the payload is
    /// built through a closure. A failed build consumes no id.
    ///
    /// [`TerminalBackend`]: egui_term::TerminalBackend
    pub fn push_with<E>(
        &mut self,
        title: impl Into<String>,
        make: impl FnOnce(TabId) -> Result<T, E>,
    ) -> Result<TabId, E> {
        let id = self.next_id;
        let payload = make(id)?;
        self.next_id += 1;
        self.tabs.push(Tab {
            id,
            title: title.into(),
            payload,
        });
        self.active = self.tabs.len() - 1;
        Ok(id)
    }

    /// Removes the tab at `index` and returns it, keeping the tab the user was
    /// looking at selected where it still exists.
    pub fn close(&mut self, index: usize) -> Option<Tab<T>> {
        if index >= self.tabs.len() {
            return None;
        }

        let removed = self.tabs.remove(index);

        // Everything after `index` shifted down a slot, so the active index has
        // to follow it to stay on the same tab. Closing the active tab leaves
        // the index alone, which selects whatever slid into its place.
        if index < self.active {
            self.active -= 1;
        }
        self.active = self.active.min(self.tabs.len().saturating_sub(1));

        Some(removed)
    }

    pub fn select(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active = index;
        }
    }

    pub fn select_next(&mut self) {
        if !self.tabs.is_empty() {
            self.active = (self.active + 1) % self.tabs.len();
        }
    }

    pub fn select_prev(&mut self) {
        if !self.tabs.is_empty() {
            self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
        }
    }
}

/// Renders a shell-reported title as tab-strip text: control characters
/// flattened to spaces, trimmed, and capped at `max_chars`.
pub fn display_title(title: &str, max_chars: usize) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let cleaned = cleaned.trim();

    if cleaned.is_empty() {
        return DEFAULT_TITLE.to_string();
    }
    if cleaned.chars().count() <= max_chars {
        return cleaned.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }

    let mut truncated: String = cleaned.chars().take(max_chars - 1).collect();
    truncated.push('…');
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tabs carrying a name instead of a PTY, so selection can be asserted by
    /// identity rather than index.
    fn tabs(names: &[&str]) -> Tabs<&'static str> {
        let mut tabs = Tabs::new();
        for name in names {
            let name: &'static str = Box::leak(name.to_string().into_boxed_str());
            tabs.push_with::<()>(name, |_| Ok(name)).unwrap();
        }
        tabs
    }

    fn active(tabs: &Tabs<&'static str>) -> Option<&'static str> {
        tabs.active().map(|tab| tab.payload)
    }

    #[test]
    fn a_new_tab_becomes_active() {
        let tabs = tabs(&["a", "b", "c"]);
        assert_eq!(tabs.iter().count(), 3);
        assert_eq!(active(&tabs), Some("c"));
    }

    #[test]
    fn ids_are_unique_and_resolvable() {
        let mut tabs: Tabs<&'static str> = Tabs::new();
        let first = tabs.push_with::<()>("a", |_| Ok("a")).unwrap();
        let second = tabs.push_with::<()>("b", |_| Ok("b")).unwrap();

        assert_ne!(first, second);
        assert_eq!(tabs.index_of(first), Some(0));
        assert_eq!(tabs.index_of(second), Some(1));
        assert_eq!(tabs.index_of(9999), None);
    }

    #[test]
    fn a_failed_spawn_adds_nothing_and_burns_no_id() {
        let mut tabs: Tabs<&'static str> = Tabs::new();
        assert!(tabs.push_with::<&str>("a", |_| Err("boom")).is_err());
        assert!(tabs.is_empty());

        let id = tabs.push_with::<()>("b", |_| Ok("b")).unwrap();
        assert_eq!(id, 0);
    }

    #[test]
    fn closing_a_tab_left_of_the_active_one_keeps_the_same_tab_selected() {
        let mut tabs = tabs(&["a", "b", "c"]);
        tabs.select(2);

        tabs.close(0);

        assert_eq!(active(&tabs), Some("c"));
        assert_eq!(tabs.active_index(), 1);
    }

    #[test]
    fn closing_the_active_tab_selects_the_one_that_takes_its_place() {
        let mut tabs = tabs(&["a", "b", "c"]);
        tabs.select(1);

        tabs.close(1);

        assert_eq!(active(&tabs), Some("c"));
    }

    #[test]
    fn closing_the_last_tab_falls_back_to_its_neighbour() {
        let mut tabs = tabs(&["a", "b", "c"]);
        tabs.select(2);

        tabs.close(2);

        assert_eq!(active(&tabs), Some("b"));
    }

    #[test]
    fn closing_a_tab_right_of_the_active_one_leaves_the_selection_alone() {
        let mut tabs = tabs(&["a", "b", "c"]);
        tabs.select(0);

        tabs.close(2);

        assert_eq!(active(&tabs), Some("a"));
        assert_eq!(tabs.active_index(), 0);
    }

    #[test]
    fn closing_an_out_of_range_index_is_a_no_op() {
        let mut tabs = tabs(&["a"]);
        assert!(tabs.close(7).is_none());
        assert_eq!(tabs.iter().count(), 1);
    }

    #[test]
    fn emptying_the_strip_leaves_a_usable_index() {
        let mut tabs = tabs(&["a"]);
        tabs.close(0);

        assert!(tabs.is_empty());
        assert_eq!(tabs.active_index(), 0);
        assert!(tabs.active().is_none());
    }

    #[test]
    fn cycling_wraps_in_both_directions() {
        let mut tabs = tabs(&["a", "b", "c"]);
        tabs.select(0);

        tabs.select_next();
        assert_eq!(active(&tabs), Some("b"));
        tabs.select_next();
        tabs.select_next();
        assert_eq!(active(&tabs), Some("a"));

        tabs.select_prev();
        assert_eq!(active(&tabs), Some("c"));
    }

    #[test]
    fn cycling_an_empty_strip_does_not_panic() {
        let mut tabs: Tabs<&'static str> = Tabs::new();
        tabs.select_next();
        tabs.select_prev();
        tabs.select(3);
        assert_eq!(tabs.active_index(), 0);
    }

    #[test]
    fn short_titles_are_left_alone() {
        assert_eq!(display_title("pwsh", 24), "pwsh");
    }

    #[test]
    fn long_titles_are_capped_at_the_limit() {
        let title = display_title("a".repeat(100).as_str(), 10);
        assert_eq!(title.chars().count(), 10);
        assert!(title.ends_with('…'));
    }

    #[test]
    fn truncation_does_not_split_multibyte_characters() {
        let title = display_title("日本語のタイトルです", 4);
        assert_eq!(title, "日本語…");
    }

    #[test]
    fn control_characters_cannot_break_the_strip_layout() {
        assert_eq!(display_title("one\ntwo\tthree", 24), "one two three");
    }

    #[test]
    fn a_blank_title_falls_back_to_the_default() {
        assert_eq!(display_title("   ", 24), DEFAULT_TITLE);
        assert_eq!(display_title("", 24), DEFAULT_TITLE);
    }
}
