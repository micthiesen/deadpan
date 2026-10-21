//! Section 8 rows whose authored output had no export comparison: composed
//! recipes, gags, groups, registers, macros, dot-repeat, Trim/Slip/Roll,
//! per-play overrides and Camera on a Hold. Every fixture is built through
//! the public headless paths (`command`, `macro` save/run/apply/yank_original,
//! `sound`, `project insert-original`), and its expectations are derived from
//! the recipe semantics, never from the compiled plan.
//!
//! Sample positions use B(f) = round(f · 48000 · 1001 / 30000) = round(1601.6 f).
//! The base click (Original sample 48,000, Original frame 29.97) sounds at
//! Edit sample 28,781 of the shortened base; a moment starting at Original 24
//! holds it 9,562 samples after its own start.

use std::num::NonZeroU32;

use super::*;
use deadpan_core::{
    GagRecipe, PauseInsertionTarget, PauseLength, PitchPolicy, RepeatEditBranch, RepeatEditStep,
    ScopedNodeEdit, ScopedNodeTarget, SourceTrimCapture, SourceTrimIntent, SourceTrimPolicy,
    SourceTrimResources,
};

#[path = "section8/scoped_audio.rs"]
pub mod scoped_audio;

/// Edit sample of project frame `frame` (exact rounding of 1601.6 · frame).
fn boundary(frame: i64) -> i64 {
    (frame * 16_016 + 5) / 10
}

fn frames(count: u32) -> Result<PauseLength> {
    Ok(PauseLength::Frames {
        frames: NonZeroU32::new(count).ok_or("positive length")?,
    })
}

fn length(count: u32) -> Result<Value> {
    Ok(serde_json::to_value(frames(count)?)?)
}

fn plays(count: u32) -> Result<NonZeroU32> {
    Ok(NonZeroU32::new(count).ok_or("positive plays")?)
}

fn pose(x: (i128, i128), y: (i128, i128), scale: (i128, i128)) -> Result<FramingPose> {
    Ok(FramingPose::new(
        ratio(x.0, x.1)?,
        ratio(y.0, y.1)?,
        ratio(scale.0, scale.1)?,
    )?)
}

fn static_framing(pose: FramingPose) -> Result<Value> {
    Ok(serde_json::to_value(Framing::static_pose(pose)?)?)
}

/// A 512-sample left-channel window from 109 samples before a click onset
/// whose peak lies strictly inside `peak`.
fn click_peak(onset: i64, peak: (f64, f64)) -> Signal {
    Signal {
        start: onset - 109,
        count: 512,
        crossings_per_second: None,
        crest: None,
        peak: Some(peak),
    }
}

/// The base click (left peak about 0.75) after a -12 dB trim: about 0.188.
const ATTENUATED_CLICK: (f64, f64) = (0.17, 0.21);

/// The group whose label is exactly `label`.
fn labelled(document: &ProjectDocument, label: &str) -> Result<NodeId> {
    document
        .nodes()
        .iter()
        .find(|(_, node)| node.label == label)
        .map(|(id, _)| id.clone())
        .ok_or_else(|| format!("no node is labelled {label:?}").into())
}

/// False start (composed): Original `"by` of [24, 33) and `"ay` of its prefix
/// [24, 27), then `s` at Edit 15, `"bp` after the first half, `"aP` before
/// b and `,h 6f` between them, through the headless semantic apply. The speaker starts, stops on a freeze of the
/// prefix's last picture (Original 26), then starts again and says it all:
/// a [15, 18), pause [18, 24), b [24, 33), base resumes at Original 27:
/// 30 + 3 + 6 + 9 = 48 frames.
pub fn false_start(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "false-start")?;
    project.shorten()?;
    let document = project.split_root(15)?;
    let left = root_child_at(&document, 0)?.0;
    project.yank_original('b', 24, 33)?;
    project.yank_original('a', 24, 27)?;
    // `p` places after the selected beat and `P` before it; each paste
    // selects its new root with the cursor at its start.
    let saved = project.apply_semantic(
        json!([
            {"type":"paste","register":"b","before":false},
            {"type":"paste","register":"a","before":true},
            {"type":"move_frames","forward":true,"count":3},
            {"type":"insert_pause","length":length(6)?}
        ]),
        0,
        Some(&left),
    )?;
    if saved.duration()?.frames() != 48 {
        return Err(format!("false start is {} frames", saved.duration()?.frames()).into());
    }
    let expectations = vec![
        (0, base(0)),
        (14, original(26)),
        (15, original(24)),
        (17, original(26)),
        (18, original(26)),
        (23, original(26)),
        (24, original(24)),
        (32, original(32)),
        (33, original(27)),
        (47, original(41)),
    ];
    let mut fixture = project.finish(vec!["False start"], expectations, Vec::new())?;
    // The prefix (source [38,439, 43,243)) holds no click and the pause is
    // silent; b holds it 9,562 samples after B(24) = 38,438, at 48,000; the
    // base's own click moves 18 frames later, to 28,780.8 + 28,828.8.
    fixture.audio = vec![
        (24_576, false),
        (28_672, false),
        (32_000, false),
        (47_872, true),
        (57_472, true),
    ];
    Ok(fixture)
}

/// Interrupted answer (composed): `s` at Edit 15, Visual `d` of the ending
/// [22, 30), then `:cutaway register=r` (Original 90..96) over the remaining
/// [15, 22) of the answer while its sound continues: 22 frames.
pub fn interrupted_answer(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "interrupted-answer")?;
    project.shorten()?;
    project.split_root(15)?;
    let document = project.delete_range(22, 30)?;
    let (answer, start) = root_child_at(&document, 15)?;
    if start != 15 || document.duration()?.frames() != 22 {
        return Err("unexpected interrupted base".into());
    }
    project.yank_original('r', 90, 96)?;
    project.apply_semantic(
        json!([{"type":"set_cutaway","register":"r"}]),
        15,
        Some(&answer),
    )?;
    // Hold fit: Edit 15 + k shows Original 90 + min(k, 5).
    let expectations = vec![
        (0, base(0)),
        (14, base(14)),
        (15, original(90)),
        (18, original(93)),
        (20, original(95)),
        (21, original(95)),
    ];
    let mut fixture = project.finish(vec!["Interrupted answer"], expectations, Vec::new())?;
    // The answer's click still sounds under the reaction; the rest is quiet.
    fixture.audio = vec![(20_000, false), (28_672, true), (33_000, false)];
    Ok(fixture)
}

