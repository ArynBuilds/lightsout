//! How a tab's panes are placed: a tree of splits (side by side or stacked), and panes that
//! float over it (their own position and size, like windows).

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Node {
    Leaf(u64),
    Split {
        side_by_side: bool,
        ratio: f32,
        a: Box<Node>,
        b: Box<Node>,
    },
    /// No pane in the tree: they all float.
    Empty,
}

/// A pane floating over the tab, as a share of the area it floats in (0..1), so it stays in
/// place when the window changes size (and works in a terminal's cells as in pixels). The
/// order in the tab's list is the stacking order: the last is on top.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Float {
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    #[serde(default)]
    pub min: bool,
}

impl Float {
    /// Where it is in `area` (title bar included), at least `min_w` × `min_h`, inside the area.
    pub fn frame(&self, area: Rect, min_w: i32, min_h: i32) -> Rect {
        let w = ((self.w * area.w as f32).round() as i32).clamp(min_w.min(area.w), area.w);
        let h = ((self.h * area.h as f32).round() as i32).clamp(min_h.min(area.h), area.h);
        let x =
            (area.x + (self.x * area.w as f32).round() as i32).clamp(area.x, area.x + area.w - w);
        let y =
            (area.y + (self.y * area.h as f32).round() as i32).clamp(area.y, area.y + area.h - h);
        Rect { x, y, w, h }
    }
}

/// The line between two panes, for dragging.
pub struct Divider {
    pub side_by_side: bool,
    pub area: Rect,
    pub at: i32,
    pub path: Vec<bool>,
}

pub const GAP: i32 = 1;

impl Node {
    pub fn leaves(&self) -> Vec<u64> {
        let mut v = Vec::new();
        self.collect(&mut v);
        v
    }

    fn collect(&self, v: &mut Vec<u64>) {
        match self {
            Node::Leaf(id) => v.push(*id),
            Node::Split { a, b, .. } => {
                a.collect(v);
                b.collect(v);
            }
            Node::Empty => {}
        }
    }

    pub fn layout(&self, r: Rect) -> (Vec<(u64, Rect)>, Vec<Divider>) {
        let (mut panes, mut divs) = (Vec::new(), Vec::new());
        self.walk(r, &mut Vec::new(), &mut panes, &mut divs);
        (panes, divs)
    }

    fn walk(
        &self,
        r: Rect,
        path: &mut Vec<bool>,
        out: &mut Vec<(u64, Rect)>,
        divs: &mut Vec<Divider>,
    ) {
        match self {
            Node::Leaf(id) => out.push((*id, r)),
            Node::Empty => {}
            Node::Split {
                side_by_side,
                ratio,
                a,
                b,
            } => {
                let (ra, rb, at) = if *side_by_side {
                    let wa = (((r.w - GAP) as f32) * ratio).round() as i32;
                    (
                        Rect { w: wa, ..r },
                        Rect {
                            x: r.x + wa + GAP,
                            w: r.w - wa - GAP,
                            ..r
                        },
                        r.x + wa,
                    )
                } else {
                    let ha = (((r.h - GAP) as f32) * ratio).round() as i32;
                    (
                        Rect { h: ha, ..r },
                        Rect {
                            y: r.y + ha + GAP,
                            h: r.h - ha - GAP,
                            ..r
                        },
                        r.y + ha,
                    )
                };
                divs.push(Divider {
                    side_by_side: *side_by_side,
                    area: r,
                    at,
                    path: path.clone(),
                });
                path.push(false);
                a.walk(ra, path, out, divs);
                path.pop();
                path.push(true);
                b.walk(rb, path, out, divs);
                path.pop();
            }
        }
    }

    /// Splits pane `target`, putting `new` next to it (right or below).
    pub fn split(&mut self, target: u64, new: u64, side_by_side: bool) -> bool {
        match self {
            Node::Leaf(id) if *id == target => {
                *self = Node::Split {
                    side_by_side,
                    ratio: 0.5,
                    a: Box::new(Node::Leaf(target)),
                    b: Box::new(Node::Leaf(new)),
                };
                true
            }
            Node::Leaf(_) | Node::Empty => false,
            Node::Split { a, b, .. } => {
                a.split(target, new, side_by_side) || b.split(target, new, side_by_side)
            }
        }
    }

    /// Adds a pane: next to the last one (right), or as the only one when there is none.
    pub fn add(&mut self, new: u64) {
        match self.leaves().last() {
            Some(last) => {
                self.split(*last, new, true);
            }
            None => *self = Node::Leaf(new),
        }
    }

