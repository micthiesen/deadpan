//! Editorial edges derived from the current tree.
//!
//! A split fragment (Partition) has no audio edge of its own, so a cut made
//! by an ordinary command (a word Repeat, a pause, a speed change, a reverse)
//! would join two unrelated sounds with a raw step: a click. Every fragment
//! side that meets anything but its own continuation therefore gets the
//! automatic short fade, exactly as if `SetEditorialEdges` had marked it; an
//! explicit Hard policy still cuts it. A side that continues the same sound
//! (a pure Split: equal child recipes and contiguous mappings) stays
//! continuous, because a fade there would dip it.

use std::collections::BTreeMap;

use deadpan_core::{
    AudioEditorialEdges, FrozenAudioKind, FrozenAudioLayout, NodeId, NodeKind, ProjectDocument,
    RetimePurpose,
};

/// One node's audio shape, as far as cuts are concerned.
pub(super) enum Shape<'a> {
    Sequence(&'a [NodeId]),
    Repeat {
        child: &'a NodeId,
        plays: u32,
        gap: bool,
    },
    Partition {
        child: &'a NodeId,
        start: i64,
        end: i64,
    },
    /// Any other single-child stage: an atom whose interior is visited.
    Stage(&'a NodeId),
    Leaf,
}

/// A tree the plan compiles: a live document or a frozen audio layout.
pub(super) trait Tree {
    fn root(&self) -> &NodeId;
    fn shape(&self, id: &NodeId) -> Shape<'_>;
    fn positive(&self, id: &NodeId) -> bool;
    /// Whether two Partition children are the same recipe.
    fn same(&self, left: &NodeId, right: &NodeId) -> bool;
    /// Override and gap-override roots owned by a Repeat.
    fn extra_roots(&self, id: &NodeId) -> Vec<&NodeId>;
}

pub(super) struct Document<'a, F> {
    pub(super) document: &'a ProjectDocument,
    pub(super) positive: F,
}

impl<F: Fn(&NodeId) -> bool> Tree for Document<'_, F> {
    fn root(&self) -> &NodeId {
        self.document.root()
    }
    fn shape(&self, id: &NodeId) -> Shape<'_> {
        match &self.document.nodes()[id].kind {
            NodeKind::Sequence { children } => Shape::Sequence(children),
            NodeKind::Repeat {
                child,
                iterations,
                gap,
                ..
            } => Shape::Repeat {
                child,
                plays: iterations.len(),
                gap: gap.as_ref().is_some_and(|gap| gap.duration.frames() > 0),
            },
            NodeKind::Retime {
                child,
                mapping,
                purpose: RetimePurpose::Partition,
                ..
            } => Shape::Partition {
                child,
                start: mapping.start().0,
                end: mapping.end().0,
            },
            NodeKind::Retime { child, .. } => Shape::Stage(child),
            NodeKind::Source { .. } | NodeKind::Hold { .. } => Shape::Leaf,
        }
    }
    fn positive(&self, id: &NodeId) -> bool {
        (self.positive)(id)
    }
    fn same(&self, left: &NodeId, right: &NodeId) -> bool {
        same_beat(self.document, left, right, 0)
    }
    fn extra_roots(&self, id: &NodeId) -> Vec<&NodeId> {
        let plays = self.document.overrides().get(id).into_iter();
        let gaps = self.document.gap_overrides().get(id).into_iter();
        plays
            .chain(gaps)
            .flat_map(|roots| roots.iter().map(|(_, root)| root))
            .collect()
    }
}

impl Tree for FrozenAudioLayout {
    fn root(&self) -> &NodeId {
        FrozenAudioLayout::root(self)
    }
    fn shape(&self, id: &NodeId) -> Shape<'_> {
        match &self.nodes()[id].kind {
            FrozenAudioKind::Sequence { children } => Shape::Sequence(children),
            FrozenAudioKind::Repeat {
                child,
                iterations,
                gap_duration,
                ..
            } => Shape::Repeat {
                child,
                plays: iterations.len(),
                gap: gap_duration.frames() > 0,
            },
            FrozenAudioKind::Retime {
                child,
                mapping,
                purpose: RetimePurpose::Partition,
                ..
            } => Shape::Partition {
                child,
                start: mapping.start().0,
                end: mapping.end().0,
            },
            FrozenAudioKind::Retime { child, .. } => Shape::Stage(child),
            FrozenAudioKind::Source { .. } | FrozenAudioKind::Hold { .. } => Shape::Leaf,
        }
    }
    fn positive(&self, id: &NodeId) -> bool {
        self.nodes()[id].duration.frames() > 0
    }
    fn same(&self, left: &NodeId, right: &NodeId) -> bool {
        same_frozen(self, left, right, 0)
    }
    fn extra_roots(&self, id: &NodeId) -> Vec<&NodeId> {
        let plays = self.overrides().get(id).into_iter();
        let gaps = self.gap_overrides().get(id).into_iter();
        plays
            .chain(gaps)
            .flat_map(|roots| roots.iter().map(|(_, root)| root))
            .collect()
    }
}