/// Callback: `"ay` of Visual Edit [12, 24) (Original 24..36), then at the
/// end of the edit `"ap`, in one headless semantic apply: the moment comes
/// back after the base, 30 + 12 = 42 frames.
pub fn callback_register(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "callback-register")?;
    project.shorten()?;
    project.apply_semantic(
        json!([
            {"type":"begin_selection"},
            {"type":"move_frames","forward":true,"count":12},
            {"type":"yank_selection","register":"a"},
            {"type":"clear_selection"},
            {"type":"move_scope","end":true},
            {"type":"paste","register":"a","before":false}
        ]),
        12,
        None,
    )?;
    let expectations = vec![
        (0, base(0)),
        (12, original(24)),
        (29, original(41)),
        (30, original(24)),
        (35, original(29)),
        (41, original(35)),
    ];
    let mut fixture = project.finish(
        vec!["Callback", "Paste from register", "Registers"],
        expectations,
        Vec::new(),
    )?;
    // The callback's click is 9,562 samples after B(30) = 48,048.
    fixture.audio = vec![(28_672, true), (50_000, false), (57_472, true)];
    Ok(fixture)
}

/// Reaction ping-pong (composed): three disjoint `:cutaway` ranges on the one
/// base host alternate the speaker with reactions (Original 90..95, 100..105,
/// 90..95 again) while the speaker's sound is kept: 30 frames.
pub fn reaction_ping_pong(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "reaction-ping-pong")?;
    project.shorten()?;
    let asset = project.asset.clone();
    project.apply(|_, document, _| {
        let (beat, start) = root_child_at(document, 0)?;
        let (host, offset) = source_host(document, &beat)?;
        let local = |edit: i64| offset + edit - start;
        let reaction = |from: i64, to: i64, first: i64| -> Result<Cutaway> {
            Ok(Cutaway {
                range: FrameRange::new(ProjectFrame(local(from)), ProjectFrame(local(to)))?,
                asset: asset.clone(),
                selection: ordinal_span(document, &asset, first, first + (to - from))?,
                fit: CutawayFit::Hold,
                removed: false,
            })
        };
        Ok(Command::SetCutaways {
            node: host,
            cutaways: vec![
                reaction(3, 8, 90)?,
                reaction(13, 18, 100)?,
                reaction(22, 27, 90)?,
            ],
        })
    })?;
    let expectations = vec![
        (2, base(2)),
        (3, original(90)),
        (7, original(94)),
        (8, base(8)),
        (12, base(12)),
        (13, original(100)),
        (17, original(104)),
        (18, base(18)),
        (21, base(21)),
        (22, original(90)),
        (26, original(94)),
        (27, base(27)),
        (29, base(29)),
    ];
    let mut fixture = project.finish(
        vec!["Reaction ping-pong", "Cutaways"],
        expectations,
        Vec::new(),
    )?;
    // The click at Edit 17.97 sounds under the second reaction.
    fixture.audio = vec![(20_000, false), (28_672, true)];
    Ok(fixture)
}

/// Abrupt return: the base split at Edit 15 with both halves at a static
/// 1.35x zoom, then `:zoom off` on the second half through the headless
/// semantic apply: a hard cut from the zoomed picture back to the full one.
pub fn abrupt_return(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "abrupt-return")?;
    project.shorten()?;
    project.split_root(15)?;
    let zoomed = pose((1, 2), (1, 2), (27, 20))?;
    for at in [0, 15] {
        project.apply(|_, document, _| {
            Ok(Command::SetFraming {
                node: root_child_at(document, at)?.0,
                framing: Some(Framing::static_pose(zoomed)?),
            })
        })?;
    }
    let second = root_child_at(&project.document()?, 15)?.0;
    let saved = project.apply_semantic(
        json!([{"type":"set_framing","framing":null}]),
        15,
        Some(&second),
    )?;
    if saved.nodes()[&second].framing.is_some()
        || saved.nodes()[&root_child_at(&saved, 0)?.0]
            .framing
            .is_none()
    {
        return Err("`:zoom off` did not return only the second half".into());
    }
    let expectations = [0u64, 14, 15, 29]
        .into_iter()
        .map(|frame| (frame, base(frame)))
        .collect();
    let mut fixture = project.finish(vec!["Abrupt return"], expectations, Vec::new())?;
    fixture.audio = vec![(28_672, true)];
    Ok(fixture)
}

/// `:gag long-answer pause=12f creep=1.35` at Edit 15 through the headless
/// semantic path: a 12-frame silent freeze of Original 26 with a smoothstep
/// creep to 1.35x, grouped under a label that pins recipe, version and
/// parameters: 30 + 12 = 42 frames. Version 2 is refused.
pub fn long_answer(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "long-answer")?;
    project.shorten()?;
    let recipe = GagRecipe::LongAnswer {
        version: 1,
        pause: frames(12)?,
        scale: ratio(27, 20)?,
    };
    let saved = project.run_semantic(json!([{"type":"gag","recipe":recipe}]), 15, None)?;
    let (group, _) = root_child_at(&saved, 15)?;
    let label = saved.nodes()[&group].label.clone();
    if label != "The Long Answer · v1 · pause 12f, creep to 1.350×"
        || GagRecipe::from_label(&label) != Some(recipe)
    {
        return Err(format!("unexpected gag label {label:?}").into());
    }
    let unavailable = GagRecipe::LongAnswer {
        version: 2,
        pause: frames(12)?,
        scale: ratio(27, 20)?,
    };
    let refusal = project.semantic_refusal(json!([{"type":"gag","recipe":unavailable}]), 15)?;
    if !refusal.contains("version 2 is not available") {
        return Err(format!("unexpected version refusal {refusal}").into());
    }
    let expectations = vec![
        (0, base(0)),
        (14, original(26)),
        (15, original(26)),
        (21, original(26)),
        (26, original(26)),
        (27, original(27)),
        (41, original(41)),
    ];
    let mut fixture = project.finish(
        vec![
            "The Long Answer",
            "Versioned definition, pinned version and parameters",
        ],
        expectations,
        vec![format!("pinned label {label:?}; version 2 refused")],
    )?;
    // The pause [24,024, 43,243) is silent; the click moves 12 frames later.
    fixture.audio = vec![(30_000, false), (40_000, false), (47_872, true)];
    Ok(fixture)
}

