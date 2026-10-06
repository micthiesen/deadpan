//! Decode-ahead at upcoming playback discontinuities.
//!
//! An edit's picture plan is immutable for one revision, so the next picture
//! that does not continue the Original sequentially (a cut, a Repeat restart,
//! a jump after a Hold) is known before playback reaches it. A
//! [`LookAhead`] keeps at most one companion decoder per Original
//! ([`SourceSession::companion`]: the same verified bytes, index and
//! complete-measurement state) and positions it there on its own thread: a
//! keyframe seek, then forward decoding to the exact picture, one native
//! decoder call between stop checks. The active decoder keeps serving
//! sequential pictures meanwhile. When playback reaches the discontinuity,
//! the companion's plan is cheaper and the two swap roles: the companion
//! becomes the serving decoder and the previous one becomes the companion
//! for the next discontinuity.
//!
//! Only which decoder serves a picture changes. The picture, its identity and
//! its index check are those of [`SourceSession::frame`]; nothing here
//! returns pixels, and the heard clock, requests and generations stay the
//! caller's. Positioning never waits for the serving decoder or blocks its
//! picture: a request that reaches the discontinuity before the companion
//! waits at most for its current native call, and only when the serving
//! decoder would otherwise seek.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use deadpan_core::SourceFrameId;

use crate::source_session::{CompanionSpec, DecodePlan, RepositionProgress, SourceSession};

/// How far ahead of the requested picture discontinuities are looked for.
/// A 4K long-GOP keyframe seek takes about 150 ms; the horizon leaves room
/// for load and for discontinuities a few seconds apart.
pub const LOOKAHEAD_HORIZON: Duration = Duration::from_secs(4);
/// The longest forward step that still counts as sequential playback (a
/// fast Retime): the serving decoder continues without a seek.
pub const MAX_FORWARD_STEP: u64 = 8;
/// Bound on picture lookups of one [`DiscontinuityScan::next`] call.
pub const MAX_SCAN_FRAMES: u64 = 1024;
/// A companion that failed to open this often is not opened again.
const MAX_OPEN_FAILURES: u32 = 2;
/// Relative work of one fully decoded and converted forward picture against
/// one preroll ordinal of a seek (non-reference pictures skipped, nothing
/// converted); see `playback_pictures`.
const FORWARD_WEIGHT: u64 = 4;
/// Fixed work of a keyframe seek beyond its ordinals: the demuxer seek, the
/// decoder flush and the keyframe itself.
const SEEK_OVERHEAD: u64 = 2 * FORWARD_WEIGHT;

/// Whether playback from Original picture `previous` to `next` continues on
/// the same decoder without a seek: the same picture (a Hold) or a short
/// forward step.
pub fn continuous(previous: SourceFrameId, next: SourceFrameId) -> bool {
    next.0 >= previous.0 && next.0 - previous.0 <= MAX_FORWARD_STEP
}

/// A project picture that playback reaches only through a seek of the
/// Original: its frame and the Original ordinal it shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Discontinuity {
    pub frame: i64,
    pub target: SourceFrameId,
}

#[derive(Clone, Copy, Debug)]
struct Scanned {
    /// The frame the scan started from.
    start: i64,
    /// Every frame in `(start, through]` was examined.
    through: i64,
    /// The last Original ordinal at or before `through`.
    last: Option<SourceFrameId>,
    found: Option<Discontinuity>,
}

/// Finds the next discontinuity of one immutable picture plan, reusing the
/// examined run while playback moves through it. Reset it whenever the plan
/// (revision) changes.
#[derive(Clone, Debug, Default)]
pub struct DiscontinuityScan {
    state: Option<Scanned>,
}

impl DiscontinuityScan {
    pub fn reset(&mut self) {
        self.state = None;
    }

