//! Independent fractional-clock move oracles using the decoded 44.1 kHz fixture.
//! Raw timing, live owner gain and the independent root sound bus are separate.
use super::*;

fn moved(
    before: &ProjectDocument,
    parent: &str,
    selected: Range<i64>,
    destination: MoveRangeDestination,
    name: &str,
) -> ProjectDocument {
    let range = FrameRange::new(ProjectFrame(selected.start), ProjectFrame(selected.end)).unwrap();
    let query = before.range_move(&id(parent), range, &destination).unwrap();
    let after = edit(
        before,
        name,
        Command::MoveRange {
            source_revision: before.revision_id().clone(),
            source_parent: id(parent),
            range,
            destination,
            identities: SplitIdentities {
                nodes: (0..query.required_ids)
                    .map(|n| id(&format!("{name}-{n}")))
                    .collect(),
            },
            timing: AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
        },
    );
    assert_eq!(after.duration().unwrap(), before.duration().unwrap());
    assert_eq!(after.sounds(), before.sounds());
    assert_eq!(after.sound_routes(), before.sound_routes());
    after
}

fn count(start: i64, end: i64) -> usize {
    usize::try_from(boundary(end) - boundary(start)).unwrap()
}

// (old start, new start, frame length, old physical owner start, owner duration).
// These fixture-specific intervals are specified by hand, never read from a
// binding, preflight map or compiled plan. Sample entries come from B(old start)
// and exact owner origins; retained discrete support is independent of geometry.

fn silent_picture(duration: i64) -> BeatNode {
    let mut node = source(ntsc(), duration);
    let NodeKind::Source { source } = &mut node.kind else {
        unreachable!()
    };
    // A picture-only Source supplies time without introducing a silent Hold's
    // root-sound gate. The shared synthetic asset already admits still images.
    source.video = SourceVideo::Still {
        asset: AssetId::new("media").unwrap(),
    };
    source.audio = None;
    source.audio_mapping = SourceAudioMapping::FitBeat;
    node
}

fn sounded(document: &ProjectDocument, start: i64, length: i64) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    wire["assets"]["media"]["source_qualification"] =
        serde_json::to_value(SourceQualificationId::new("b".repeat(64)).unwrap()).unwrap();
    let qualified = ProjectDocument::from_json(&wire.to_string()).unwrap();
    edit(
        &qualified,
        "placed-sound",
        Command::SetSound {
            id: SoundId::new("effect").unwrap(),
            event: SoundEvent {
                owner: id("root"),
                label: "Root clock".into(),
                source: audio(100..20_100),
                mapping: SourceAudioMapping::SelectedPlacement {
                    start: ExactRatio::integer(start),
                    frames: ratio(2_000_000, 147_147),
                    selection: ExactFrameRange::new(
                        ExactRatio::integer(start),
                        ExactRatio::integer(start + length),
                    )
                    .unwrap(),
                },
                offset: AudioSample(0),
                treatments: deadpan_core::AudioTreatments::default(),
                start_edge: AudioEdgePolicy::Hard,
                end_edge: AudioEdgePolicy::Hard,
                overflow: SoundOverflowPolicy::Reject,
            },
        },
    )
}