/// `:gag escalator plays=3 gain-step=3dB zoom-step=0.08` on the selected beat
/// Edit [12, 24) through the headless semantic path: three plays of Original
/// 24..36 with growing gain and scale: 12 + 36 + 6 = 54 frames.
pub fn escalator(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "escalator")?;
    project.shorten()?;
    project.split_root(12)?;
    let document = project.split_root(24)?;
    let beat = root_child_at(&document, 12)?.0;
    let recipe = GagRecipe::Escalator {
        version: 1,
        plays: plays(3)?,
        gain_step: GainDb::new(3_000)?,
        zoom_step: grid_step()?,
    };
    let saved = project.run_semantic(json!([{"type":"gag","recipe":recipe}]), 12, Some(&beat))?;
    let (group, _) = root_child_at(&saved, 12)?;
    let label = saved.nodes()[&group].label.clone();
    if label != "The Escalator · v1 · 3 plays, +3 dB and +0.080 scale per play"
        || GagRecipe::from_label(&label) != Some(recipe)
    {
        return Err(format!("unexpected gag label {label:?}").into());
    }
    let mut expectations = vec![(0, base(0)), (11, base(11))];
    for play in 0..3u64 {
        let start = 12 + 12 * play;
        expectations.extend([
            (start, original(24)),
            (start + 6, original(30)),
            (start + 11, original(35)),
        ]);
    }
    expectations.extend([(48, original(36)), (53, original(41))]);
    let mut fixture = project.finish(
        vec![
            "The Escalator",
            "Versioned definition, pinned version and parameters",
        ],
        expectations,
        vec![format!("pinned label {label:?}")],
    )?;
    // Each play's click, 9,562 samples after B(12), B(24) and B(36).
    fixture.audio = vec![
        (28_672, true),
        (35_000, false),
        (47_872, true),
        (55_000, false),
        (67_072, true),
    ];
    Ok(fixture)
}

/// `s` at Edit 15, then `:gag non-sequitur register=r` with the first half
/// selected and register `r` holding the Original moment [90, 96): a hard cut
/// to it and straight back after that beat, grouped:
/// 30 + 6 = 36 frames.
pub fn non_sequitur(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "non-sequitur")?;
    project.shorten()?;
    let document = project.split_root(15)?;
    let left = root_child_at(&document, 0)?.0;
    project.yank_original('r', 90, 96)?;
    let recipe = GagRecipe::NonSequitur {
        version: 1,
        register: RegisterName::new('r')?,
    };
    // Its paste goes after the selected beat, Edit [0, 15).
    let saved = project.run_semantic(json!([{"type":"gag","recipe":recipe}]), 0, Some(&left))?;
    let (group, _) = root_child_at(&saved, 15)?;
    let label = &saved.nodes()[&group].label;
    if label != "The Non-Sequitur · v1 · register r" {
        return Err(format!("unexpected gag label {label:?}").into());
    }
    let expectations = vec![
        (0, base(0)),
        (14, original(26)),
        (15, original(90)),
        (20, original(95)),
        (21, original(27)),
        (35, original(41)),
    ];
    let mut fixture = project.finish(
        vec!["The Non-Sequitur", "Paste from register"],
        expectations,
        Vec::new(),
    )?;
    // The moment's sound has no click; the base click moves 6 frames later,
    // to 28,780.8 + 9,609.6.
    fixture.audio = vec![(28_672, false), (38_272, true)];
    Ok(fixture)
}

/// `,g` / `:group name="the uncomfortable answer"` over Visual Edit [10, 20),
/// then a static 1.35x zoom and a -12 dB trim on the new group; and
/// `:group name=aside` over [22, 26) followed by `:ungroup`, all through the
/// headless semantic apply. Time and pictures are unchanged; the group's
/// trim attenuates the click it contains.
pub fn named_group(dir: &Path) -> Result<Fixture> {
    const LABEL: &str = "the uncomfortable answer";
    let mut project = Project::create(dir, "named-group")?;
    project.shorten()?;
    let saved = project.apply_semantic(
        json!([
            {"type":"begin_selection"},
            {"type":"move_frames","forward":true,"count":10},
            {"type":"group","selector":{"type":"visual_selection"},"label":LABEL}
        ]),
        10,
        None,
    )?;
    let group = labelled(&saved, LABEL)?;
    if root_child_at(&saved, 10)? != (group.clone(), 10) || root_child_at(&saved, 20)?.0 == group {
        return Err("the group does not own exactly Edit [10, 20)".into());
    }
    let zoomed = pose((1, 2), (1, 2), (27, 20))?;
    project.apply_semantic(
        json!([
            {"type":"set_framing","framing":static_framing(zoomed)?},
            {"type":"set_audio","change":{"type":"trim","gain":-12_000}}
        ]),
        10,
        Some(&group),
    )?;
    let saved = project.apply_semantic(
        json!([
            {"type":"begin_selection"},
            {"type":"move_frames","forward":true,"count":4},
            {"type":"group","selector":{"type":"visual_selection"},"label":"aside"},
            {"type":"clear_selection"},
            {"type":"ungroup"}
        ]),
        22,
        None,
    )?;
    if labelled(&saved, "aside").is_ok()
        || saved.nodes()[&group].framing.is_none()
        || saved.duration()?.frames() != 30
    {
        return Err("unexpected group/ungroup result".into());
    }
    let expectations = [0u64, 9, 10, 15, 19, 20, 22, 25, 26, 29]
        .into_iter()
        .map(|frame| (frame, base(frame)))
        .collect();
    let mut fixture = project.finish(
        vec![
            "Group / ungroup",
            "`,g` named group",
            "`:group name=\"the uncomfortable answer\"`",
        ],
        expectations,
        Vec::new(),
    )?;
    fixture.signals = vec![click_peak(28_781, ATTENUATED_CLICK)];
    Ok(fixture)
}

