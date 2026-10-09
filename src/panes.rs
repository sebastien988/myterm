//! Split panes (Windows Terminal / iTerm2 / Terminator style): each tab holds
//! a binary tree of terminals separated by draggable dividers.

use egui::{pos2, Color32, CornerRadius, CursorIcon, Rect, Sense, Stroke, StrokeKind, UiBuilder};
use egui_term::{TerminalBackend, TerminalFont, TerminalTheme, TerminalView};

const DIVIDER: f32 = 4.0;

pub struct Pane {
    pub id: u64,
    pub backend: TerminalBackend,
    pub title: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// Children side by side (a vertical divider).
    Horizontal,
    /// Children stacked (a horizontal divider).
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

pub enum Node {
    Leaf(Box<Pane>),
    Split {
        axis: Axis,
        ratio: f32,
        first: Box<Node>,
        second: Box<Node>,
    },
}

/// Per-frame inputs and outputs of [`Node::show`].
pub struct PaneUi<'a> {
    pub focused: u64,
    /// Whether the focused pane should take keyboard input (false while an
    /// overlay such as the command palette owns the keyboard).
    pub keyboard: bool,
    pub theme: &'a TerminalTheme,
    pub font: &'a TerminalFont,
    pub accent: Color32,
    pub divider: Color32,
    pub inactive_overlay: Color32,
    pub split: bool,
    // Outputs.
    pub pressed: Option<u64>,
    pub right_clicked: Option<u64>,
    pub selection_released: Option<u64>,
    pub rects: Vec<(u64, Rect)>,
}

impl Node {
    pub fn is_split(&self) -> bool {
        matches!(self, Node::Split { .. })
    }

    pub fn contains(&self, id: u64) -> bool {
        self.pane(id).is_some()
    }

    pub fn pane(&self, id: u64) -> Option<&Pane> {
        match self {
            Node::Leaf(p) => (p.id == id).then_some(p),
            Node::Split { first, second, .. } => first.pane(id).or_else(|| second.pane(id)),
        }
    }

    pub fn pane_mut(&mut self, id: u64) -> Option<&mut Pane> {
        match self {
            Node::Leaf(p) => (p.id == id).then_some(p),
            Node::Split { first, second, .. } => match first.pane_mut(id) {
                Some(p) => Some(p),
                None => second.pane_mut(id),
            },
        }
    }

    pub fn first_id(&self) -> u64 {
        match self {
            Node::Leaf(p) => p.id,
            Node::Split { first, .. } => first.first_id(),
        }
    }

    /// Replace the pane `target` with a split holding it and `new`.
    /// `new` is taken only if `target` is found.
    pub fn split(self, target: u64, axis: Axis, new: &mut Option<Pane>) -> Node {
        match self {
            Node::Leaf(p) if p.id == target && new.is_some() => Node::Split {
                axis,
                ratio: 0.5,
                first: Box::new(Node::Leaf(p)),
                second: Box::new(Node::Leaf(Box::new(new.take().unwrap()))),
            },
            Node::Leaf(p) => Node::Leaf(p),
            Node::Split {
                axis: a,
                ratio,
                first,
                second,
            } => Node::Split {
                axis: a,
                ratio,
                first: Box::new(first.split(target, axis, new)),
                second: Box::new(second.split(target, axis, new)),
            },
        }
    }

    /// Remove the pane `id`; its sibling takes over the parent's space.
    /// Returns `None` when the tree is now empty.
    pub fn remove(self, id: u64) -> Option<Node> {
        match self {
            Node::Leaf(p) if p.id == id => None,
            Node::Leaf(p) => Some(Node::Leaf(p)),
            Node::Split {
                axis,
                ratio,
                first,
                second,
            } => match (first.remove(id), second.remove(id)) {
                (Some(a), Some(b)) => Some(Node::Split {
                    axis,
                    ratio,
                    first: Box::new(a),
                    second: Box::new(b),
                }),
                (Some(only), None) | (None, Some(only)) => Some(only),
                (None, None) => None,
            },
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, rect: Rect, p: &mut PaneUi) {
        match self {
            Node::Leaf(pane) => show_leaf(pane, ui, rect, p),
            Node::Split {
                axis,
                ratio,
                first,
                second,
            } => {
                let (a, divider, b) = split_rect(*axis, *ratio, rect);
                let id = ui.id().with(("divider", first.first_id(), second.first_id()));
                let resp = ui.interact(divider, id, Sense::drag());
                if resp.hovered() || resp.dragged() {
                    ui.ctx().set_cursor_icon(match axis {
                        Axis::Horizontal => CursorIcon::ResizeHorizontal,
                        Axis::Vertical => CursorIcon::ResizeVertical,
                    });
                }
                if resp.dragged() {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        let r = match axis {
                            Axis::Horizontal => (pos.x - rect.min.x) / rect.width(),
                            Axis::Vertical => (pos.y - rect.min.y) / rect.height(),
                        };
                        *ratio = r.clamp(0.1, 0.9);
                    }
                }
                let color = if resp.hovered() || resp.dragged() {
                    p.accent
                } else {
                    p.divider
                };
                ui.painter().rect_filled(divider, CornerRadius::ZERO, color);

                first.show(ui, a, p);
                second.show(ui, b, p);
            }
        }
    }
}

