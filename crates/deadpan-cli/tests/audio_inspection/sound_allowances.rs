use super::*;
use deadpan_core::{
    AudioEdgePolicy, AudioTimingId, InstancePath, SoundEvent, SoundHoldIssuer, SoundId,
    SoundOverflowPolicy, SourceAudioMapping,
};

fn issuer(name: &str) -> SoundHoldIssuer {
    SoundHoldIssuer::Node {
        instance: InstancePath {
            node: node(name),
            repeats: vec![],
        },
    }
}

fn allow(store: &mut ProjectStore, next: &str, hold: &str, allowed: bool) -> Result {
    commit(
        store,
        next,
        Command::SetSoundAllowance {
            sound: SoundId::new("effect-a")?,
            issuer: issuer(hold),
            allowed,
        },
    )
}

#[test]
fn decoded_sound_allowance_preserves_other_silence_and_ripple_sample_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    register(
        &mut store,
        &fixture("cfr-bframes.mp4"),
        1,
        OriginalOwnership::Managed,
        "import",
    )?;
    let before = store.snapshot()?;
    let rate = before.presentation_basis().frame_rate;
    let pause_frames = (before.duration()?.frames() + 1) / 2;
    let onset = rate.audio_boundary(ProjectFrame(before.duration()?.frames()))?;
    let expected = ProjectAudioSession::open(&path)?
        .read_time_mapped(AudioSample(137), 256, &active())?
        .samples;
    assert!(
        expected
            .iter()
            .any(|frame| frame.iter().any(|sample| sample.abs() > 0.001))
    );
    let source = SourceAudio {
        asset: asset(),
        span: before.assets()[&asset()].audio.unwrap(),
    };
    for (name, index) in [("hold-x", 1), ("hold-y", 2)] {
        commit(
            &mut store,
            name,
            Command::Insert {
                parent: node("root"),
                index,
                subtree: Subtree {
                    root: node(name),
                    nodes: BTreeMap::from([(
                        node(name),
                        BeatNode::hold(
                            name,
                            HoldRecipe {
                                duration: FrameDuration::new(pause_frames)?,
                                video: HoldVideo::Background,
                                audio: HoldAudio::Silence,
                                picture_context: None,
                            },
                        ),
                    )]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
        )?;
    }
    for (name, gain) in [("effect-a", 0), ("effect-b", 6000)] {
        commit(
            &mut store,
            name,
            Command::SetSound {
                id: SoundId::new(name)?,
                event: SoundEvent {
                    owner: node("root"),
                    label: name.into(),
                    source: source.clone(),
                    mapping: SourceAudioMapping::natural_rate(source.span, rate)?,
                    offset: onset,
                    gain_millidecibels: gain,
                    start_edge: AudioEdgePolicy::Hard,
                    end_edge: AudioEdgePolicy::Hard,
                    overflow: SoundOverflowPolicy::Reject,
                },
            },
        )?;
    }
    let window = AudioSample(onset.0 + 137);
    assert_eq!(
        ProjectAudioSession::open(&path)?
            .read_limited(window, 256, &active())?
            .samples,
        vec![[0.0; 2]; 256]
    );
    allow(&mut store, "allow-a-x", "hold-x", true)?;
    let allowed = store.snapshot()?;
    let mut session = ProjectAudioSession::open(&path)?;
    assert_eq!(
        session.read_time_mapped(window, 256, &active())?.samples,
        vec![[0.0; 2]; 256],
        "the Original remains silent"
    );
    assert_eq!(
        session.read_limited(window, 256, &active())?.samples,
        expected,
        "only A is allowed; neither Original nor the louder B may leak through"
    );
    let second_hold =
        AudioSample(onset.0 + rate.audio_boundary(ProjectFrame(pause_frames))?.0 + 137);
    assert!(
        deadpan_plan::RenderPlan::compile(&allowed)?
            .root_sound(&SoundId::new("effect-a")?)?
            .selects_sample(second_hold)?
    );
    assert_eq!(
        session.read_limited(second_hold, 256, &active())?.samples,
        vec![[0.0; 2]; 256],
        "Hold Y still suppresses A"
    );
    // Explicit revocation and durable navigation must reach the same current
    // policy without changing the source recipe or its phase.
    allow(&mut store, "revoke-a-x", "hold-x", false)?;
    assert_eq!(
        ProjectAudioSession::open(&path)?
            .read_limited(window, 256, &active())?
            .samples,
        vec![[0.0; 2]; 256]
    );
    store.undo(
        &store.snapshot()?.revision_id().clone(),
        revision("undo-revoke"),
    )?;
    assert_eq!(
        store.snapshot()?.sound_allowances(),
        allowed.sound_allowances()
    );
    assert_eq!(
        ProjectAudioSession::open(&path)?
            .read_limited(window, 256, &active())?
            .samples,
        expected
    );
    store.redo(
        &store.snapshot()?.revision_id().clone(),
        revision("redo-revoke"),
    )?;
    assert_eq!(
        ProjectAudioSession::open(&path)?
            .read_limited(window, 256, &active())?
            .samples,
        vec![[0.0; 2]; 256]
    );
    allow(&mut store, "restore-a-x", "hold-x", true)?;
    commit(
        &mut store,
        "prefix-time",
        Command::InsertTime {
            at: ProjectFrame(0),
            id: node("new-pause"),
            hold: HoldRecipe {
                duration: FrameDuration::new(1)?,
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
                picture_context: None,
            },
            identities: SplitIdentities::default(),
            timing: AudioTimingId {
                allocation: revision("prefix-time"),
                ordinal: 0,
            },
        },
    )?;
    let routed = store.snapshot()?;
    assert_eq!(routed.sound_allowances(), allowed.sound_allowances());
    assert_eq!(routed.sound_routes().len(), 2);
    allow(&mut store, "allow-empty-prefix", "new-pause", true)?;
    let shifted = AudioSample(window.0 + rate.audio_boundary(ProjectFrame(1))?.0);
    drop(store);
    let mut reopened = ProjectAudioSession::open(&path)?;
    assert_eq!(
        reopened.read_limited(shifted, 256, &active())?.samples,
        expected,
        "ripple retains the exact old source phase beneath the allowed Hold"
    );
    assert_eq!(
        reopened
            .read_limited(AudioSample(137), 256, &active())?
            .samples,
        vec![[0.0; 2]; 256],
        "an allowance cannot manufacture selected support in a route gap"
    );
    Ok(())
}