/// Dot-repeat of a creative edit: `,h 6f` at Edit 10 through the headless
/// semantic apply, then the same instruction resubmitted at Edit 22 (`.`):
/// two silent freezes, of Original 21 and Original 27: 30 + 12 = 42 frames.
pub fn dot_repeat(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "dot-repeat")?;
    project.shorten()?;
    let pause = json!([{"type":"insert_pause","length":length(6)?}]);
    project.apply_semantic(pause.clone(), 10, None)?;
    project.apply_semantic(pause, 22, None)?;
    let expectations = vec![
        (9, original(21)),
        (10, original(21)),
        (15, original(21)),
        (16, original(22)),
        (21, original(27)),
        (22, original(27)),
        (27, original(27)),
        (28, original(28)),
        (41, original(41)),
    ];
    let mut fixture = project.finish(
        vec!["Dot-repeat of creative edits"],
        expectations,
        vec!["`.` resubmits the same apply instruction at the new cursor (PARITY.md)".into()],
    )?;
    // Pauses [16,016, 25,626) and [35,235, 44,845) are silent; the click
    // moves 12 frames later, to 48,000.
    fixture.audio = vec![(17_000, false), (36_500, false), (47_872, true)];
    Ok(fixture)
}

/// `,i` / `:insert` through `project insert-original`: the whole Original
/// (120 frames) reused after the 30-frame base: 150 frames.
pub fn reuse_original(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "reuse-original")?;
    let document = project.shorten()?;
    let index = root_children(&document)?.len().to_string();
    success(&[
        "project",
        "insert-original",
        project.path(),
        "--parent",
        document.root().as_str(),
        "--index",
        &index,
        "--expected",
        document.revision_id().as_str(),
    ])?;
    let mut expectations = vec![(0, base(0)), (29, base(29))];
    expectations.extend(
        [0u64, 1, 60, 90, 119]
            .into_iter()
            .map(|frame| (30 + frame, original(frame))),
    );
    let mut fixture = project.finish(vec!["`,i` reuse Original"], expectations, Vec::new())?;
    // Both Original clicks (48,000 and 191,992) sound again B(30) = 48,048
    // later, after the base's own.
    fixture.audio = vec![
        (28_672, true),
        (60_000, false),
        (95_939, true),
        (239_931, true),
    ];
    Ok(fixture)
}

/// `,s` / `:sound-place` through the headless `sound` request: the complete
/// catalog WAV placed at Edit frame 10 by the shared `sound_events::placement`.
pub fn sound_place(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "sound-place")?;
    project.shorten()?;
    let wav = project.directory.join("sound.wav");
    fs::copy(sound_media(), &wav)?;
    register_catalog_sound(&mut project, &wav, "Click sound")?;
    let document = project.document()?;
    let revision = project.next_revision()?;
    let request = project.directory.join("sound-place.json");
    fs::write(
        &request,
        serde_json::to_vec(&json!({
            "protocol": 1,
            "expected_revision": document.revision_id(),
            "new_revision": revision,
            "edit": {"type": "place", "asset": "sound", "at": {"frame": SOUND_FRAME}, "id": "click"}
        }))?,
    )?;
    success(&[
        "sound",
        project.path(),
        "--json",
        request.to_str().ok_or("UTF-8")?,
    ])?;
    let saved = project.document()?;
    let placed = &saved.sounds()[&SoundId::new("click")?];
    if saved.revision_id() != &revision || placed.offset != AudioSample(16_016) {
        return Err(format!("unexpected placement {placed:?}").into());
    }
    let expectations = [0u64, 9, 10, 15, 29]
        .into_iter()
        .map(|frame| (frame, base(frame)))
        .collect();
    let mut fixture = project.finish(vec!["`,s` place sound"], expectations, Vec::new())?;
    // The sound starts at B(10) = 16,016, exactly.
    fixture.audio = vec![(15_744, false), (16_128, true), (21_504, true)];
    Ok(fixture)
}

/// `,v` / `:trim edge=out delta=-3f mode=ripple` on the first half of the base
/// split at Edit 15: one `ApplySourceTrim` with Out -3 and the literal right
/// sibling. The left beat ends at Original 24, the right one follows at once:
/// 27 frames.
pub fn trim_out(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "trim-out")?;
    project.shorten()?;
    project.split_root(15)?;
    project.apply(|project, document, revision| {
        let parent = document.root().clone();
        let left = root_child_at(document, 0)?.0;
        let right = root_child_at(document, 15)?.0;
        let intent = SourceTrimIntent {
            in_frames: 0,
            out_frames: -3,
            slip_frames: 0,
            roll_frames: 0,
            policy: SourceTrimPolicy::Ripple,
        };
        let resolution = document.source_trim_edit(&parent, &left, Some(&right), intent)?;
        let wrappers = project.fresh(2)?;
        Ok(Command::ApplySourceTrim {
            parent,
            node: left,
            right: Some(right),
            intent,
            resources: SourceTrimResources {
                target_wrapper: resolution
                    .required_target_wrapper
                    .then(|| wrappers[0].clone()),
                right_wrapper: resolution
                    .required_right_wrapper
                    .then(|| wrappers[1].clone()),
                split: SplitIdentities {
                    nodes: project.fresh(resolution.required_split_nodes)?,
                },
                fillers: project.fresh(resolution.required_filler_nodes)?,
                timing: (resolution.capture != SourceTrimCapture::None).then(|| AudioTimingId {
                    allocation: revision.clone(),
                    ordinal: 0,
                }),
            },
        })
    })?;
    let expectations = vec![
        (0, base(0)),
        (11, original(23)),
        (12, original(27)),
        (20, original(35)),
        (26, original(41)),
    ];
    let mut fixture = project.finish(
        vec!["`,v` Trim", "`:trim edge=out delta=-3f mode=ripple`"],
        expectations,
        Vec::new(),
    )?;
    // The click (Original frame 29.97) is now at Edit 14.97: 28,780.8 - B(3).
    fixture.audio = vec![(23_867, true), (28_672, false)];
    Ok(fixture)
}

