//! Bounded, exact edits of one retained sound-recipe clock. Routes select and
//! translate intervals; they never stretch, decode, fade or establish a sampled
//! lattice. Gaps have no inherited recipe contribution. Repeat nodes describe
//! periodic edits, not repeated source playback: their input stride preserves
//! monotone recipe progress without materializing plays.

use std::{cmp::Ordering, io::Write};

use serde::{Deserialize, Serialize};

use crate::{
    DocumentError, DocumentErrorCode, ExactFrameRange, ExactRatio, InsertionBias, TimeError,
};

pub const MAX_SOUND_ROUTE_NODES: usize = 4096;
pub const MAX_SOUND_ROUTE_EDGES: usize = 16_384;
pub const MAX_SOUND_ROUTE_DEPTH: usize = 64;
pub const MAX_SOUND_ROUTE_JSON_BYTES: usize = 1024 * 1024;
pub const MAX_SOUND_ROUTE_QUERY_WORK: usize = 1_000_000;
pub const MAX_SOUND_ROUTE_QUERY_SPANS: usize = 16_384;

type Result<T> = std::result::Result<T, DocumentError>;

/// References must name earlier arena entries. This makes cycles and forward
/// references invalid without recursive graph admission or recursive serde.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SoundRippleNode {
    Keep {
        range: ExactFrameRange,
    },
    Gap {
        duration: ExactRatio,
    },
    Sequence {
        #[serde(deserialize_with = "bounded_vec")]
        parts: Vec<u32>,
    },
    Repeat {
        body: u32,
        count: u32,
        input_stride: ExactRatio,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RippleIndex {
    extent: ExactRatio,
    footprint: Option<ExactFrameRange>,
    ends: Vec<ExactRatio>,
    depth: usize,
}

/// A partial, monotone, unity-rate map from new output to previous output.
/// Sequence concatenates output; Repeat advances all Keep coordinates in its
/// body by `iteration * input_stride`. Its output period is the body extent.
/// Use `from_json` for external input. Serde embedding validates the resulting
/// map but requires its caller to bound the containing input before parsing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SoundRippleMap {
    input_extent: ExactRatio,
    output_extent: ExactRatio,
    root: u32,
    nodes: Vec<SoundRippleNode>,
    #[serde(skip)]
    index: Vec<RippleIndex>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RippleWire {
    input_extent: ExactRatio,
    output_extent: ExactRatio,
    root: u32,
    #[serde(deserialize_with = "bounded_vec")]
    nodes: Vec<SoundRippleNode>,
}

impl SoundRippleMap {
    pub fn new(input_extent: ExactRatio, root: u32, nodes: Vec<SoundRippleNode>) -> Result<Self> {
        positive(input_extent)?;
        node_count(nodes.len())?;
        let mut index: Vec<RippleIndex> = Vec::with_capacity(nodes.len());
        let mut edges = 0usize;
        for (ordinal, node) in nodes.iter().enumerate() {
            let entry = match node {
                SoundRippleNode::Keep { range } => {
                    contained(*range, input_extent, false)?;
                    RippleIndex {
                        extent: length(*range)?,
                        footprint: Some(*range),
                        ends: vec![],
                        depth: 1,
                    }
                }
                SoundRippleNode::Gap { duration } => {
                    positive(*duration)?;
                    RippleIndex {
                        extent: *duration,
                        footprint: None,
                        ends: vec![],
                        depth: 1,
                    }
                }
                SoundRippleNode::Sequence { parts } => {
                    if parts.is_empty() {
                        return Err(invalid("sound ripple Sequence is empty"));
                    }
                    add_count(&mut edges, parts.len(), MAX_SOUND_ROUTE_EDGES)?;
                    let mut extent = ExactRatio::ZERO;
                    let mut footprint: Option<ExactFrameRange> = None;
                    let mut depth = 1;
                    let mut ends = Vec::with_capacity(parts.len());
                    for part in parts {
                        let child = &index[earlier(*part, ordinal)?];
                        extent = extent.checked_add(child.extent)?;
                        positive(extent)?;
                        ends.push(extent);
                        depth = depth.max(child.depth + 1);
                        if let Some(next) = child.footprint {
                            if let Some(previous) = &mut footprint {
                                if compare(previous.end, next.start)?.is_gt() {
                                    return Err(invalid(
                                        "sound ripple Keep intervals are out of order",
                                    ));
                                }
                                previous.end = next.end;
                            } else {
                                footprint = Some(next);
                            }
                        }
                    }
                    RippleIndex {
                        extent,
                        footprint,
                        ends,
                        depth,
                    }
                }
                SoundRippleNode::Repeat {
                    body,
                    count,
                    input_stride,
                } => {
                    add_count(&mut edges, 1, MAX_SOUND_ROUTE_EDGES)?;
                    let child = &index[earlier(*body, ordinal)?];
                    if *count == 0 || input_stride.compare_integer(0).is_lt() {
                        return Err(invalid(
                            "sound ripple Repeat needs positive count and nonnegative stride",
                        ));
                    }
                    let extent = child
                        .extent
                        .checked_mul(ExactRatio::integer(i64::from(*count)))?;
                    positive(extent)?;
                    let footprint =
                        child
                            .footprint
                            .map(|range| -> Result<_> {
                                if *count > 1 && compare(*input_stride, length(range)?)?.is_lt() {
                                    return Err(invalid(
                                        "sound ripple Repeat overlaps or reverses input",
                                    ));
                                }
                                let end =
                                    range.end.checked_add(input_stride.checked_mul(
                                        ExactRatio::integer(i64::from(*count - 1)),
                                    )?)?;
                                let footprint = ExactFrameRange {
                                    start: range.start,
                                    end,
                                };
                                contained(footprint, input_extent, false)?;
                                Ok(footprint)
                            })
                            .transpose()?;
                    RippleIndex {
                        extent,
                        footprint,
                        ends: vec![],
                        depth: child.depth + 1,
                    }
                }
            };
            check_depth(entry.depth)?;
            index.push(entry);
        }
        let root_index = root_index(root, index.len())?;
        let mut reachable = vec![false; nodes.len()];
        reachable[root_index] = true;
        for ordinal in (0..nodes.len()).rev() {
            if reachable[ordinal] {
                match &nodes[ordinal] {
                    SoundRippleNode::Sequence { parts } => {
                        for part in parts {
                            reachable[*part as usize] = true;
                        }
                    }
                    SoundRippleNode::Repeat { body, .. } => reachable[*body as usize] = true,
                    _ => {}
                }
            }
        }
        if reachable.contains(&false) {
            return Err(invalid("sound ripple contains unreachable nodes"));
        }
        let output_extent = index[root_index].extent;
        let map = Self {
            input_extent,
            output_extent,
            root,
            nodes,
            index,
        };
        map.to_json()?;
        Ok(map)
    }

    pub fn input_extent(&self) -> ExactRatio {
        self.input_extent
    }
    pub fn output_extent(&self) -> ExactRatio {
        self.output_extent
    }
    pub fn nodes(&self) -> &[SoundRippleNode] {
        &self.nodes
    }
    pub fn root(&self) -> u32 {
        self.root
    }

    /// Locate one complete Keep or Gap interval in this map's output clock.
    /// A Keep's `recipe` interval names this map's immediate input, including
    /// every enclosing Repeat stride, rather than a flattened recipe clock.
    /// Exact seams select the preceding or following interval according to
    /// `bias`. Outward endpoints have no provider; points outside the clock fail.
    /// An entirely gap-only subtree is returned as one complete Gap interval.
    pub fn locate(
        &self,
        at: ExactRatio,
        bias: InsertionBias,
        limits: SoundRouteQueryLimits,
    ) -> Result<SoundRoutePoint> {
        limits.validate()?;
        contained(
            ExactFrameRange { start: at, end: at },
            self.output_extent,
            true,
        )?;
        let mut work = QueryWork {
            limits,
            stats: SoundRouteQueryStats::default(),
            slices: vec![],
        };
        if (at == ExactRatio::ZERO && bias == InsertionBias::Left)
            || (at == self.output_extent && bias == InsertionBias::Right)
        {
            return Ok(SoundRoutePoint {
                slice: None,
                stats: work.stats,
            });
        }
        self.locate_node(at, bias, &mut work)?;
        let slice = work
            .slices
            .pop()
            .ok_or_else(|| invalid("sound ripple point has no selected interval"))?;
        Ok(SoundRoutePoint {
            slice: Some(slice),
            stats: work.stats,
        })
    }

    fn locate_node(
        &self,
        mut at: ExactRatio,
        bias: InsertionBias,
        work: &mut QueryWork,
    ) -> Result<()> {
        let mut id = self.root;
        let mut destination_shift = ExactRatio::ZERO;
        let mut input_shift = ExactRatio::ZERO;
        loop {
            work.visit()?;
            let index = &self.index[id as usize];
            let destination = ExactFrameRange {
                start: destination_shift,
                end: destination_shift.checked_add(index.extent)?,
            };
            if index.footprint.is_none() {
                return work.push(destination, None);
            }
            match &self.nodes[id as usize] {
                SoundRippleNode::Keep { range } => {
                    return work.push(destination, Some(shift(*range, input_shift)?));
                }
                SoundRippleNode::Gap { .. } => return work.push(destination, None),
                SoundRippleNode::Sequence { parts } => {
                    let mut low = 0;
                    let mut high = index.ends.len();
                    while low < high {
                        work.spend()?;
                        let mid = low + (high - low) / 2;
                        let comparison = compare(index.ends[mid], at)?;
                        if comparison.is_lt()
                            || (comparison.is_eq() && bias == InsertionBias::Right)
                        {
                            low = mid + 1;
                        } else {
                            high = mid;
                        }
                    }
                    id = *parts
                        .get(low)
                        .ok_or_else(|| invalid("sound ripple point escaped Sequence extent"))?;
                    let origin = if low == 0 {
                        ExactRatio::ZERO
                    } else {
                        index.ends[low - 1]
                    };
                    at = at.checked_sub(origin)?;
                    destination_shift = destination_shift.checked_add(origin)?;
                }
                SoundRippleNode::Repeat {
                    body,
                    count,
                    input_stride,
                } => {
                    work.spend()?;
                    let period = self.index[*body as usize].extent;
                    let quotient = at.checked_div(period)?;
                    let mut ordinal = u32::try_from(quotient.floor())
                        .map_err(|_| DocumentError::from(TimeError::Overflow))?;
                    if bias == InsertionBias::Left && ordinal > 0 && quotient.denominator() == 1 {
                        ordinal -= 1;
                    }
                    if ordinal >= *count {
                        return Err(invalid("sound ripple point escaped Repeat extent"));
                    }
                    let ordinal = ExactRatio::integer(i64::from(ordinal));
                    let origin = period.checked_mul(ordinal)?;
                    at = at.checked_sub(origin)?;
                    destination_shift = destination_shift.checked_add(origin)?;
                    input_shift = input_shift.checked_add(input_stride.checked_mul(ordinal)?)?;
                    id = *body;
                }
            }
        }
    }

    pub fn from_json(json: &str) -> Result<Self> {
        byte_count(json.len())?;
        let wire: RippleWire = serde_json::from_str(json).map_err(DocumentError::json)?;
        let map = Self::new(wire.input_extent, wire.root, wire.nodes)?;
        if map.output_extent != wire.output_extent {
            return Err(invalid(
                "sound ripple output extent disagrees with its arena",
            ));
        }
        Ok(map)
    }

    pub fn to_json(&self) -> Result<String> {
        bounded_json(self)
    }
}

impl<'de> Deserialize<'de> for SoundRippleMap {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        // Internally tagged route nodes buffer this value through serde's
        // content deserializer, which cannot deserialize RawValue. from_json
        // bounds bytes before parsing; embedding callers must bound their own
        // input. Every map validates collections, extents and serialized size.
        let wire = RippleWire::deserialize(deserializer)?;
        let map = Self::new(wire.input_extent, wire.root, wire.nodes)
            .map_err(serde::de::Error::custom)?;
        if map.output_extent != wire.output_extent {
            return Err(serde::de::Error::custom(
                "sound ripple output extent disagrees with its arena",
            ));
        }
        Ok(map)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SoundRouteNode {
    Recipe {},
    /// Select a previous output window and normalize its output origin to zero.
    Window {
        input: u32,
        selection: ExactFrameRange,
    },
    Ripple {
        input: u32,
        map: SoundRippleMap,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RouteIndex {
    extent: ExactRatio,
}

/// Validated immutable route. The retained recipe extent never changes when
/// selecting or editing output. Every reported non-gap slice is unity-rate.
/// Use `from_json` for external input. Serde embedding validates the resulting
/// route but requires its caller to bound the containing input before parsing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SoundRoute {
    recipe_extent: ExactRatio,
    output_extent: ExactRatio,
    root: u32,
    nodes: Vec<SoundRouteNode>,
    #[serde(skip)]
    index: Vec<RouteIndex>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RouteWire {
    recipe_extent: ExactRatio,
    output_extent: ExactRatio,
    root: u32,
    #[serde(deserialize_with = "bounded_vec")]
    nodes: Vec<SoundRouteNode>,
}

impl SoundRoute {
    pub fn identity(recipe_extent: ExactRatio) -> Result<Self> {
        Self::new(recipe_extent, 0, vec![SoundRouteNode::Recipe {}])
    }

    pub fn new(recipe_extent: ExactRatio, root: u32, nodes: Vec<SoundRouteNode>) -> Result<Self> {
        positive(recipe_extent)?;
        node_count(nodes.len())?;
        let mut index: Vec<RouteIndex> = Vec::with_capacity(nodes.len());
        let mut total_nodes = nodes.len();
        let mut edges = 0usize;
        for (ordinal, node) in nodes.iter().enumerate() {
            let entry = match node {
                SoundRouteNode::Recipe {} => RouteIndex {
                    extent: recipe_extent,
                },
                SoundRouteNode::Window { input, selection } => {
                    add_count(&mut edges, 1, MAX_SOUND_ROUTE_EDGES)?;
                    let child = &index[earlier(*input, ordinal)?];
                    contained(*selection, child.extent, false)?;
                    RouteIndex {
                        extent: length(*selection)?,
                    }
                }
                SoundRouteNode::Ripple { input, map } => {
                    add_count(&mut edges, 1, MAX_SOUND_ROUTE_EDGES)?;
                    add_count(&mut total_nodes, map.nodes.len(), MAX_SOUND_ROUTE_NODES)?;
                    for map_node in &map.nodes {
                        add_count(
                            &mut edges,
                            match map_node {
                                SoundRippleNode::Sequence { parts } => parts.len(),
                                SoundRippleNode::Repeat { .. } => 1,
                                _ => 0,
                            },
                            MAX_SOUND_ROUTE_EDGES,
                        )?;
                    }
                    let child = &index[earlier(*input, ordinal)?];
                    if child.extent != map.input_extent {
                        return Err(invalid(
                            "sound ripple input extent differs from previous route output",
                        ));
                    }
                    RouteIndex {
                        extent: map.output_extent,
                    }
                }
            };
            index.push(entry);
        }
        let root_index = root_index(root, index.len())?;
        let mut reachable = vec![false; nodes.len()];
        reachable[root_index] = true;
        for ordinal in (0..nodes.len()).rev() {
            if reachable[ordinal] {
                match &nodes[ordinal] {
                    SoundRouteNode::Window { input, .. } | SoundRouteNode::Ripple { input, .. } => {
                        reachable[*input as usize] = true
                    }
                    SoundRouteNode::Recipe {} => {}
                }
            }
        }
        if reachable.contains(&false) {
            return Err(invalid("sound route contains unreachable nodes"));
        }
        let output_extent = index[root_index].extent;
        let route = Self {
            recipe_extent,
            output_extent,
            root,
            nodes,
            index,
        };
        route.to_json()?;
        Ok(route)
    }

    pub fn recipe_extent(&self) -> ExactRatio {
        self.recipe_extent
    }
    pub fn output_extent(&self) -> ExactRatio {
        self.output_extent
    }
    pub fn root(&self) -> u32 {
        self.root
    }
    pub fn nodes(&self) -> &[SoundRouteNode] {
        &self.nodes
    }

    /// Each chronological route node retains its own exact output extent.
    /// Consumers can bind that clock once without flattening subsequent edits.
    pub fn node_extent(&self, id: u32) -> Option<ExactRatio> {
        self.index
            .get(usize::try_from(id).ok()?)
            .map(|entry| entry.extent)
    }

    pub fn window(&self, selection: ExactFrameRange) -> Result<Self> {
        contained(selection, self.output_extent, false)?;
        let mut nodes = self.nodes.clone();
        nodes.push(SoundRouteNode::Window {
            input: self.root,
            selection,
        });
        Self::new(self.recipe_extent, new_root(nodes.len())?, nodes)
    }

    pub fn ripple(&self, map: SoundRippleMap) -> Result<Self> {
        let mut nodes = self.nodes.clone();
        nodes.push(SoundRouteNode::Ripple {
            input: self.root,
            map,
        });
        Self::new(self.recipe_extent, new_root(nodes.len())?, nodes)
    }

    pub fn from_json(json: &str) -> Result<Self> {
        byte_count(json.len())?;
        let wire: RouteWire = serde_json::from_str(json).map_err(DocumentError::json)?;
        let route = Self::new(wire.recipe_extent, wire.root, wire.nodes)?;
        if route.output_extent != wire.output_extent {
            return Err(invalid(
                "sound route output extent disagrees with its arena",
            ));
        }
        Ok(route)
    }

    pub fn to_json(&self) -> Result<String> {
        bounded_json(self)
    }

    pub fn query(
        &self,
        range: ExactFrameRange,
        limits: SoundRouteQueryLimits,
    ) -> Result<SoundRouteQuery> {
        limits.validate()?;
        contained(range, self.output_extent, true)?;
        let mut work = QueryWork {
            limits,
            stats: SoundRouteQueryStats::default(),
            slices: vec![],
        };
        if range.start != range.end {
            self.query_node(self.root, range, ExactRatio::ZERO, &mut work)?;
        }
        Ok(SoundRouteQuery {
            range,
            slices: work.slices,
            stats: work.stats,
        })
    }

    fn query_node(
        &self,
        id: u32,
        range: ExactFrameRange,
        destination_shift: ExactRatio,
        work: &mut QueryWork,
    ) -> Result<()> {
        // History is a reachable linear chain bounded by the arena count, not
        // Ripple's structural nesting limit. Continuations visit one Sequence
        // child or Repeat ordinal at a time; a long play run cannot fill this
        // stack before the shared query-work budget is checked.
        let mut pending = vec![RouteTask::Route {
            id,
            range,
            destination_shift,
        }];
        while let Some(task) = pending.pop() {
            match task {
                RouteTask::Route {
                    id,
                    range,
                    destination_shift,
                } => {
                    work.visit()?;
                    match &self.nodes[id as usize] {
                        SoundRouteNode::Recipe {} => {
                            work.push(shift(range, destination_shift)?, Some(range))?
                        }
                        SoundRouteNode::Window { input, selection } => {
                            pending.push(RouteTask::Route {
                                id: *input,
                                range: shift(range, selection.start)?,
                                destination_shift: destination_shift
                                    .checked_sub(selection.start)?,
                            })
                        }
                        SoundRouteNode::Ripple { input, map } => pending.push(RouteTask::Map {
                            query: RippleQuery { input: *input, map },
                            id: map.root,
                            range,
                            input_shift: ExactRatio::ZERO,
                            destination_shift,
                        }),
                    }
                }
                RouteTask::Map {
                    query,
                    id,
                    range,
                    input_shift,
                    destination_shift,
                } => {
                    work.visit()?;
                    let map = query.map;
                    if map.index[id as usize].footprint.is_none() {
                        work.push(shift(range, destination_shift)?, None)?;
                        continue;
                    }
                    match &map.nodes[id as usize] {
                        SoundRippleNode::Keep { range: kept } => {
                            let offset = kept.start.checked_add(input_shift)?;
                            pending.push(RouteTask::Route {
                                id: query.input,
                                range: shift(range, offset)?,
                                destination_shift: destination_shift.checked_sub(offset)?,
                            });
                        }
                        SoundRippleNode::Gap { .. } => {
                            work.push(shift(range, destination_shift)?, None)?
                        }
                        SoundRippleNode::Sequence { .. } => {
                            let ends = &map.index[id as usize].ends;
                            let mut low = 0;
                            let mut high = ends.len();
                            while low < high {
                                work.spend()?;
                                let mid = low + (high - low) / 2;
                                if compare(ends[mid], range.start)?.is_le() {
                                    low = mid + 1;
                                } else {
                                    high = mid;
                                }
                            }
                            pending.push(RouteTask::Sequence {
                                query,
                                id,
                                position: low,
                                range,
                                input_shift,
                                destination_shift,
                            });
                        }
                        SoundRippleNode::Repeat { body, .. } => {
                            let period = map.index[*body as usize].extent;
                            let iteration = u32::try_from(range.start.checked_div(period)?.floor())
                                .map_err(|_| DocumentError::from(TimeError::Overflow))?;
                            pending.push(RouteTask::Repeat {
                                query,
                                id,
                                iteration,
                                range,
                                input_shift,
                                destination_shift,
                            });
                        }
                    }
                }
                RouteTask::Sequence {
                    query,
                    id,
                    position,
                    range,
                    input_shift,
                    destination_shift,
                } => {
                    work.spend()?;
                    let SoundRippleNode::Sequence { parts } = &query.map.nodes[id as usize] else {
                        unreachable!("Sequence continuation");
                    };
                    let ends = &query.map.index[id as usize].ends;
                    let origin = if position == 0 {
                        ExactRatio::ZERO
                    } else {
                        ends[position - 1]
                    };
                    let end = minimum(ends[position], range.end)?;
                    if compare(end, range.end)?.is_lt() {
                        pending.push(RouteTask::Sequence {
                            query,
                            id,
                            position: position + 1,
                            range: ExactFrameRange {
                                start: end,
                                end: range.end,
                            },
                            input_shift,
                            destination_shift,
                        });
                    }
                    pending.push(RouteTask::Map {
                        query,
                        id: parts[position],
                        range: ExactFrameRange {
                            start: range.start.checked_sub(origin)?,
                            end: end.checked_sub(origin)?,
                        },
                        input_shift,
                        destination_shift: destination_shift.checked_add(origin)?,
                    });
                }
                RouteTask::Repeat {
                    query,
                    id,
                    iteration,
                    range,
                    input_shift,
                    destination_shift,
                } => {
                    work.spend()?;
                    let SoundRippleNode::Repeat {
                        body,
                        count,
                        input_stride,
                    } = &query.map.nodes[id as usize]
                    else {
                        unreachable!("Repeat continuation");
                    };
                    if iteration >= *count {
                        return Err(invalid("sound route Repeat query escaped its extent"));
                    }
                    let period = query.map.index[*body as usize].extent;
                    let ordinal = ExactRatio::integer(i64::from(iteration));
                    let origin = period.checked_mul(ordinal)?;
                    let end = minimum(origin.checked_add(period)?, range.end)?;
                    if compare(end, range.end)?.is_lt() {
                        pending.push(RouteTask::Repeat {
                            query,
                            id,
                            iteration: iteration.checked_add(1).ok_or(TimeError::Overflow)?,
                            range: ExactFrameRange {
                                start: end,
                                end: range.end,
                            },
                            input_shift,
                            destination_shift,
                        });
                    }
                    let shifted_input =
                        input_shift.checked_add(input_stride.checked_mul(ordinal)?)?;
                    pending.push(RouteTask::Map {
                        query,
                        id: *body,
                        range: ExactFrameRange {
                            start: range.start.checked_sub(origin)?,
                            end: end.checked_sub(origin)?,
                        },
                        input_shift: shifted_input,
                        destination_shift: destination_shift.checked_add(origin)?,
                    });
                }
            }
            if pending.len() > MAX_SOUND_ROUTE_NODES + MAX_SOUND_ROUTE_DEPTH {
                return Err(limit("sound route query stack limit"));
            }
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for SoundRoute {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let wire = RouteWire::deserialize(deserializer)?;
        let route = Self::new(wire.recipe_extent, wire.root, wire.nodes)
            .map_err(serde::de::Error::custom)?;
        if route.output_extent != wire.output_extent {
            return Err(serde::de::Error::custom(
                "sound route output extent disagrees with its arena",
            ));
        }
        Ok(route)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoundRouteQueryLimits {
    pub maximum_work: usize,
    pub maximum_spans: usize,
}

impl Default for SoundRouteQueryLimits {
    fn default() -> Self {
        Self {
            maximum_work: 65_536,
            maximum_spans: 4096,
        }
    }
}

impl SoundRouteQueryLimits {
    fn validate(self) -> Result<()> {
        if self.maximum_work == 0
            || self.maximum_work > MAX_SOUND_ROUTE_QUERY_WORK
            || self.maximum_spans == 0
            || self.maximum_spans > MAX_SOUND_ROUTE_QUERY_SPANS
        {
            return Err(limit("invalid sound route query limits"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SoundRouteQueryStats {
    pub work: usize,
    pub node_visits: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoundRouteSlice {
    pub destination: ExactFrameRange,
    /// None denotes allocated output with no inherited contribution. Route
    /// queries name the retained recipe; ripple point lookup names map input.
    pub recipe: Option<ExactFrameRange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundRoutePoint {
    /// The complete selected leaf (or compressed Gap), not a point-sized crop.
    /// None denotes an outward endpoint, distinct from an allocated Gap slice.
    pub slice: Option<SoundRouteSlice>,
    pub stats: SoundRouteQueryStats,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundRouteQuery {
    pub range: ExactFrameRange,
    pub slices: Vec<SoundRouteSlice>,
    pub stats: SoundRouteQueryStats,
}

struct QueryWork {
    limits: SoundRouteQueryLimits,
    stats: SoundRouteQueryStats,
    slices: Vec<SoundRouteSlice>,
}

#[derive(Clone, Copy)]
struct RippleQuery<'a> {
    input: u32,
    map: &'a SoundRippleMap,
}

// Every entry is constant-sized; lazy continuations retain no expanded plays.
enum RouteTask<'a> {
    Route {
        id: u32,
        range: ExactFrameRange,
        destination_shift: ExactRatio,
    },
    Map {
        query: RippleQuery<'a>,
        id: u32,
        range: ExactFrameRange,
        input_shift: ExactRatio,
        destination_shift: ExactRatio,
    },
    Sequence {
        query: RippleQuery<'a>,
        id: u32,
        position: usize,
        range: ExactFrameRange,
        input_shift: ExactRatio,
        destination_shift: ExactRatio,
    },
    Repeat {
        query: RippleQuery<'a>,
        id: u32,
        iteration: u32,
        range: ExactFrameRange,
        input_shift: ExactRatio,
        destination_shift: ExactRatio,
    },
}

impl QueryWork {
    fn spend(&mut self) -> Result<()> {
        add_count(&mut self.stats.work, 1, self.limits.maximum_work)
    }
    fn visit(&mut self) -> Result<()> {
        self.spend()?;
        self.stats.node_visits += 1;
        Ok(())
    }
    fn push(
        &mut self,
        destination: ExactFrameRange,
        recipe: Option<ExactFrameRange>,
    ) -> Result<()> {
        self.spend()?;
        if let Some(last) = self.slices.last_mut()
            && last.destination.end == destination.start
        {
            match (&mut last.recipe, recipe) {
                (None, None) => {
                    last.destination.end = destination.end;
                    return Ok(());
                }
                (Some(previous), Some(next)) if previous.end == next.start => {
                    previous.end = next.end;
                    last.destination.end = destination.end;
                    return Ok(());
                }
                _ => {}
            }
        }
        if self.slices.len() >= self.limits.maximum_spans {
            return Err(limit("sound route query span limit"));
        }
        self.slices.push(SoundRouteSlice {
            destination,
            recipe,
        });
        Ok(())
    }
}

fn invalid(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::InvalidTree, message)
}
fn limit(message: &str) -> DocumentError {
    DocumentError::new(DocumentErrorCode::LimitExceeded, message)
}
fn positive(value: ExactRatio) -> Result<()> {
    crate::source_mapping::validate_duration(value).map_err(Into::into)
}
fn length(range: ExactFrameRange) -> Result<ExactRatio> {
    Ok(range.end.checked_sub(range.start)?)
}
fn compare(a: ExactRatio, b: ExactRatio) -> Result<Ordering> {
    Ok(a.checked_sub(b)?.compare_integer(0))
}
fn minimum(a: ExactRatio, b: ExactRatio) -> Result<ExactRatio> {
    Ok(if compare(a, b)?.is_le() { a } else { b })
}
fn shift(range: ExactFrameRange, offset: ExactRatio) -> Result<ExactFrameRange> {
    Ok(ExactFrameRange {
        start: range.start.checked_add(offset)?,
        end: range.end.checked_add(offset)?,
    })
}
fn contained(range: ExactFrameRange, extent: ExactRatio, empty: bool) -> Result<()> {
    if range.start.compare_integer(0).is_lt()
        || compare(range.end, extent)?.is_gt()
        || compare(range.start, range.end)?.is_gt()
        || (!empty && range.start == range.end)
    {
        return Err(invalid("sound route interval is outside its clock"));
    }
    Ok(())
}
fn earlier(id: u32, ordinal: usize) -> Result<usize> {
    let index = usize::try_from(id).map_err(|_| invalid("sound route index overflow"))?;
    if index >= ordinal {
        return Err(invalid("sound route references must precede their owner"));
    }
    Ok(index)
}
fn root_index(id: u32, length: usize) -> Result<usize> {
    let index = usize::try_from(id).map_err(|_| invalid("sound route root overflow"))?;
    if index >= length || index + 1 != length {
        return Err(invalid("sound route root must be the final arena entry"));
    }
    Ok(index)
}
fn new_root(length: usize) -> Result<u32> {
    u32::try_from(length - 1).map_err(|_| invalid("sound route arena overflow"))
}
fn add_count(total: &mut usize, count: usize, maximum: usize) -> Result<()> {
    *total = total
        .checked_add(count)
        .filter(|value| *value <= maximum)
        .ok_or_else(|| limit("sound route work or collection limit"))?;
    Ok(())
}
fn node_count(count: usize) -> Result<()> {
    if count == 0 || count > MAX_SOUND_ROUTE_NODES {
        return Err(limit("sound route node count"));
    }
    Ok(())
}
fn check_depth(depth: usize) -> Result<()> {
    if depth > MAX_SOUND_ROUTE_DEPTH {
        return Err(limit("sound route depth"));
    }
    Ok(())
}
fn byte_count(count: usize) -> Result<()> {
    if count > MAX_SOUND_ROUTE_JSON_BYTES {
        return Err(limit("sound route JSON byte limit"));
    }
    Ok(())
}

fn bounded_vec<'de, D, T>(deserializer: D) -> std::result::Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Visitor<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Visitor<T> {
        type Value = Vec<T>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a bounded sound route array")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> std::result::Result<Vec<T>, A::Error> {
            let mut output = Vec::new();
            while let Some(value) = sequence.next_element()? {
                if output.len() >= MAX_SOUND_ROUTE_EDGES {
                    return Err(serde::de::Error::custom("sound route array limit"));
                }
                output.push(value);
            }
            Ok(output)
        }
    }
    deserializer.deserialize_seq(Visitor(std::marker::PhantomData))
}

fn bounded_json(value: &impl Serialize) -> Result<String> {
    struct Writer(Vec<u8>);
    impl Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > MAX_SOUND_ROUTE_JSON_BYTES - self.0.len() {
                return Err(std::io::Error::other("sound route JSON byte limit"));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Writer(Vec::new());
    serde_json::to_writer(&mut writer, value).map_err(|error| {
        if error.is_io() {
            limit("sound route JSON byte limit")
        } else {
            DocumentError::json(error)
        }
    })?;
    String::from_utf8(writer.0).map_err(|_| invalid("sound route JSON is not UTF-8"))
}