    /// Removes a pane; its neighbour takes the space (the last one leaves the tree empty).
    /// False if not found here.
    pub fn remove(&mut self, target: u64) -> bool {
        if matches!(self, Node::Leaf(x) if *x == target) {
            *self = Node::Empty;
            return true;
        }
        let replacement = match self {
            Node::Split { a, b, .. } => match (&**a, &**b) {
                (Node::Leaf(x), _) if *x == target => Some((**b).clone()),
                (_, Node::Leaf(x)) if *x == target => Some((**a).clone()),
                _ => None,
            },
            Node::Leaf(_) | Node::Empty => None,
        };
        if let Some(r) = replacement {
            *self = r;
            return true;
        }
        match self {
            Node::Split { a, b, .. } => a.remove(target) || b.remove(target),
            Node::Leaf(_) | Node::Empty => false,
        }
    }

    pub fn set_ratio(&mut self, path: &[bool], value: f32) {
        match self {
            Node::Split { ratio, a, b, .. } => match path.first() {
                None => *ratio = value.clamp(0.1, 0.9),
                Some(false) => a.set_ratio(&path[1..], value),
                Some(true) => b.set_ratio(&path[1..], value),
            },
            Node::Leaf(_) | Node::Empty => {}
        }
    }

    pub fn remap(&self, f: &dyn Fn(u64) -> u64) -> Node {
        match self {
            Node::Empty => Node::Empty,
            Node::Leaf(id) => Node::Leaf(f(*id)),
            Node::Split {
                side_by_side,
                ratio,
                a,
                b,
            } => Node::Split {
                side_by_side: *side_by_side,
                ratio: *ratio,
                a: Box::new(a.remap(f)),
                b: Box::new(b.remap(f)),
            },
        }
    }
}

/// The pane next to `from` in a direction (dx, dy), for Alt+arrows.
pub fn neighbor(rects: &[(u64, Rect)], from: u64, dx: i32, dy: i32) -> Option<u64> {
    let f = rects.iter().find(|(id, _)| *id == from)?.1;
    let overlap = |a0: i32, a1: i32, b0: i32, b1: i32| a0 < b1 && b0 < a1;
    rects
        .iter()
        .filter(|(id, _)| *id != from)
        .filter_map(|(id, r)| {
            let gap = if dx > 0 && r.x >= f.x + f.w && overlap(f.y, f.y + f.h, r.y, r.y + r.h) {
                r.x - (f.x + f.w)
            } else if dx < 0 && r.x + r.w <= f.x && overlap(f.y, f.y + f.h, r.y, r.y + r.h) {
                f.x - (r.x + r.w)
            } else if dy > 0 && r.y >= f.y + f.h && overlap(f.x, f.x + f.w, r.x, r.x + r.w) {
                r.y - (f.y + f.h)
            } else if dy < 0 && r.y + r.h <= f.y && overlap(f.x, f.x + f.w, r.x, r.x + r.w) {
                f.y - (r.y + r.h)
            } else {
                return None;
            };
            Some((gap, *id))
        })
        .min()
        .map(|(_, id)| id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_pane_leaves_an_empty_tree_and_the_next_fills_it() {
        let mut n = Node::Leaf(1);
        n.add(2);
        assert_eq!(n.leaves(), [1, 2]);
        assert!(n.remove(1));
        assert_eq!(n.leaves(), [2]);
        assert!(n.remove(2));
        assert!(matches!(n, Node::Empty));
        assert!(n
            .layout(Rect {
                x: 0,
                y: 0,
                w: 10,
                h: 10
            })
            .0
            .is_empty());
        n.add(3);
        assert_eq!(n.leaves(), [3]);
    }

    #[test]
    fn a_floating_pane_stays_inside_and_keeps_a_usable_size() {
        let area = Rect {
            x: 10,
            y: 5,
            w: 100,
            h: 50,
        };
        let f = Float {
            id: 1,
            x: 0.5,
            y: 0.5,
            w: 0.25,
            h: 0.2,
            min: false,
        };
        assert_eq!(
            f.frame(area, 4, 4),
            Rect {
                x: 60,
                y: 30,
                w: 25,
                h: 10
            }
        );
        // pushed back in when it would stick out, and never smaller than asked
        let far = Float {
            x: 0.95,
            y: 0.95,
            w: 0.01,
            h: 0.01,
            ..f
        };
        let r = far.frame(area, 20, 8);
        assert_eq!((r.w, r.h), (20, 8));
        assert!(r.x + r.w <= area.x + area.w && r.y + r.h <= area.y + area.h);
    }
}