/// `:slip +5f` on the middle third of the base split at Edit 10 and 20: its
/// pictures and sound select Original 27..37 instead of 22..32 in the same
/// place, so the click moves 5 frames earlier: 30 frames.
pub fn slip(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "slip")?;
    project.shorten()?;
    project.split_root(10)?;
    project.split_root(20)?;
    project.apply(|_, document, _| {
        Ok(Command::SlipSource {
            parent: document.root().clone(),
            node: root_child_at(document, 10)?.0,
            delta_frames: 5,
        })
    })?;
    let expectations = vec![
        (9, base(9)),
        (10, original(27)),
        (15, original(32)),
        (19, original(36)),
        (20, base(20)),
        (29, base(29)),
    ];
    let mut fixture = project.finish(vec!["`:slip +5f`"], expectations, Vec::new())?;
    // The click at 28,780.8 - B(5) = 20,772.8; nothing sounds where it was.
    fixture.audio = vec![(20_664, true), (28_672, false)];
    Ok(fixture)
}

/// `:roll +2f` on the seam between Original [10, 29) and [60, 71) at Edit 19:
/// the left beat gains Original 29..31 (holding the click at Original frame
/// 29.97), the right one starts at Original 62. Pair duration stays 30.
pub fn roll(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "roll")?;
    project.delete_range(71, 120)?;
    project.delete_range(29, 60)?;
    let document = project.delete_range(0, 10)?;
    if root_child_at(&document, 19)?.1 != 19 || document.duration()?.frames() != 30 {
        return Err("unexpected Roll base".into());
    }
    let saved = project.apply(|project, document, revision| {
        let parent = document.root().clone();
        let left = root_child_at(document, 18)?.0;
        let right = root_child_at(document, 19)?.0;
        let resolution = document.source_roll(&parent, &left, &right, 2)?;
        if resolution.applied_delta_frames != 2 {
            return Err(format!("roll clamped to {}", resolution.applied_delta_frames).into());
        }
        let wrappers = project.fresh(2)?;
        Ok(Command::RollSources {
            parent,
            left,
            right,
            delta_frames: 2,
            left_wrapper: resolution.left.needs_wrapper.then(|| wrappers[0].clone()),
            right_wrapper: resolution.right.needs_wrapper.then(|| wrappers[1].clone()),
            timing: AudioTimingId {
                allocation: revision.clone(),
                ordinal: 0,
            },
        })
    })?;
    if saved.duration()?.frames() != 30 || root_child_at(&saved, 21)?.1 != 21 {
        return Err("the Roll did not move the seam to Edit 21".into());
    }
    let expectations = [0u64, 18, 19, 20, 21, 25, 29]
        .into_iter()
        .map(|frame| {
            (
                frame,
                original(if frame < 21 { 10 + frame } else { 41 + frame }),
            )
        })
        .collect();
    let mut fixture = project.finish(vec!["`:roll +2f`"], expectations, Vec::new())?;
    // The click at Edit 19.97 (48,000 - B(10) = 31,984); before the Roll the
    // right beat's silent Original 60.97 played there.
    fixture.audio = vec![(20_000, false), (31_875, true), (40_000, false)];
    Ok(fixture)
}

/// Per-play overrides: Edit [12, 24) as three total plays, then `:scope play
/// 2` edits (`EditScoped`) giving only the second play a -12 dB trim and a
/// static 1.35x zoom: 12 + 36 + 6 = 54 frames.
pub fn scoped_play(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "scoped-play")?;
    project.shorten()?;
    project.split_root(12)?;
    project.split_root(24)?;
    let document = project.apply(|_, document, _| {
        Ok(Command::WrapRepeat {
            node: root_child_at(document, 12)?.0,
            id: node("repeat")?,
            plays: 3,
            gap: None,
            anchor_policy: WrapAnchorPolicy::First,
        })
    })?;
    let NodeKind::Repeat {
        child, iterations, ..
    } = &document.nodes()[&node("repeat")?].kind
    else {
        return Err("repeat".into());
    };
    let (child, second) = (child.clone(), iterations.at(1).ok_or("second play")?);
    // The play's own branch: the shared definition until the first edit
    // isolates it, then that play's override root.
    let target = |document: &ProjectDocument| -> Result<ScopedNodeTarget> {
        let repeat = node("repeat")?;
        let owned = document
            .overrides()
            .get(&repeat)
            .and_then(|plays| plays.get(&second))
            .cloned();
        Ok(ScopedNodeTarget {
            node: owned.unwrap_or_else(|| child.clone()),
            repeats: vec![RepeatEditStep {
                repeat,
                branch: RepeatEditBranch::Play {
                    iteration: second.clone(),
                },
            }],
        })
    };
    let zoomed = pose((1, 2), (1, 2), (27, 20))?;
    for edit in [
        ScopedNodeEdit::SetAudioTreatments {
            treatments: AudioTreatments::from_clip_gain(ClipGain::new(
                GainDb::new(-12_000)?,
                false,
                Vec::new(),
                Vec::new(),
            )?),
        },
        ScopedNodeEdit::SetFraming {
            framing: Some(Framing::static_pose(zoomed)?),
        },
    ] {
        project.apply(|project, document, revision| {
            let target = target(document)?;
            let needs = document.scoped_edit_requirements(&target, &edit)?;
            Ok(Command::EditScoped {
                target,
                edit: edit.clone(),
                identities: deadpan_core::OccurrenceIdentities {
                    nodes: project.fresh(needs.nodes)?,
                    marks: (0..needs.marks)
                        .map(|n| deadpan_core::MarkId::new(format!("{revision}-mark-{n}")))
                        .collect::<std::result::Result<_, _>>()?,
                },
            })
        })?;
    }
    let mut expectations = vec![(0, base(0)), (11, base(11))];
    for play in 0..3u64 {
        let start = 12 + 12 * play;
        expectations.extend([
            (start, original(24)),
            (start + 6, original(30)),
            (start + 11, original(35)),
        ]);
    }
    expectations.extend([(48, original(36)), (53, original(41))]);
    let mut fixture = project.finish(
        vec!["Per-play overrides"],
        expectations,
        vec!["only play 2 is attenuated and zoomed".into()],
    )?;
    // Plays 1 and 3 keep the full click; play 2's, at B(24) + 9,562 = 48,000,
    // is 12 dB lower.
    fixture.audio = vec![(28_672, true), (67_072, true)];
    fixture.signals = vec![click_peak(48_000, ATTENUATED_CLICK)];
    Ok(fixture)
}

