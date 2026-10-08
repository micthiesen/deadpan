//! Accepted Generated Hold recipes, built without a model.
//!
//! Each recipe starts from `black_pause` (a 12-frame silent Background Hold
//! `black` at Edit 15 between Original 26 and 27), fills it with the
//! synthetic worker (`deadpan_cli::generation::attempt::synthetic`) and
//! accepts that Ready bundle. Only the model is replaced: conditioning from
//! the committed boundary pictures, the durable attempt lifecycle, host bundle
//! qualification by `deadpan-media-worker`, generated-object publication and
//! `accept_generation_bundle` are the production path, so the accepted Hold
//! is an ordinary schema-3 Generated Hold with all six retained objects.
//! Later edits go through the ordinary headless `command` API.
//!
//! The sampled master has one picture per Hold frame, so Hold-local frame
//! `k` shows sampled frame `k`, in every Repeat play and after a shortening
//! (which reuses the accepted prefix without resampling).

use std::sync::atomic::AtomicBool;

use deadpan_cli::generation::acceptance;
use deadpan_cli::generation::attempt::{self, AllocateInput, synthetic};
use deadpan_cli::generation::conditioning;
use deadpan_core::{RepeatEditBranch, RepeatEditStep, ScopedNodeTarget};
use deadpan_jobs::{GenerationOptions, JobState};

use super::*;

/// The synthetic worker's tools: an `ffmpeg` with `libx264rgb`
/// (`DEADPAN_BRIDGE_FFMPEG`, else Homebrew's), `deadpan-media-worker`
/// (`DEADPAN_MEDIA_WORKER`, else beside the tested `deadpan-cli`) and sibling
/// `deadpan-track`.
pub(super) fn tools() -> std::result::Result<synthetic::SyntheticWorker, String> {
    let ffmpeg = super::development_ffmpeg()?;
    let media_worker = std::env::var_os("DEADPAN_MEDIA_WORKER")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(cli_path())
                .with_file_name("deadpan-media-worker")
                .to_path_buf()
        });
    if !media_worker.is_file() {
        return Err(format!(
            "needs a built deadpan-media-worker at {} (cargo build -p deadpan-media-worker with the same profile, or DEADPAN_MEDIA_WORKER)",
            media_worker.display()
        ));
    }
    let landmark_worker = media_worker.with_file_name("deadpan-track");
    if !landmark_worker.is_file() {
        return Err(format!(
            "needs a built deadpan-track beside deadpan-media-worker at {}",
            landmark_worker.display()
        ));
    }
    Ok(synthetic::SyntheticWorker {
        ffmpeg,
        media_worker,
        landmark_worker,
    })
}

/// The Hold every recipe fills.
const HOLD: &str = "black";
/// Edit frames of the Hold in `black_pause`.
const HOLD_START: u64 = 15;
const HOLD_FRAMES: u64 = 12;

/// Fill Hold `black` of a `black_pause` project with one synthetic Ready
/// variant seeded `seed` and accept it as the next revision.
fn accept_synthetic(dir: &Path, name: &'static str, seed: u64) -> Result<Project> {
    let project = synthetic_fallback(dir, name)?;
    accept_synthetic_target(
        project,
        seed,
        ScopedNodeTarget {
            node: node(HOLD)?,
            repeats: Vec::new(),
        },
    )
}

/// Build the deterministic pause before authoring its final context.
fn synthetic_fallback(dir: &Path, name: &'static str) -> Result<Project> {
    tools()?;
    let mut project = Project::create(dir, name)?;
    project.shorten()?;
    let at = ProjectFrame(HOLD_START as i64);
    project.apply(|project, document, revision| {
        let identities = match document.insert_time_target(at)?.split {
            Some(split) => project.fresh(split.required_ids)?,
            None => Vec::new(),
        };
        Ok(Command::InsertTime {
            at,
            hold: silent(HOLD_FRAMES as i64, HoldVideo::Background)?,
            id: node(HOLD)?,
            identities: SplitIdentities { nodes: identities },
            timing: AudioTimingId {
                allocation: revision.clone(),
                ordinal: 0,
            },
        })
    })?;
    Ok(project)
}

