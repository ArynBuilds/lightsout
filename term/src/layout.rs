//! Splits: each tab is a tree of panes, side by side or stacked.

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
    Split { side_by_side: bool, ratio: f32, a: Box<Node>, b: Box<Node> },
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
        }
    }

    pub fn layout(&self, r: Rect) -> (Vec<(u64, Rect)>, Vec<Divider>) {
        let (mut panes, mut divs) = (Vec::new(), Vec::new());
        self.walk(r, &mut Vec::new(), &mut panes, &mut divs);
        (panes, divs)
    }

    fn walk(&self, r: Rect, path: &mut Vec<bool>, out: &mut Vec<(u64, Rect)>, divs: &mut Vec<Divider>) {
        match self {
            Node::Leaf(id) => out.push((*id, r)),
            Node::Split { side_by_side, ratio, a, b } => {
                let (ra, rb, at) = if *side_by_side {
                    let wa = (((r.w - GAP) as f32) * ratio).round() as i32;
                    (Rect { w: wa, ..r }, Rect { x: r.x + wa + GAP, w: r.w - wa - GAP, ..r }, r.x + wa)
                } else {
                    let ha = (((r.h - GAP) as f32) * ratio).round() as i32;
                    (Rect { h: ha, ..r }, Rect { y: r.y + ha + GAP, h: r.h - ha - GAP, ..r }, r.y + ha)
                };
                divs.push(Divider { side_by_side: *side_by_side, area: r, at, path: path.clone() });
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
            Node::Leaf(_) => false,
            Node::Split { a, b, .. } => a.split(target, new, side_by_side) || b.split(target, new, side_by_side),
        }
    }

    /// Removes a pane; its neighbour takes the space. False if not found here.
    pub fn remove(&mut self, target: u64) -> bool {
        let replacement = match self {
            Node::Split { a, b, .. } => match (&**a, &**b) {
                (Node::Leaf(x), _) if *x == target => Some((**b).clone()),
                (_, Node::Leaf(x)) if *x == target => Some((**a).clone()),
                _ => None,
            },
            Node::Leaf(_) => None,
        };
        if let Some(r) = replacement {
            *self = r;
            return true;
        }
        match self {
            Node::Split { a, b, .. } => a.remove(target) || b.remove(target),
            Node::Leaf(_) => false,
        }
    }

    pub fn set_ratio(&mut self, path: &[bool], value: f32) {
        match self {
            Node::Split { ratio, a, b, .. } => match path.first() {
                None => *ratio = value.clamp(0.1, 0.9),
                Some(false) => a.set_ratio(&path[1..], value),
                Some(true) => b.set_ratio(&path[1..], value),
            },
            Node::Leaf(_) => {}
        }
    }

    pub fn remap(&self, f: &dyn Fn(u64) -> u64) -> Node {
        match self {
            Node::Leaf(id) => Node::Leaf(f(*id)),
            Node::Split { side_by_side, ratio, a, b } => Node::Split {
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