/// Semantic macros: Macro `p` = `,h 4f video=black`, Macro `m` = `@p` then
/// `10l`, saved through the headless `macro save` and run as `2@m` from Edit
/// 5 in one Compound. Each call inserts a black pause at the cursor, which
/// stays at the pause start, then moves ten frames: pauses at Edit 5 and 15
/// (base 11): 30 + 8 = 38 frames.
pub fn macro_call(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "macro-call")?;
    project.shorten()?;
    project.save_macro(
        'p',
        json!([{"type":"insert_pause","length":length(4)?,"black":true}]),
    )?;
    project.save_macro(
        'm',
        json!([
            {"type":"call","register":"p","count":1},
            {"type":"move_frames","forward":true,"count":10}
        ]),
    )?;
    project.run_macro('m', 5, None, 2)?;
    let expectations = vec![
        (4, base(4)),
        (5, Expected::Background),
        (8, Expected::Background),
        (9, base(5)),
        (14, base(10)),
        (15, Expected::Background),
        (18, Expected::Background),
        (19, base(11)),
        (37, base(29)),
    ];
    let mut fixture = project.finish(vec!["Semantic macros"], expectations, Vec::new())?;
    // Silent pauses [8,008, 14,414) and [24,024, 30,430); the click moves 8
    // frames later, to 28,780.8 + 12,812.8.
    fixture.audio = vec![(9_000, false), (26_000, false), (41_485, true)];
    Ok(fixture)
}

/// Freeze a detail: `,h 12f` at Edit 15 then Camera on the new Hold, a static
/// 2x pose centered at (0.30, 0.35), through the headless semantic apply:
/// a silent, magnified freeze of Original 26: 42 frames.
pub fn freeze_detail(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "freeze-detail")?;
    project.shorten()?;
    let detail = pose((3, 10), (7, 20), (2, 1))?;
    let saved = project.apply_semantic(
        json!([
            {"type":"insert_pause","length":length(12)?},
            {"type":"set_framing","framing":static_framing(detail)?}
        ]),
        15,
        None,
    )?;
    let (hold, start) = root_child_at(&saved, 15)?;
    let hold = &saved.nodes()[&hold];
    if start != 15
        || !matches!(hold.kind, NodeKind::Hold { .. })
        || hold.framing != Some(Framing::static_pose(detail)?)
    {
        return Err("the Hold at Edit 15 does not carry the detail framing".into());
    }
    let expectations = vec![
        (14, original(26)),
        (15, original(26)),
        (20, original(26)),
        (26, original(26)),
        (27, original(27)),
        (41, original(41)),
    ];
    let mut fixture = project.finish(
        vec!["Freeze a detail", "Frozen stare"],
        expectations,
        Vec::new(),
    )?;
    fixture.audio = vec![(30_000, false), (47_872, true)];
    Ok(fixture)
}

/// Save a modified group as a local recipe: `:gag one-more-time plays=2
/// gap=6f shorten=1f` on Edit [12, 24), a -12 dB trim on the inserted group,
/// `:recipe-save a` (`"ayag`), then at the end `:recipe a` (`"ap`): the copy
/// keeps the change. 12 + 30 + 6 + 30 = 78 frames.
pub fn local_recipe(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "local-recipe")?;
    project.shorten()?;
    project.split_root(12)?;
    let document = project.split_root(24)?;
    let beat = root_child_at(&document, 12)?.0;
    let recipe = GagRecipe::OneMoreTime {
        version: 1,
        plays: plays(2)?,
        gap: frames(6)?,
        shorten: frames(1)?,
        variation: None,
    };
    let saved = project.run_semantic(json!([{"type":"gag","recipe":recipe}]), 12, Some(&beat))?;
    let (group, _) = root_child_at(&saved, 12)?;
    project.apply_semantic(
        json!([{"type":"set_audio","change":{"type":"trim","gain":-12_000}}]),
        12,
        Some(&group),
    )?;
    let saved = project.apply_semantic(
        json!([
            {"type":"yank","selector":{"type":"text_object","object":{"type":"around_group"}},
                "register":"a"},
            {"type":"move_scope","end":true},
            {"type":"paste","register":"a","before":false}
        ]),
        12,
        Some(&group),
    )?;
    // The pasted slice arrives as its own copied-contents root holding the
    // group, which keeps its pinned recipe label.
    let (copy, start) = root_child_at(&saved, 48)?;
    let inner: Vec<String> = match &saved.nodes()[&copy].kind {
        NodeKind::Sequence { children } => children
            .iter()
            .map(|child| saved.nodes()[child].label.clone())
            .collect(),
        _ => Vec::new(),
    };
    if start != 48 || copy == group || inner != [recipe.label()] {
        return Err(format!("the local recipe was not inserted at Edit 48: {inner:?}").into());
    }
    let mut expectations = vec![(0, base(0)), (11, base(11))];
    for start in [12u64, 48] {
        expectations.extend([
            (start, original(24)),
            (start + 11, original(35)),
            (start + 12, original(35)),
            (start + 17, original(35)),
            (start + 18, original(24)),
            (start + 29, original(35)),
        ]);
    }
    expectations.extend([(42, original(36)), (47, original(41))]);
    let mut fixture = project.finish(
        vec!["Save modified group as local recipe", "Registers"],
        expectations,
        Vec::new(),
    )?;
    // Both plays of both groups hold the trimmed click 9,562 samples after
    // their starts; the freeze gaps are silent.
    fixture.audio = vec![(40_000, false), (100_000, false)];
    fixture.signals = [12, 30, 48, 66]
        .into_iter()
        .map(|start| click_peak(boundary(start) + 9_562, ATTENUATED_CLICK))
        .collect();
    Ok(fixture)
}