    /// The first picture after `from`, through `through`, whose Original
    /// ordinal does not continue (see [`continuous`]) from the previous
    /// picture that shows this Original. `ordinal_at(frame)` is the ordinal
    /// shown at a project frame, None for a picture this decoder does not
    /// serve (Background, a generated pause, another asset); those are
    /// passed over. A first Original picture after only such pictures is a
    /// discontinuity.
    ///
    /// A call from inside the previously examined run before its
    /// discontinuity reuses it and examines only new frames. Every call
    /// examines at most [`MAX_SCAN_FRAMES`] frames.
    pub fn next<E>(
        &mut self,
        from: i64,
        through: i64,
        mut ordinal_at: impl FnMut(i64) -> Result<Option<SourceFrameId>, E>,
    ) -> Result<Option<Discontinuity>, E> {
        let limit = i64::try_from(MAX_SCAN_FRAMES).unwrap_or(i64::MAX);
        let reused = self.state.filter(|scanned| {
            scanned.start <= from
                && from <= scanned.through
                && scanned.found.is_none_or(|found| from < found.frame)
        });
        let mut scanned = match reused {
            Some(scanned) => scanned,
            None => Scanned {
                start: from,
                through: from,
                last: ordinal_at(from)?,
                found: None,
            },
        };
        let end = through.min(scanned.through.saturating_add(limit));
        while scanned.found.is_none() && scanned.through < end {
            let frame = scanned.through + 1;
            if let Some(ordinal) = ordinal_at(frame)? {
                if scanned.last.is_none_or(|last| !continuous(last, ordinal)) {
                    scanned.found = Some(Discontinuity {
                        frame,
                        target: ordinal,
                    });
                }
                scanned.last = Some(ordinal);
            }
            scanned.through = frame;
        }
        self.state = Some(scanned);
        Ok(scanned.found.filter(|found| found.frame <= through))
    }
}

fn plan_work(plan: DecodePlan) -> u64 {
    match plan {
        DecodePlan::Current => 0,
        DecodePlan::Forward { pictures } => pictures.saturating_mul(FORWARD_WEIGHT),
        DecodePlan::Resume { remaining, after } => {
            remaining.saturating_add(after.saturating_mul(FORWARD_WEIGHT))
        }
        DecodePlan::Seek { ordinals } => ordinals.saturating_add(SEEK_OVERHEAD),
    }
}

/// Whether the companion reaches a picture with less work than the serving
/// decoder. Ties keep the serving decoder.
pub fn prefer_companion(serving: DecodePlan, companion: DecodePlan) -> bool {
    plan_work(companion) < plan_work(serving)
}

/// Whether the serving decoder's plan is a seek or a forward decode too long
/// to count as sequential playback.
fn seeks(plan: DecodePlan) -> bool {
    match plan {
        DecodePlan::Seek { .. } => true,
        DecodePlan::Forward { pictures } => pictures > MAX_FORWARD_STEP,
        DecodePlan::Current | DecodePlan::Resume { .. } => false,
    }
}

/// Counts of one [`LookAhead`]'s work.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LookAheadStats {
    /// Companion decoders opened.
    pub opened: u64,
    /// Positioning jobs started.
    pub started: u64,
    /// Jobs that reached their target picture.
    pub reached: u64,
    /// Jobs stopped before their target (retargeted, cancelled, overtaken).
    pub stopped: u64,
    /// Jobs or opens that failed; a failed companion reopens its decoder on
    /// its next use, and repeated open failures disable the look-ahead.
    pub failed: u64,
    /// Pictures at which the companion became the serving decoder.
    pub swaps: u64,
}

const OPENING: u8 = 0;
const SEEKING: u8 = 1;
const PREROLL: u8 = 2;

enum Slot {
    Empty,
    Idle(Box<SourceSession>),
    Working(Job),
}

struct Job {
    target: SourceFrameId,
    stop: Arc<AtomicBool>,
    phase: Arc<AtomicU8>,
    handle: JoinHandle<Outcome>,
}

struct Outcome {
    session: Option<SourceSession>,
    opened: bool,
    result: JobResult,
}

enum JobResult {
    Reached,
    Stopped,
    OpenFailed,
    Failed,
}

