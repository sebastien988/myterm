//! Command palette (Windows Terminal / VS Code / Warp style): type to fuzzy
//! filter every action, arrow keys to pick, Enter to run.

use egui::{Align2, Color32, Key, RichText};

pub struct Command<A> {
    pub label: String,
    pub shortcut: &'static str,
    pub action: A,
}

#[derive(Default)]
pub struct Palette {
    query: String,
    selected: usize,
    just_opened: bool,
}

pub enum Outcome<A> {
    Open,
    Close,
    Run(A),
}

impl Palette {
    pub fn new() -> Self {
        Self {
            just_opened: true,
            ..Default::default()
        }
    }

    pub fn show<A: Clone>(
        &mut self,
        ctx: &egui::Context,
        commands: &[Command<A>],
        accent: Color32,
        dim: Color32,
    ) -> Outcome<A> {
        let mut matches: Vec<(i32, &Command<A>)> = commands
            .iter()
            .filter_map(|c| fuzzy_score(&self.query, &c.label).map(|s| (s, c)))
            .collect();
        matches.sort_by_key(|(score, _)| -score);
        self.selected = self.selected.min(matches.len().saturating_sub(1));

        let (up, down, enter, escape) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, Key::Enter),
                i.consume_key(egui::Modifiers::NONE, Key::Escape),
            )
        });
        if escape {
            return Outcome::Close;
        }
        if up {
            self.selected = self.selected.saturating_sub(1);
        }
        if down && self.selected + 1 < matches.len() {
            self.selected += 1;
        }
        if enter {
            return match matches.get(self.selected) {
                Some((_, c)) => Outcome::Run(c.action.clone()),
                None => Outcome::Close,
            };
        }

        let first_frame = self.just_opened;
        let mut outcome = Outcome::Open;
        let area = egui::Area::new(egui::Id::new("command_palette"))
            .order(egui::Order::Foreground)
            .anchor(Align2::CENTER_TOP, egui::vec2(0.0, 48.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_width(460.0);
                        let edit = ui.add(
                            egui::TextEdit::singleline(&mut self.query)
                                .hint_text("Type a command…")
                                .desired_width(f32::INFINITY),
                        );
                        if self.just_opened {
                            edit.request_focus();
                            self.just_opened = false;
                        }
                        if edit.changed() {
                            self.selected = 0;
                        }
                        ui.add_space(4.0);

                        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                            if matches.is_empty() {
                                ui.label(RichText::new("No matching commands").color(dim));
                            }
                            for (i, (_, c)) in matches.iter().enumerate() {
                                let selected = i == self.selected;
                                let row = ui.horizontal(|ui| {
                                    ui.set_width(ui.available_width());
                                    let text = RichText::new(&c.label);
                                    ui.label(if selected { text.color(accent) } else { text });
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| ui.label(RichText::new(c.shortcut).color(dim).small()),
                                    );
                                });
                                let resp = ui.interact(
                                    row.response.rect,
                                    ui.id().with(("palette_row", i)),
                                    egui::Sense::click(),
                                );
                                if selected {
                                    resp.scroll_to_me(None);
                                }
                                if resp.hovered() {
                                    self.selected = i;
                                }
                                if resp.clicked() {
                                    outcome = Outcome::Run(c.action.clone());
                                }
                            }
                        });
                    });
            });

        if !first_frame && matches!(outcome, Outcome::Open) && area.response.clicked_elsewhere() {
            return Outcome::Close;
        }
        outcome
    }
}

/// Case-insensitive subsequence match. Higher is better; `None` = no match.
fn fuzzy_score(query: &str, label: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let query_lc = query.trim().to_lowercase();
    let label_lower = label.to_lowercase();
    // A literal substring beats any scattered match, more so at a word start.
    // (Skipped for 1–2 letters, which are usually initials like "nt".)
    let substring = if query_lc.chars().count() >= 3 {
        label_lower.find(&query_lc)
    } else {
        None
    };
    let substring_bonus = match substring {
        Some(0) => 40,
        Some(i) if !label_lower[..i].ends_with(char::is_alphanumeric) => 40,
        Some(_) => 25,
        None => 0,
    };
    let label_lc: Vec<char> = label_lower.chars().collect();
    let mut score = 0;
    let mut pos = 0;
    let mut prev_hit: Option<usize> = None;
    for q in query.to_lowercase().chars().filter(|c| !c.is_whitespace()) {
        let found = label_lc[pos..].iter().position(|&c| c == q)? + pos;
        score += 1;
        if prev_hit.is_some_and(|p| p + 1 == found) {
            score += 3; // consecutive letters
        }
        if found == 0 || !label_lc[found - 1].is_alphanumeric() {
            score += 5; // start of a word
        }
        prev_hit = Some(found);
        pos = found + 1;
    }
    Some(score + substring_bonus - label_lc.len() as i32 / 10)
}

#[cfg(test)]
mod tests {
    use super::fuzzy_score;

    #[test]
    fn subsequence_matches() {
        assert!(fuzzy_score("spr", "Split pane right").is_some());
        assert!(fuzzy_score("xyz", "Split pane right").is_none());
        assert!(fuzzy_score("", "anything").is_some());
    }

    #[test]
    fn substring_beats_scattered_match() {
        let exact = fuzzy_score("light", "Color scheme: Solarized Light").unwrap();
        let scattered = fuzzy_score("light", "Split pane right").unwrap();
        assert!(exact > scattered);
    }

    #[test]
    fn word_starts_rank_higher() {
        let word_start = fuzzy_score("nt", "New tab").unwrap();
        let scattered = fuzzy_score("nt", "Font size: reset").unwrap();
        assert!(word_start > scattered);
    }
}