/// Split fragments hold full copies of one context under fresh node IDs, so
/// the same sound is an equal recipe tree, compared without identities.
fn same_beat(document: &ProjectDocument, left: &NodeId, right: &NodeId, depth: usize) -> bool {
    if left == right {
        return true;
    }
    if depth > deadpan_core::MAX_DOCUMENT_DEPTH {
        return false;
    }
    let (l, r) = (&document.nodes()[left], &document.nodes()[right]);
    if l.audio_treatments != r.audio_treatments || l.audio_edges != r.audio_edges {
        return false;
    }
    let same = |a: &NodeId, b: &NodeId| same_beat(document, a, b, depth + 1);
    match (&l.kind, &r.kind) {
        (NodeKind::Sequence { children: a }, NodeKind::Sequence { children: b }) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b))
        }
        (
            NodeKind::Retime {
                child: a,
                duration: ad,
                mapping: am,
                pitch: ap,
                purpose: au,
            },
            NodeKind::Retime {
                child: b,
                duration: bd,
                mapping: bm,
                pitch: bp,
                purpose: bu,
            },
        ) => ad == bd && am == bm && ap == bp && au == bu && same(a, b),
        (
            NodeKind::Repeat {
                child: a,
                iterations: ai,
                gap: ag,
                escalation: ae,
            },
            NodeKind::Repeat {
                child: b,
                iterations: bi,
                gap: bg,
                escalation: be,
            },
        ) => {
            let plain = |id: &NodeId| {
                !document.overrides().contains_key(id) && !document.gap_overrides().contains_key(id)
            };
            ai == bi && ag == bg && ae == be && plain(left) && plain(right) && same(a, b)
        }
        (a, b) => a == b,
    }
}

fn same_frozen(layout: &FrozenAudioLayout, left: &NodeId, right: &NodeId, depth: usize) -> bool {
    if left == right {
        return true;
    }
    if depth > deadpan_core::MAX_DOCUMENT_DEPTH {
        return false;
    }
    let (l, r) = (&layout.nodes()[left], &layout.nodes()[right]);
    if l.duration != r.duration || l.edges != r.edges {
        return false;
    }
    let same = |a: &NodeId, b: &NodeId| same_frozen(layout, a, b, depth + 1);
    match (&l.kind, &r.kind) {
        (FrozenAudioKind::Sequence { children: a }, FrozenAudioKind::Sequence { children: b }) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b))
        }
        (
            FrozenAudioKind::Retime {
                child: a,
                mapping: am,
                pitch: ap,
                purpose: au,
            },
            FrozenAudioKind::Retime {
                child: b,
                mapping: bm,
                pitch: bp,
                purpose: bu,
            },
        ) => am == bm && ap == bp && au == bu && same(a, b),
        (
            FrozenAudioKind::Repeat {
                child: a,
                iterations: ai,
                gap_duration: ag,
                gap_audio: aa,
            },
            FrozenAudioKind::Repeat {
                child: b,
                iterations: bi,
                gap_duration: bg,
                gap_audio: ba,
            },
        ) => {
            let plain = |id: &NodeId| {
                !layout.overrides().contains_key(id) && !layout.gap_overrides().contains_key(id)
            };
            ai == bi && ag == bg && aa == ba && plain(left) && plain(right) && same(a, b)
        }
        (a, b) => a == b,
    }
}

/// The leaf that sounds first or last in a node's output.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Atom<'a> {
    Node(&'a NodeId),
    /// An implicit Repeat gap: never a continuation.
    Gap,
}

struct Walk<'a, T: ?Sized> {
    tree: &'a T,
    pairs: Vec<(Atom<'a>, Atom<'a>)>,
}