fn show_leaf(pane: &mut Pane, ui: &mut egui::Ui, rect: Rect, p: &mut PaneUi) {
    let is_focused = pane.id == p.focused;
    let mut child = ui.new_child(UiBuilder::new().max_rect(rect).id_salt(("pane", pane.id)));
    let view = TerminalView::new(&mut child, &mut pane.backend)
        .set_focus(is_focused && p.keyboard)
        .set_size(rect.size())
        .set_theme(p.theme.clone())
        .set_font(p.font.clone());
    let resp = child.add(view);

    let (pressed, released) =
        ui.input(|i| (i.pointer.any_pressed(), i.pointer.primary_released()));
    if resp.contains_pointer() && pressed {
        p.pressed = Some(pane.id);
    }
    if resp.secondary_clicked() {
        p.right_clicked = Some(pane.id);
    }
    if is_focused && released {
        p.selection_released = Some(pane.id);
    }

    if p.split {
        if is_focused {
            ui.painter().rect_stroke(
                rect,
                CornerRadius::ZERO,
                Stroke::new(1.0_f32, p.accent),
                StrokeKind::Inside,
            );
        } else {
            ui.painter()
                .rect_filled(rect, CornerRadius::ZERO, p.inactive_overlay);
        }
    }
    p.rects.push((pane.id, rect));
}

fn split_rect(axis: Axis, ratio: f32, rect: Rect) -> (Rect, Rect, Rect) {
    match axis {
        Axis::Horizontal => {
            let x = (rect.min.x + (rect.width() - DIVIDER) * ratio).round();
            (
                Rect::from_min_max(rect.min, pos2(x, rect.max.y)),
                Rect::from_min_max(pos2(x, rect.min.y), pos2(x + DIVIDER, rect.max.y)),
                Rect::from_min_max(pos2(x + DIVIDER, rect.min.y), rect.max),
            )
        }
        Axis::Vertical => {
            let y = (rect.min.y + (rect.height() - DIVIDER) * ratio).round();
            (
                Rect::from_min_max(rect.min, pos2(rect.max.x, y)),
                Rect::from_min_max(pos2(rect.min.x, y), pos2(rect.max.x, y + DIVIDER)),
                Rect::from_min_max(pos2(rect.min.x, y + DIVIDER), rect.max),
            )
        }
    }
}

/// The pane next to `from` in `dir`, using the rects from the last frame.
pub fn neighbor(rects: &[(u64, Rect)], from: u64, dir: Direction) -> Option<u64> {
    let (_, cur) = rects.iter().find(|(id, _)| *id == from)?;
    let c = cur.center();
    rects
        .iter()
        .filter(|(id, _)| *id != from)
        .filter_map(|(id, r)| {
            // Distance along `dir`, and how far off-axis the candidate is.
            let (ahead, overlap, off_axis) = match dir {
                Direction::Left => (cur.min.x - r.max.x, overlaps(cur.y_range(), r.y_range()), (r.center().y - c.y).abs()),
                Direction::Right => (r.min.x - cur.max.x, overlaps(cur.y_range(), r.y_range()), (r.center().y - c.y).abs()),
                Direction::Up => (cur.min.y - r.max.y, overlaps(cur.x_range(), r.x_range()), (r.center().x - c.x).abs()),
                Direction::Down => (r.min.y - cur.max.y, overlaps(cur.x_range(), r.x_range()), (r.center().x - c.x).abs()),
            };
            (ahead >= -1.0 && overlap).then_some((*id, ahead + off_axis * 0.5))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

fn overlaps(a: egui::Rangef, b: egui::Rangef) -> bool {
    a.min < b.max && b.min < a.max
}