/// Qualify and accept only after the Hold's complete authored definition exists.
fn accept_synthetic_target(
    mut project: Project,
    seed: u64,
    target: ScopedNodeTarget,
) -> Result<Project> {
    let worker = tools()?;
    let hold = node(HOLD)?;
    let origin = project.document()?.revision_id().clone();
    let cancelled = AtomicBool::new(false);
    let inputs = conditioning::prepare_scoped_with_options(
        &project.package,
        &origin,
        &target,
        &GenerationOptions::default(),
        &cancelled,
    )?;
    let mut store = ProjectStore::open(&project.package, AccessMode::ReadWrite)?;
    let allocated = attempt::allocate_scoped_with_provider(
        &mut store,
        AllocateInput {
            hold: hold.clone(),
            expected_revision: origin,
            seed,
            inputs: inputs.into(),
        },
        target,
        deadpan_cli::generation::development_provider(seed),
    )?;
    let run = synthetic::run(
        &allocated,
        &worker,
        |_| {},
        |record| {
            attempt::record(&mut store, &allocated, &record).map_err(|error| error.to_string())
        },
        &cancelled,
    );
    let finished = attempt::finish(&mut store, &allocated, run)?;
    if finished.state != JobState::Ready {
        return Err(format!(
            "synthetic attempt ended {:?}: {:?}",
            finished.state, finished.failure
        )
        .into());
    }
    let accepted = project.next_revision()?;
    acceptance::accept(&mut store, &allocated.request.request_id, accepted.clone())?;
    drop(store);
    let saved = project.document()?;
    if saved.revision_id() != &accepted {
        return Err(format!("{accepted} was not committed").into());
    }
    match &saved.nodes()[&hold].kind {
        NodeKind::Hold { recipe } if matches!(recipe.video, HoldVideo::Generated { .. }) => {}
        other => return Err(format!("the Hold was not accepted: {other:?}").into()),
    }
    if !ProjectStore::open(&project.package, AccessMode::ReadOnly)?
        .generation_preparations(None, 64)?
        .is_empty()
    {
        return Err("synthetic acceptance unexpectedly queued a boundary replacement".into());
    }
    Ok(project)
}

fn generated(sampled_frame: u64) -> Expected {
    Expected::Generated { sampled_frame }
}

const ROWS: [&str; 2] = ["Living stare", "`,a` AI hold"];

/// The accepted Generated Hold in place: Edit [15, 27) shows sampled frames
/// 0..12 between Original 26 and 27. 42 frames.
pub fn generated_pause(dir: &Path) -> Result<Fixture> {
    let project = accept_synthetic(dir, "generated-pause", 11)?;
    let mut expectations = vec![(0, base(0)), (14, original(26))];
    for k in [0, 1, 5, 6, 10, 11] {
        expectations.push((HOLD_START + k, generated(k)));
    }
    expectations.extend([(27, original(27)), (29, original(29)), (41, original(41))]);
    let mut fixture = project.finish(ROWS.to_vec(), expectations, Vec::new())?;
    // The Hold is silent; the click (Edit sample 28,781 of the base) follows
    // it 12 frames (19,219.2 samples) later.
    fixture.audio = vec![(30_000, false), (47_795, true)];
    Ok(fixture)
}