impl<'a, T: Tree + ?Sized> Walk<'a, T> {
    fn first(&self, id: &'a NodeId) -> Option<Atom<'a>> {
        match self.tree.shape(id) {
            Shape::Sequence(children) => children
                .iter()
                .filter(|child| self.tree.positive(child))
                .find_map(|child| self.first(child)),
            Shape::Repeat { child, .. } => self.first(child),
            _ => Some(Atom::Node(id)),
        }
    }

    fn last(&self, id: &'a NodeId) -> Option<Atom<'a>> {
        match self.tree.shape(id) {
            Shape::Sequence(children) => children
                .iter()
                .rev()
                .filter(|child| self.tree.positive(child))
                .find_map(|child| self.last(child)),
            Shape::Repeat { child, .. } => self.last(child),
            _ => Some(Atom::Node(id)),
        }
    }

    fn visit(&mut self, id: &'a NodeId) {
        match self.tree.shape(id) {
            Shape::Sequence(children) => {
                let mut previous: Option<Atom<'a>> = None;
                for child in children {
                    if !self.tree.positive(child) {
                        continue;
                    }
                    if let (Some(left), Some(right)) = (previous, self.first(child)) {
                        self.pairs.push((left, right));
                    }
                    if let Some(last) = self.last(child) {
                        previous = Some(last);
                    }
                    self.visit(child);
                }
            }
            Shape::Repeat { child, plays, gap } => {
                if plays > 1
                    && self.tree.positive(child)
                    && let (Some(first), Some(last)) = (self.first(child), self.last(child))
                {
                    if gap {
                        self.pairs.push((last, Atom::Gap));
                        self.pairs.push((Atom::Gap, first));
                    } else {
                        self.pairs.push((last, first));
                    }
                }
                self.visit(child);
            }
            Shape::Partition { child, .. } | Shape::Stage(child) => self.visit(child),
            Shape::Leaf => {}
        }
        for root in self.tree.extra_roots(id) {
            self.visit(root);
        }
    }
}

/// Fragment sides that are cuts, for every Partition with at least one.
pub(super) fn derive<T: Tree + ?Sized>(tree: &T) -> BTreeMap<NodeId, AudioEditorialEdges> {
    let mut walk = Walk {
        tree,
        pairs: Vec::new(),
    };
    walk.visit(tree.root());
    let partition = |atom: Atom<'_>| match atom {
        Atom::Node(id) => match tree.shape(id) {
            Shape::Partition { child, start, end } => Some((id.clone(), child.clone(), start, end)),
            _ => None,
        },
        Atom::Gap => None,
    };
    let mut edges: BTreeMap<NodeId, AudioEditorialEdges> = BTreeMap::new();
    let marked = |node: &NodeId, start: bool, edges: &mut BTreeMap<_, AudioEditorialEdges>| {
        let entry = edges.entry(node.clone()).or_default();
        let side = if start {
            &mut entry.start
        } else {
            &mut entry.end
        };
        !std::mem::replace(side, true)
    };
    for (left, right) in &walk.pairs {
        let (l, r) = (partition(*left), partition(*right));
        let continuous = matches!(
            (&l, &r),
            (Some((_, lc, _, lend)), Some((_, rc, rstart, _)))
                if lend == rstart && tree.same(lc, rc)
        );
        if continuous {
            continue;
        }
        if let Some((id, ..)) = &l {
            marked(id, false, &mut edges);
        }
        if let Some((id, ..)) = &r {
            marked(id, true, &mut edges);
        }
    }
    // A fade on one side of a boundary needs the other side to fade too, or
    // the faded edge meets the full-level neighbor with a step. A node's mark
    // applies at every occurrence, so a play seam's mark also reaches the
    // Repeat's outer neighbors even where they continue the first play.
    let side = |edges: &BTreeMap<NodeId, AudioEditorialEdges>, id: &NodeId, start: bool| {
        edges
            .get(id)
            .is_some_and(|edge| if start { edge.start } else { edge.end })
    };
    loop {
        let mut changed = false;
        for (left, right) in &walk.pairs {
            if let (Some((l, ..)), Some((r, ..))) = (partition(*left), partition(*right)) {
                if side(&edges, &l, false) && !side(&edges, &r, true) {
                    changed |= marked(&r, true, &mut edges);
                }
                if side(&edges, &r, true) && !side(&edges, &l, false) {
                    changed |= marked(&l, false, &mut edges);
                }
            }
        }
        if !changed {
            break;
        }
    }
    edges.retain(|_, edge| !edge.is_empty());
    edges
}