/// At most one companion decoder of one Original and at most one thread
/// positioning it. Dropping it stops and joins that thread, which waits for
/// at most one native decoder call (or a cancellable open).
pub struct LookAhead {
    spec: CompanionSpec,
    timeout: Duration,
    slot: Slot,
    /// Where the next job positions the companion once the running one has
    /// stopped.
    wanted: Option<SourceFrameId>,
    open_failures: u32,
    /// The target whose positioning failed; not retried until the target
    /// changes, so a persistent failure costs one attempt per cut.
    failed_target: Option<SourceFrameId>,
    stats: LookAheadStats,
}

impl LookAhead {
    /// A look-ahead for the Original of `spec`. Nothing is opened until the
    /// first [`Self::request`]. `timeout` bounds each open and positioning.
    pub fn new(spec: CompanionSpec, timeout: Duration) -> Self {
        Self {
            spec,
            timeout,
            slot: Slot::Empty,
            wanted: None,
            open_failures: 0,
            failed_target: None,
            stats: LookAheadStats::default(),
        }
    }

    pub fn stats(&self) -> LookAheadStats {
        self.stats
    }

    /// Whether a companion decoder is open (idle or being positioned).
    pub fn holds_decoder(&self) -> bool {
        !matches!(self.slot, Slot::Empty)
    }

    /// The running job's target, if any.
    pub fn positioning(&self) -> Option<SourceFrameId> {
        match &self.slot {
            Slot::Working(job) => Some(job.target),
            _ => None,
        }
    }

    /// The idle companion's current picture, if any.
    pub fn positioned(&self) -> Option<SourceFrameId> {
        match &self.slot {
            Slot::Idle(session) => session.current(),
            _ => None,
        }
    }

    /// The running job's stop flag, which another thread may set to stop it
    /// within one native decoder call without waiting.
    pub fn stop_flag(&self) -> Option<Arc<AtomicBool>> {
        match &self.slot {
            Slot::Working(job) => Some(Arc::clone(&job.stop)),
            _ => None,
        }
    }

    /// Collect a finished job and start the wanted one. Never waits.
    pub fn poll(&mut self) {
        if let Slot::Working(job) = &self.slot
            && job.handle.is_finished()
        {
            self.finish();
        }
        self.start_wanted();
    }

    /// Position the companion at `target`, the next discontinuity's picture;
    /// None stops positioning and keeps an idle companion. A running job for
    /// another target is stopped (without waiting) and the new one starts
    /// when it has returned.
    pub fn request(&mut self, target: Option<SourceFrameId>) {
        self.poll();
        match (&self.slot, target) {
            // A job already stopped (by `cancel` or the mailbox) for this
            // target restarts once it has returned.
            (Slot::Working(job), Some(target)) if job.target == target => {
                self.wanted = job.stop.load(Ordering::Acquire).then_some(target);
            }
            (Slot::Working(job), _) => {
                job.stop.store(true, Ordering::Release);
                self.wanted = target;
            }
            (_, target) => {
                self.wanted = target;
                self.start_wanted();
            }
        }
    }

    /// Stop positioning without waiting; an open companion is kept.
    pub fn cancel(&mut self) {
        self.wanted = None;
        if let Slot::Working(job) = &self.slot {
            job.stop.store(true, Ordering::Release);
        }
    }

    /// Stop positioning, wait for the job's current native call and close
    /// the companion decoder, releasing its memory.
    pub fn release(&mut self) {
        self.cancel();
        self.finish();
        self.slot = Slot::Empty;
    }

    /// The companion, when the serving decoder would seek to picture `id`
    /// (its `serving` plan is a seek or a long forward decode) and the
    /// companion reaches it with less work; the caller serves `id` from it
    /// and hands the previous serving decoder back through [`Self::keep`].
    /// Sequential pictures always stay on the serving decoder, so playback
    /// passing a companion's target ordinal before its cut does not consume
    /// the positioned companion. A job still decoding toward a target at or
    /// before `id` is stopped and waited for (one native decoder call); a
    /// job still opening or seeking is not.
    pub fn take_for(&mut self, id: SourceFrameId, serving: DecodePlan) -> Option<SourceSession> {
        self.poll();
        if !seeks(serving) {
            return None;
        }
        if let Slot::Working(job) = &self.slot
            && job.target.0 <= id.0
            && job.phase.load(Ordering::Acquire) == PREROLL
        {
            job.stop.store(true, Ordering::Release);
            self.finish();
        }
        let Slot::Idle(session) = &self.slot else {
            return None;
        };
        if !prefer_companion(serving, session.decode_plan(id)) {
            return None;
        }
        let Slot::Idle(session) = std::mem::replace(&mut self.slot, Slot::Empty) else {
            return None;
        };
        self.stats.swaps += 1;
        deadpan_diagnostics::PLAYBACK_PICTURES
            .lookahead_swaps
            .increment();
        Some(*session)
    }