/// Two plays of a local definition containing Original 26, the 12-frame Hold,
/// and Original 27, with a silent 4-frame Background gap. Its real endpoints
/// exist before Default generation and acceptance. Edit [15, 27) and [33, 45)
/// show sampled 0..12; [28, 32) is Background. 14 + 2·14 + 4 + 14 = 60 frames.
pub fn generated_repeat(dir: &Path) -> Result<Fixture> {
    let mut project = synthetic_fallback(dir, "generated-repeat")?;
    project.split_root(14)?;
    project.split_root(28)?;
    project.apply(|_, document, _| {
        let children = root_children(document)?;
        let first = root_child_at(document, 14)?.0;
        let last = root_child_at(document, 27)?.0;
        Ok(Command::Group {
            parent: document.root().clone(),
            start: children
                .iter()
                .position(|node| node == &first)
                .ok_or("local first child")?,
            end: children
                .iter()
                .position(|node| node == &last)
                .ok_or("local last child")?
                + 1,
            id: node("ai-local")?,
            label: "AI pause with Original endpoints".into(),
        })
    })?;
    project.apply(|_, _, _| {
        Ok(Command::WrapRepeat {
            node: node("ai-local")?,
            id: node("ai-repeat")?,
            plays: 2,
            gap: Some(silent(4, HoldVideo::Background)?),
            anchor_policy: WrapAnchorPolicy::First,
        })
    })?;
    let project = accept_synthetic_target(
        project,
        12,
        ScopedNodeTarget {
            node: node(HOLD)?,
            repeats: vec![RepeatEditStep {
                repeat: node("ai-repeat")?,
                branch: RepeatEditBranch::Default,
            }],
        },
    )?;
    let mut expectations = vec![(0, base(0)), (14, original(26))];
    for play in [HOLD_START, HOLD_START + HOLD_FRAMES + 6] {
        for k in [0, 1, 6, 11] {
            expectations.push((play + k, generated(k)));
        }
    }
    expectations.extend([
        (27, original(27)),
        (28, Expected::Background),
        (31, Expected::Background),
        (32, original(26)),
        (45, original(27)),
        (46, original(28)),
        (59, original(41)),
    ]);
    let mut fixture = project.finish(
        vec!["Living stare", "Repeat operator"],
        expectations,
        vec![
            "Repeat Default is admitted after authoring its one-frame Original endpoint handles."
                .into(),
        ],
    )?;
    // Thirty inserted frames (48,048 samples) move the base click at sample
    // 28,781 to 76,829. Both generated Holds and the gap remain silent.
    fixture.audio = vec![
        (30_000, false),
        (45_000, false),
        (60_000, false),
        (76_700, true),
    ];
    Ok(fixture)
}

/// The accepted Hold reframed by a static 1.35x zoom about its center (live
/// Hold framing over generated pictures). Timing and provenance as
/// `generated-pause`. 42 frames.
pub fn generated_reframe(dir: &Path) -> Result<Fixture> {
    let mut project = accept_synthetic(dir, "generated-reframe", 13)?;
    let half = ratio(1, 2)?;
    let zoomed = FramingPose::new(half, half, ratio(27, 20)?)?;
    project.apply(|_, _, _| {
        Ok(Command::SetFraming {
            node: node(HOLD)?,
            framing: Some(Framing::static_pose(zoomed)?),
        })
    })?;
    let mut expectations = vec![(14, original(26))];
    for k in [0, 5, 11] {
        expectations.push((HOLD_START + k, generated(k)));
    }
    expectations.extend([(27, original(27)), (41, original(41))]);
    let mut fixture =
        project.finish(vec!["Living stare", "Smash zoom"], expectations, Vec::new())?;
    fixture.audio = vec![(30_000, false), (47_795, true)];
    Ok(fixture)
}

/// The accepted Hold shortened to 7 frames, reusing the accepted sampled
/// prefix: Edit [15, 22) shows sampled 0..7, then Original 27..42. 37 frames.
pub fn generated_prefix(dir: &Path) -> Result<Fixture> {
    let mut project = accept_synthetic(dir, "generated-prefix", 14)?;
    project.apply(|_, _, _| {
        Ok(Command::SetHoldDuration {
            node: node(HOLD)?,
            duration: FrameDuration::new(7)?,
        })
    })?;
    let mut expectations = vec![(14, original(26))];
    for k in [0, 3, 6] {
        expectations.push((HOLD_START + k, generated(k)));
    }
    expectations.extend([(22, original(27)), (36, original(41))]);
    let mut fixture = project.finish(ROWS.to_vec(), expectations, Vec::new())?;
    // 7 inserted frames (11,211.2 samples) before the click.
    fixture.audio = vec![(30_000, false), (39_900, true)];
    Ok(fixture)
}