/// Detach from template: `:gag one-more-time plays=3 gap=12f shorten=6f` on
/// Edit [12, 24), then `:ungroup` of the inserted gag group through the
/// headless semantic apply: the same pictures and sound as ordinary beats with
/// no pinned label left: 72 frames.
pub fn detach_gag(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "detach-gag")?;
    project.shorten()?;
    project.split_root(12)?;
    let document = project.split_root(24)?;
    let beat = root_child_at(&document, 12)?.0;
    let recipe = GagRecipe::OneMoreTime {
        version: 1,
        plays: plays(3)?,
        gap: frames(12)?,
        shorten: frames(6)?,
        variation: None,
    };
    let saved = project.run_semantic(json!([{"type":"gag","recipe":recipe}]), 12, Some(&beat))?;
    let (group, _) = root_child_at(&saved, 12)?;
    let saved = project.apply_semantic(json!([{"type":"ungroup"}]), 12, Some(&group))?;
    let (child, _) = root_child_at(&saved, 12)?;
    if saved.nodes().contains_key(&group)
        || labelled(&saved, &recipe.label()).is_ok()
        || !matches!(saved.nodes()[&child].kind, NodeKind::Repeat { .. })
    {
        return Err("ungroup did not detach the gag into its Repeat".into());
    }
    let mut expectations = vec![(0, base(0)), (11, base(11))];
    for (start, gap) in [(12u64, Some(12u64)), (36, Some(6)), (54, None)] {
        expectations.extend([
            (start, original(24)),
            (start + 6, original(30)),
            (start + 11, original(35)),
        ]);
        if let Some(gap) = gap {
            expectations.extend([(start + 12, original(35)), (start + 11 + gap, original(35))]);
        }
    }
    expectations.extend([(66, original(36)), (71, original(41))]);
    let mut fixture = project.finish(
        vec!["Detach from template", "Group / ungroup"],
        expectations,
        Vec::new(),
    )?;
    fixture.audio = vec![
        (28_672, true),
        (45_056, false),
        (67_072, true),
        (80_128, false),
        (96_000, true),
    ];
    Ok(fixture)
}

/// The builders of this module, by fixture name.
/// A pause inside a slowed beat: the base's [12, 24) slowed to 24 frames, then
/// a 3-frame black pause at Edit 17, half way through Original frame 26.
/// FollowSpeed keeps the stage as the pause's owner with exact clocks;
/// Preserve splits the stage into two full processing contexts. Either way
/// pictures keep their source times: 12 + n shows Original 24 + n/2, the
/// pause is black over [17, 20), and later frames show the old ones three on.
fn slowed_pause(dir: &Path, name: &'static str, pitch: PitchPolicy) -> Result<Fixture> {
    let mut project = Project::create(dir, name)?;
    project.shorten()?;
    project.split_root(12)?;
    let document = project.split_root(24)?;
    let target = root_child_at(&document, 12)?.0;
    project.apply(|_, _, _| {
        Ok(Command::WrapRetime {
            node: target.clone(),
            id: NodeId::new("slow").expect("constant identity"),
            duration: FrameDuration::new(24).expect("positive duration"),
            pitch,
        })
    })?;
    let at = ProjectFrame(17);
    project.apply(|project, document, revision| {
        let required = match document.pause_insertion_target(at)? {
            PauseInsertionTarget::Ordinary(target) => {
                target.split.map_or(0, |split| split.required_ids)
            }
            PauseInsertionTarget::Retimed(target) => target.required_ids,
            PauseInsertionTarget::Occurrence(_) => {
                return Err("a root speed change is not an occurrence".into());
            }
        };
        Ok(Command::InsertTime {
            at,
            hold: silent(3, HoldVideo::Background)?,
            id: node("pause")?,
            identities: SplitIdentities {
                nodes: project.fresh(required)?,
            },
            timing: AudioTimingId {
                allocation: revision.clone(),
                ordinal: 0,
            },
        })
    })?;
    let mut expectations = vec![(0, base(0)), (11, base(11))];
    for n in [0u64, 1, 2, 3, 4] {
        expectations.push((12 + n, original(24 + n / 2)));
    }
    expectations.extend([(17, Expected::Background), (19, Expected::Background)]);
    for n in [5u64, 6, 7, 12, 23] {
        expectations.push((15 + n, original(24 + n / 2)));
    }
    expectations.extend([(39, original(36)), (44, original(41))]);
    project.finish(
        vec!["Hold (insert time)", "Slow delivery"],
        expectations,
        Vec::new(),
    )
}

pub fn tape_slowed_pause(dir: &Path) -> Result<Fixture> {
    slowed_pause(dir, "tape-slowed-pause", PitchPolicy::FollowSpeed)
}

pub fn preserve_slowed_pause(dir: &Path) -> Result<Fixture> {
    slowed_pause(dir, "preserve-slowed-pause", PitchPolicy::Preserve)
}