    /// Keep the previously serving decoder as the companion after
    /// [`Self::take_for`]. A companion already held keeps its place and
    /// `session` is closed, so at most one companion exists.
    pub fn keep(&mut self, session: SourceSession) {
        if matches!(self.slot, Slot::Empty) {
            self.slot = Slot::Idle(Box::new(session));
        }
    }

    /// Wait for the running job, if any, and keep its decoder.
    fn finish(&mut self) {
        let Slot::Working(job) = std::mem::replace(&mut self.slot, Slot::Empty) else {
            return;
        };
        let counters = &deadpan_diagnostics::PLAYBACK_PICTURES;
        let target = job.target;
        let Ok(outcome) = job.handle.join() else {
            self.failed_target = Some(target);
            self.stats.failed += 1;
            counters.lookahead_failed.increment();
            return;
        };
        if outcome.opened {
            self.stats.opened += 1;
            self.open_failures = 0;
        }
        match outcome.result {
            JobResult::Reached => self.stats.reached += 1,
            JobResult::Stopped => self.stats.stopped += 1,
            JobResult::OpenFailed => {
                self.open_failures += 1;
                self.stats.failed += 1;
                counters.lookahead_failed.increment();
            }
            JobResult::Failed => {
                self.failed_target = Some(target);
                self.stats.failed += 1;
                counters.lookahead_failed.increment();
            }
        }
        if let Some(session) = outcome.session {
            self.slot = Slot::Idle(Box::new(session));
        }
    }

