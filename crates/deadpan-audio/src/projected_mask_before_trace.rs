//! Test-only source-ready receipts for the pre-G routed projected-mask seam.
use super::*;
use serde::Serialize;
use std::cell::RefCell;

const MAX_ROWS: usize = 128;
const MAX_SOURCE_RANGES: usize = 512;

thread_local! {
    static CAPTURE: RefCell<Option<Snapshot>> = const { RefCell::new(None) };
}

#[derive(Default, Serialize)]
pub(super) struct Snapshot {
    pub attempted: usize,
    pub dropped: usize,
    pub rows: Vec<Row>,
}

#[derive(Serialize)]
pub(super) struct Row {
    pub start: i64,
    pub frames: usize,
    pub source_len: usize,
    pub source_capacity: usize,
    pub source_ranges: Vec<[i64; 2]>,
    pub source_range_count: usize,
    pub source_ranges_truncated: bool,
    pub source_allocation_nonempty: bool,
    pub prepared_relative_depth: usize,
    pub prepared_dependencies: usize,
    pub cached_stages: usize,
    pub cached_resident_frames: u64,
    pub active_frames: u64,
    pub projected_resident_frames: u64,
    pub projected_disk_bytes: u64,
    pub account_live_frames: u64,
    pub plan_work_after_suppression: usize,
    pub cache_plan_work_after_suppression: usize,
    // Kept only for in-process comparison; never included in the capture.
    #[allow(dead_code)]
    #[serde(skip)]
    source_pointer: usize,
}

impl Row {
    pub(super) fn allocation_is_distinct_from(&self, destination: &SignalSuppressionMask) -> bool {
        self.source_capacity > 0
            && destination.is_owned()
            && self.source_pointer != destination.as_slice().as_ptr() as usize
    }
}

pub(super) fn begin() {
    CAPTURE.with(|capture| {
        *capture.borrow_mut() = Some(Snapshot {
            rows: Vec::new(),
            ..Snapshot::default()
        });
    });
}

pub(super) fn take() -> Snapshot {
    CAPTURE.with(|capture| capture.borrow_mut().take().unwrap_or_default())
}

pub(super) fn record(
    renderer: &StageAudio,
    control: WorkControl<'_>,
    start: SignalSample,
    frames: u32,
    prepared: &PreparedBlock,
    source: &Vec<Range<SignalSample>>,
) {
    CAPTURE.with(|capture| {
        let mut capture = capture.borrow_mut();
        let Some(snapshot) = capture.as_mut() else {
            return;
        };
        snapshot.attempted = snapshot.attempted.saturating_add(1);
        if snapshot.rows.len() >= MAX_ROWS {
            snapshot.dropped = snapshot.dropped.saturating_add(1);
            return;
        }
        let work = control.work.borrow();
        let copied_ranges = source.len().min(MAX_SOURCE_RANGES);
        snapshot.rows.push(Row {
            start: start.0,
            frames: frames as usize,
            source_len: source.len(),
            source_capacity: source.capacity(),
            source_ranges: source
                .iter()
                .take(copied_ranges)
                .map(|range| [range.start.0, range.end.0])
                .collect(),
            source_range_count: source.len(),
            source_ranges_truncated: source.len() > MAX_SOURCE_RANGES,
            source_allocation_nonempty: source.capacity() > 0,
            prepared_relative_depth: prepared.relative_depth,
            prepared_dependencies: prepared.dependencies.len(),
            cached_stages: renderer.cache.len(),
            cached_resident_frames: renderer.cached_resident_frames(),
            active_frames: renderer.active_frames,
            projected_resident_frames: work.projected_resident_frames,
            projected_disk_bytes: work.projected_disk_bytes,
            account_live_frames: renderer.owned_suppression.live_frames(),
            plan_work_after_suppression: work.plan_work,
            cache_plan_work_after_suppression: work.cache_plan_work,
            source_pointer: source.as_ptr() as usize,
        });
    });
}
