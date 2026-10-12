//! Real decoded attachment through nested Preserve, event EQ and owner EQ.
use super::*;
use deadpan_core::{BeatSound, EqualizerRecipe};

pub fn prepare(dir: &Path) -> Result<Value> {
    let mut project = Project::create(dir, "nested-equalized-sound")?;
    place_click_sound(&mut project)?;
    let sound_source = project.document()?.sounds()[&SoundId::new("click")?]
        .source
        .clone();
    project.apply(|_, _, _| {
        Ok(Command::DeleteSound {
            id: SoundId::new("click")?,
        })
    })?;
    let owner = root_children(&project.document()?)?[0].clone();
    project.apply(|_, document, _| {
        let NodeKind::Source { source } = &document.nodes()[&owner].kind else {
            unreachable!()
        };
        Ok(Command::SetSourceAudioMapping {
            node: owner.clone(),
            mapping: source.audio_mapping,
            offset: AudioSample(480_000),
        })
    })?;
    let inner = NodeId::new("inner")?;
    for (target, id, duration) in [
        (owner.clone(), inner.clone(), 180),
        (inner, NodeId::new("outer")?, 120),
    ] {
        project.apply(|_, _, _| {
            Ok(Command::WrapRetime {
                node: target,
                id,
                duration: FrameDuration::new(duration)?,
                pitch: PitchPolicy::Preserve,
            })
        })?;
    }
    let eq = EqualizerRecipe::new(
        [3000, -6000, 3000].map(|db| GainDb::new(db).unwrap()),
        None,
        None,
    )?;
    project.apply(|_, document, _| {
        let source = sound_source;
        Ok(Command::SetBeatSound {
            owner: owner.clone(),
            id: SoundId::new("attached")?,
            event: BeatSound {
                label: "Nested equalized attachment".into(),
                mapping: SourceAudioMapping::natural_rate(
                    source.span,
                    document.presentation_basis().frame_rate,
                )?,
                source,
                offset: AudioSample(0),
                treatments: AudioTreatments::default()
                    .with_clip_gain(ClipGain::new(GainDb::new(-12000)?, false, vec![], vec![])?)?
                    .with_equalizer(Some(eq))?,
                start_edge: AudioEdgePolicy::Hard,
                end_edge: AudioEdgePolicy::Hard,
                overflow: SoundOverflowPolicy::Reject,
            },
        })
    })?;
    project.apply(|_, _, _| {
        Ok(Command::SetAudioTreatments {
            node: owner,
            treatments: AudioTreatments::default().with_equalizer(Some(eq))?,
        })
    })?;
    let read = |start: i64, end: i64| -> Result<Vec<[f32; 2]>> {
        Ok(serde_json::from_value(
            success(&[
                "inspect-audio",
                project.path(),
                "--samples",
                &start.to_string(),
                &end.to_string(),
                "--authored-bus",
            ])?["audio"]["samples"]
                .clone(),
        )?)
    };
    let full = read(512, 768)?;
    assert_eq!(full.len(), 256);
    assert!(full.iter().flatten().any(|value| value.abs() > 0.001));
    let mut maximum = 0.0_f32;
    for start in [512, 640, 576] {
        let partial = read(start, start + 64)?;
        assert_eq!(partial.len(), 64);
        for (actual, expected) in partial.iter().flatten().zip(
            full[(start - 512) as usize..(start - 512) as usize + 64]
                .iter()
                .flatten(),
        ) {
            assert!(actual.is_finite());
            maximum = maximum.max((actual - expected).abs());
        }
    }
    assert!(maximum < 1e-6, "cold read difference {maximum}");
    Ok(
        json!({"package":project.package, "revision":project.document()?.revision_id(), "samples":256, "start":512, "cold_windows":3, "max_difference":maximum}),
    )
}