    fn start_wanted(&mut self) {
        let Some(target) = self.wanted else {
            return;
        };
        if matches!(self.slot, Slot::Working(_)) {
            return;
        }
        self.wanted = None;
        if self.failed_target == Some(target) {
            return;
        }
        self.failed_target = None;
        let session = match std::mem::replace(&mut self.slot, Slot::Empty) {
            Slot::Idle(session) if session.current() == Some(target) => {
                self.slot = Slot::Idle(session);
                return;
            }
            Slot::Idle(session) => Some(*session),
            Slot::Empty | Slot::Working(_) => None,
        };
        if session.is_none() && self.open_failures >= MAX_OPEN_FAILURES {
            return;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let phase = Arc::new(AtomicU8::new(OPENING));
        let spec = self.spec.clone();
        let timeout = self.timeout;
        let spawned = {
            let stop = Arc::clone(&stop);
            let phase = Arc::clone(&phase);
            std::thread::Builder::new()
                .name("deadpan-lookahead".into())
                .spawn(move || position(&spec, session, target, timeout, &stop, &phase))
        };
        match spawned {
            Ok(handle) => {
                self.stats.started += 1;
                deadpan_diagnostics::PLAYBACK_PICTURES
                    .lookahead_started
                    .increment();
                self.slot = Slot::Working(Job {
                    target,
                    stop,
                    phase,
                    handle,
                });
            }
            // The decoder moved into the unspawned closure is closed.
            Err(_) => {
                self.stats.failed += 1;
                deadpan_diagnostics::PLAYBACK_PICTURES
                    .lookahead_failed
                    .increment();
            }
        }
    }
}

impl Drop for LookAhead {
    fn drop(&mut self) {
        self.release();
    }
}

/// The job: open the companion if needed, seek to the target's keyframe and
/// decode forward to the target, checking `stop` before every picture.
fn position(
    spec: &CompanionSpec,
    session: Option<SourceSession>,
    target: SourceFrameId,
    timeout: Duration,
    stop: &AtomicBool,
    phase: &AtomicU8,
) -> Outcome {
    let (mut session, opened) = match session {
        Some(session) => (session, false),
        None => match spec.open(timeout, stop) {
            Ok(session) => (session, true),
            Err(_) if stop.load(Ordering::Acquire) => {
                return Outcome {
                    session: None,
                    opened: false,
                    result: JobResult::Stopped,
                };
            }
            Err(_) => {
                return Outcome {
                    session: None,
                    opened: false,
                    result: JobResult::OpenFailed,
                };
            }
        },
    };
    let outcome = |session, result| Outcome {
        session: Some(session),
        opened,
        result,
    };
    if stop.load(Ordering::Acquire) {
        return outcome(session, JobResult::Stopped);
    }
    // Positioning is never cancelled mid-call (that would poison the
    // decoder); `stop` is observed between pictures.
    let never = AtomicBool::new(false);
    phase.store(SEEKING, Ordering::Release);
    if session.current() == Some(target) {
        deadpan_diagnostics::PLAYBACK_PICTURES
            .lookahead_reached
            .increment();
        return outcome(session, JobResult::Reached);
    }
    if session.repositioning() != Some(target)
        && session.begin_reposition(target, timeout, &never).is_err()
    {
        return outcome(session, JobResult::Failed);
    }
    phase.store(PREROLL, Ordering::Release);
    let progress =
        session.advance_reposition(timeout, &never, &mut || stop.load(Ordering::Acquire));
    match progress {
        Ok(RepositionProgress::Reached { .. }) => {
            // Counted as it happens, before the serving thread collects it.
            deadpan_diagnostics::PLAYBACK_PICTURES
                .lookahead_reached
                .increment();
            outcome(session, JobResult::Reached)
        }
        Ok(RepositionProgress::Pending { .. }) => outcome(session, JobResult::Stopped),
        Err(_) => outcome(session, JobResult::Failed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ordinals(values: &[Option<u64>]) -> impl FnMut(i64) -> Result<Option<SourceFrameId>, ()> {
        let values = values.to_vec();
        move |frame| Ok(values[frame as usize].map(SourceFrameId))
    }

    #[test]
    fn holds_short_steps_and_gaps_continue_and_jumps_do_not() {
        assert!(continuous(SourceFrameId(10), SourceFrameId(10)));
        assert!(continuous(SourceFrameId(10), SourceFrameId(11)));
        assert!(continuous(SourceFrameId(10), SourceFrameId(18)));
        assert!(!continuous(SourceFrameId(10), SourceFrameId(19)));
        assert!(!continuous(SourceFrameId(10), SourceFrameId(9)));
    }

    #[test]
    fn a_scan_finds_a_repeat_restart_and_passes_over_background() {
        // 0..4, a Background picture, 4..6 continuing, then a restart at 2.
        let plan = [
            Some(0),
            Some(1),
            Some(2),
            Some(3),
            None,
            Some(4),
            Some(5),
            Some(2),
            Some(3),
        ];
        let mut scan = DiscontinuityScan::default();
        let found = scan.next(0, 8, ordinals(&plan)).unwrap();
        assert_eq!(
            found,
            Some(Discontinuity {
                frame: 7,
                target: SourceFrameId(2)
            })
        );
        // Beyond the horizon: not reported, but not lost either.
        let mut scan = DiscontinuityScan::default();
        assert_eq!(scan.next(0, 5, ordinals(&plan)).unwrap(), None);
        assert_eq!(scan.next(1, 8, ordinals(&plan)).unwrap(), found);
    }

    #[test]
    fn a_scan_reuses_its_examined_run_and_restarts_outside_it() {
        let plan: Vec<Option<u64>> = (0..100).chain(10..60).map(Some).collect();
        let lookups = std::cell::Cell::new(0_usize);
        let mut scan = DiscontinuityScan::default();
        let mut counted = |frame: i64| -> Result<Option<SourceFrameId>, ()> {
            lookups.set(lookups.get() + 1);
            Ok(plan[frame as usize].map(SourceFrameId))
        };
        let expected = Some(Discontinuity {
            frame: 100,
            target: SourceFrameId(10),
        });
        assert_eq!(scan.next(0, 120, &mut counted).unwrap(), expected);
        assert_eq!(lookups.get(), 101);
        // Moving through the run costs no lookups.
        for from in 1..100 {
            assert_eq!(scan.next(from, from + 120, &mut counted).unwrap(), expected);
        }
        assert_eq!(lookups.get(), 101);
        // Past the discontinuity: a new scan from there.
        assert_eq!(scan.next(100, 149, &mut counted).unwrap(), None);
        assert_eq!(lookups.get(), 151);
        // A seek backward restarts.
        assert_eq!(scan.next(50, 120, &mut counted).unwrap(), expected);
        scan.reset();
        assert_eq!(scan.next(99, 120, &mut counted).unwrap(), expected);
    }

    #[test]
    fn a_scan_extends_an_unfinished_run_incrementally_within_its_bound() {
        let plan: Vec<Option<u64>> = (0..3000).map(Some).collect();
        let lookups = std::cell::Cell::new(0_usize);
        let mut scan = DiscontinuityScan::default();
        let mut counted = |frame: i64| -> Result<Option<SourceFrameId>, ()> {
            lookups.set(lookups.get() + 1);
            Ok(plan[frame as usize].map(SourceFrameId))
        };
        assert_eq!(scan.next(0, 2999, &mut counted).unwrap(), None);
        assert_eq!(lookups.get(), 1 + MAX_SCAN_FRAMES as usize);
        assert_eq!(scan.next(10, 1034, &mut counted).unwrap(), None);
        assert_eq!(lookups.get(), 1 + MAX_SCAN_FRAMES as usize + 10);
    }

    #[test]
    fn a_first_original_picture_after_background_is_a_discontinuity() {
        let plan = [None, None, Some(40), Some(41)];
        let mut scan = DiscontinuityScan::default();
        assert_eq!(
            scan.next(0, 3, ordinals(&plan)).unwrap(),
            Some(Discontinuity {
                frame: 2,
                target: SourceFrameId(40)
            })
        );
    }

    #[test]
    fn errors_leave_the_scan_unchanged() {
        let mut scan = DiscontinuityScan::default();
        assert_eq!(
            scan.next(0, 10, |_| Err::<Option<SourceFrameId>, _>(7)),
            Err(7)
        );
        let plan = [Some(0), Some(1), Some(9 + 1)];
        assert_eq!(
            scan.next(0, 2, ordinals(&plan)).unwrap(),
            Some(Discontinuity {
                frame: 2,
                target: SourceFrameId(10)
            })
        );
    }

    #[test]
    fn the_companion_is_preferred_only_with_less_work() {
        let seek = DecodePlan::Seek { ordinals: 120 };
        assert!(prefer_companion(seek, DecodePlan::Current));
        assert!(prefer_companion(seek, DecodePlan::Forward { pictures: 2 }));
        assert!(prefer_companion(
            seek,
            DecodePlan::Resume {
                remaining: 30,
                after: 1
            }
        ));
        // Sequential serving is kept, including on a tie.
        let forward = DecodePlan::Forward { pictures: 1 };
        assert!(!prefer_companion(
            forward,
            DecodePlan::Forward { pictures: 1 }
        ));
        assert!(!prefer_companion(forward, DecodePlan::Seek { ordinals: 1 }));
        assert!(prefer_companion(forward, DecodePlan::Current));
        assert!(!prefer_companion(DecodePlan::Current, DecodePlan::Current));
        // A short seek beats a long companion preroll.
        assert!(!prefer_companion(
            DecodePlan::Seek { ordinals: 3 },
            DecodePlan::Resume {
                remaining: 200,
                after: 1
            }
        ));
        assert!(seeks(seek));
        assert!(seeks(DecodePlan::Forward { pictures: 9 }));
        assert!(!seeks(DecodePlan::Forward { pictures: 8 }));
    }
}