/// Two live tape stages compose to 3/4 speed over Original [24, 36).
/// Each expectation comes from the authored source window and the exact
/// center-sample rule, including every picture at both sides of the pause.
pub fn nested_tape_pause(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "nested-tape-pause")?;
    project.shorten()?;
    project.split_root(12)?;
    let document = project.split_root(24)?;
    let target = root_child_at(&document, 12)?.0;
    project.apply(|_, _, _| {
        Ok(Command::WrapRetime {
            node: target.clone(),
            id: node("inner-tape")?,
            duration: FrameDuration::new(24)?,
            pitch: PitchPolicy::FollowSpeed,
        })
    })?;
    project.apply(|_, _, _| {
        Ok(Command::WrapRetime {
            node: node("inner-tape")?,
            id: node("outer-tape")?,
            duration: FrameDuration::new(16)?,
            pitch: PitchPolicy::FollowSpeed,
        })
    })?;
    project.apply(|project, document, revision| {
        let at = ProjectFrame(17);
        let PauseInsertionTarget::Retimed(target) = document.pause_insertion_target(at)? else {
            return Err("nested tape pause must retain both live stages".into());
        };
        Ok(Command::InsertTime {
            at,
            hold: silent(3, HoldVideo::Background)?,
            id: node("nested-pause")?,
            identities: SplitIdentities {
                nodes: project.fresh(target.required_ids)?,
            },
            timing: AudioTimingId {
                allocation: revision.clone(),
                ordinal: 0,
            },
        })
    })?;
    let expectations = (0..37)
        .map(|frame| {
            let expected = if frame < 12 {
                base(frame)
            } else if (17..20).contains(&frame) {
                Expected::Background
            } else if frame < 31 {
                let local = frame - if frame < 17 { 12 } else { 15 };
                original(24 + (2 * local + 1) * 3 / 8)
            } else {
                original(36 + frame - 31)
            };
            (frame, expected)
        })
        .collect();
    project.finish(
        vec!["Hold (insert time)", "Slow delivery"],
        expectations,
        Vec::new(),
    )
}

/// `:repeat 4 role=audio extend=hold` over Edit [15, 20) of the 30-frame
/// base: the four plays need [15, 35), so a five-frame silent freeze of Edit
/// 29 follows the beat. The click at Edit sample 28,781 sounds again 5, 10
/// and 15 frames later as root sound events; the last lands inside the pause
/// (Edit 30..35) and is heard through its explicit allowance.
pub fn role_repeat_extend(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "role-repeat-extend")?;
    project.shorten()?;
    let saved = project.run_semantic(
        json!([
            {"type":"begin_selection"},
            {"type":"move_frames","forward":true,"count":5},
            {"type":"role_repeat","role":"audio","plays":4,"extend":true}
        ]),
        15,
        None,
    )?;
    if saved.duration()?.frames() != BASE_FRAMES as i64 + 5 || saved.sounds().len() != 3 {
        return Err(format!(
            "unexpected extended repeat: {} frames, {} root sounds",
            saved.duration()?.frames(),
            saved.sounds().len()
        )
        .into());
    }
    let expectations = [0u64, 15, 20, 29, 30, 32, 34]
        .into_iter()
        .map(|frame| (frame, base(frame.min(29))))
        .collect();
    let mut fixture = project.finish(
        vec!["Audio-only / video-only repeat (§6.5)"],
        expectations,
        Vec::new(),
    )?;
    fixture.audio = vec![
        (28_681, true),
        (36_689, true),
        (44_697, true),
        (52_705, true),
        (19_195, false),
    ];
    Ok(fixture)
}

pub const BUILDERS: [(&str, Builder); 25] = [
    ("scoped-audio-chain", scoped_audio::build),
    ("role-repeat-extend", role_repeat_extend),
    ("tape-slowed-pause", tape_slowed_pause),
    ("preserve-slowed-pause", preserve_slowed_pause),
    ("nested-tape-pause", nested_tape_pause),
    ("false-start", false_start),
    ("interrupted-answer", interrupted_answer),
    ("callback-register", callback_register),
    ("reaction-ping-pong", reaction_ping_pong),
    ("abrupt-return", abrupt_return),
    ("long-answer", long_answer),
    ("escalator", escalator),
    ("non-sequitur", non_sequitur),
    ("named-group", named_group),
    ("dot-repeat", dot_repeat),
    ("reuse-original", reuse_original),
    ("sound-place", sound_place),
    ("trim-out", trim_out),
    ("slip", slip),
    ("roll", roll),
    ("scoped-play", scoped_play),
    ("macro-call", macro_call),
    ("freeze-detail", freeze_detail),
    ("local-recipe", local_recipe),
    ("detach-gag", detach_gag),
];

/// Commit, after export, `framing` on the root child shown at Edit frame
/// `at` of an already built fixture, changing nothing else, and return the
/// new revision. An earlier movie verified against it must then differ
/// exactly on that child's frames, which shows the exported pictures carry
/// the fixture's own framing there.
pub fn reframe(fixture: &Fixture, at: i64, framing: Option<Framing>) -> Result<String> {
    let document = ProjectStore::open(&fixture.package, AccessMode::ReadOnly)?.snapshot()?;
    let asset = document
        .assets()
        .iter()
        .find(|(_, record)| record.video.is_some())
        .map(|(id, _)| id.clone())
        .ok_or("fixture has a picture asset")?;
    let mut project = Project {
        name: fixture.name,
        directory: fixture
            .package
            .parent()
            .ok_or("fixture package has a parent")?
            .to_owned(),
        package: fixture.package.clone(),
        step: 90,
        ids: 9000,
        asset,
    };
    let saved = project.apply(|_, document, _| {
        Ok(Command::SetFraming {
            node: root_child_at(document, at)?.0,
            framing,
        })
    })?;
    Ok(saved.revision_id().to_string())
}

/// The static centered 1.35x zoom the framing fixtures use.
pub fn zoomed() -> Result<Framing> {
    Ok(Framing::static_pose(pose((1, 2), (1, 2), (27, 20))?)?)
}