fn authored_all(document: &ProjectDocument, provider: &mut Provider) -> Vec<[f32; 2]> {
    let mut result = Vec::new();
    let mut reader = renderer(document, provider);
    let end = boundary(document.duration().unwrap().frames());
    for start in (0..end).step_by(256) {
        result.extend(
            reader
                .prepare_authored_bus(
                    provider,
                    AudioSample(start),
                    u32::try_from((end - start).min(256)).unwrap(),
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples,
        );
    }
    result
}

#[test]
fn root_sound_stays_at_frames_one_to_three_when_a_moves_to_the_end() {
    let before = sounded(
        &document(
            ntsc(),
            &["a", "b", "c"],
            vec![
                ("a", silent_picture(10)),
                ("b", silent_picture(10)),
                ("c", silent_picture(10)),
            ],
        ),
        1,
        2,
    );
    let after = moved(
        &before,
        "root",
        0..10,
        MoveRangeDestination::Seam {
            parent: id("root"),
            index: 3,
        },
        "sound-stays",
    );
    let mut provider = Provider::new();
    let old = authored_all(&before, &mut provider);
    let new = authored_all(&after, &mut provider);
    assert_eq!(new, old);
    let a = usize::try_from(boundary(1)).unwrap();
    let b = usize::try_from(boundary(3)).unwrap();
    assert!(new[..a].iter().all(|sample| *sample == [0.; 2]));
    assert!(new[a..b].iter().flatten().any(|value| value.abs() > 0.001));
    assert!(new[b..].iter().all(|sample| *sample == [0.; 2]));
    // The sound retains natural rate with an independently selected two-frame
    // interval. It must not follow A to frame21.
    let phase = ratio(100, 1)
        .checked_add(
            ratio(i128::from(boundary(1)) * 5 - 8008, 5)
                .checked_mul(ratio(147, 160))
                .unwrap(),
        )
        .unwrap();
    let reference = provider
        .prepared
        .prepare(
            ResampleRecipe::new(
                100..20_100,
                phase,
                AudioSample(0),
                ratio(147, 160),
                AudioSample(0)..AudioSample(i64::try_from(b - a).unwrap()),
            )
            .unwrap(),
            AudioSample(0),
            128,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap()
        .samples;
    assert_close(&new[a..a + 128], &reference);
}

#[test]
fn existing_root_sound_routes_and_sample_labels_are_unchanged_by_left_and_right_moves() {
    let original = sounded(
        &document(
            ntsc(),
            &["a", "b", "c"],
            vec![
                ("a", silent_picture(5)),
                ("b", silent_picture(5)),
                ("c", silent_picture(5)),
            ],
        ),
        0,
        12,
    );
    let before = cut(&original, 2..3, "routed-sound-cut");
    assert!(!before.sound_routes().is_empty());
    let envelope = GainEnvelope::new(
        GainClock::OwnerOutput,
        GainRange::new(ExactRatio::ZERO, ExactRatio::integer(14)).unwrap(),
        GainDb::new(-6000).unwrap(),
        vec![
            GainSegment::new(
                ExactRatio::integer(14),
                GainDb::new(6000).unwrap(),
                GainCurve::Linear,
            )
            .unwrap(),
        ],
    )
    .unwrap();
    let mut wire = serde_json::to_value(&before).unwrap();
    wire["nodes"]["root"]["audio_treatments"] =
        serde_json::to_value(gain(0, vec![envelope])).unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let NodeKind::Sequence { children } = &before.nodes()[before.root()].kind else {
        unreachable!()
    };
    let mut provider = Provider::new();
    let oracle = authored_all(&before, &mut provider);
    assert!(oracle.iter().flatten().any(|value| value.abs() > 0.001));
    for (selected, destination, name) in [
        (
            0..4,
            MoveRangeDestination::Seam {
                parent: id("root"),
                index: children.len(),
            },
            "routed-right",
        ),
        (
            9..14,
            MoveRangeDestination::Seam {
                parent: id("root"),
                index: 0,
            },
            "routed-left",
        ),
    ] {
        let after = moved(&before, "root", selected, destination, name);
        assert_eq!(authored_all(&after, &mut provider), oracle);
        for at in [boundary(10) + 17, boundary(1) + 19, boundary(6) + 23] {
            assert_eq!(
                authored(&after, &mut provider, at, 128),
                oracle[usize::try_from(at).unwrap()..usize::try_from(at).unwrap() + 128]
            );
        }
    }
}

#[test]
fn root_sound_recipe_stays_fixed_while_live_hold_gates_and_grants_follow_the_hold() {
    let base = sounded(
        &document(
            ntsc(),
            &["a", "pause", "b"],
            vec![
                ("a", silent_picture(5)),
                ("pause", silence(2)),
                ("b", silent_picture(5)),
            ],
        ),
        0,
        12,
    );
    let issuer = SoundHoldIssuer::Node {
        instance: InstancePath {
            node: id("pause"),
            repeats: vec![],
        },
    };
    let allowed = edit(
        &base,
        "allow-pause",
        Command::SetSoundAllowance {
            sound: SoundId::new("effect").unwrap(),
            issuer: issuer.clone(),
            allowed: true,
        },
    );
    let mut provider = Provider::new();
    for (before, name, permitted) in [(&base, "gate-move", false), (&allowed, "grant-move", true)] {
        let after = moved(
            before,
            "root",
            5..7,
            MoveRangeDestination::Seam {
                parent: id("root"),
                index: 0,
            },
            name,
        );
        assert_eq!(after.sound_allowances(), before.sound_allowances());
        let old = authored_all(before, &mut provider);
        let new = authored_all(&after, &mut provider);
        if permitted {
            assert_eq!(new, old);
            assert!(
                new[..count(0, 2)]
                    .iter()
                    .flatten()
                    .any(|value| value.abs() > 0.001)
            );
            assert!(after.sound_allowances()[&SoundId::new("effect").unwrap()].contains(&issuer));
        } else {
            assert!(new[..count(0, 2)].iter().all(|sample| *sample == [0.; 2]));
            assert!(
                old[usize::try_from(boundary(5)).unwrap()..usize::try_from(boundary(7)).unwrap()]
                    .iter()
                    .all(|sample| *sample == [0.; 2])
            );
            assert!(
                new[usize::try_from(boundary(5)).unwrap()..usize::try_from(boundary(7)).unwrap()]
                    .iter()
                    .flatten()
                    .any(|value| value.abs() > 0.001)
            );
            // Away from either old/new live gate, integral root labels are exact.
            for (start, end) in [(2, 5), (7, 12)] {
                let range = usize::try_from(boundary(start) + 192).unwrap()
                    ..usize::try_from(boundary(end) - 192).unwrap();
                assert_eq!(new[range.clone()], old[range]);
            }
        }
    }
}
