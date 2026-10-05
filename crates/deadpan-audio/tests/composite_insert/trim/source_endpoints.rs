//! Real 44.1 kHz decode, with phase derived from handwritten 48 kHz boundaries.
use super::*;

#[test]
fn disjoint_source_endpoint_keeps_44100_phase_and_chunk_independent_kernel() {
    let mut voice = source(ntsc(), 6);
    let NodeKind::Source { source } = &mut voice.kind else {
        unreachable!()
    };
    let full = audio(0..44_117);
    source.audio_mapping = SourceAudioMapping::natural_rate(full.span, ntsc()).unwrap();
    source.audio = Some(full);
    source.audio_offset = AudioSample(7);
    let crop = |a, b| BeatNode {
        label: "Crop".into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: id("target"),
            duration: frames(b - a),
            mapping: FrameRange::new(ProjectFrame(a), ProjectFrame(b)).unwrap(),
            pitch: PitchPolicy::FollowSpeed,
            purpose: RetimePurpose::Partition,
        },
        cutaways: Vec::new(),
        captions: Vec::new(),
    };
    let before = document(
        ntsc(),
        &["lead", "crop"],
        vec![
            ("lead", silence(5)),
            ("crop", crop(0, 1)),
            ("target", voice),
        ],
    );
    let immutable = before.clone();
    let timing = AudioTimingId {
        allocation: revision("source-endpoint"),
        ordinal: 0,
    };
    let state = capture_unbound_audio_bindings(&before, timing).unwrap();
    let mut bindings = state.bindings().clone();
    let target = bindings.get_mut(&id("target")).unwrap();
    target
        .reanchors
        .push(AudioReanchorStep::for_source_endpoint(
            target.lattice.clone(),
            AudioSourceEndpoint::End,
        ));
    let state = AudioBindingState::new(
        state
            .timings()
            .iter()
            .map(|(id, layout)| AudioTimingRecord {
                id: id.clone(),
                layout: layout.clone(),
            })
            .collect(),
        bindings,
    )
    .unwrap();
    let mut wire = serde_json::to_value(&before).unwrap();
    wire["nodes"]["crop"] = serde_json::to_value(crop(2, 3)).unwrap();
    wire["audio_bindings"] = serde_json::to_value(state).unwrap();
    let after = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let mut provider = Provider::new();
    // B(6)-B(5)+B(5)-B(4)-7=3197 canonical samples;
    // 44.1/48=147/160. Skipping the endpoint is one mix sample early.
    let expected = oracle(&provider, 0..44_117, ratio(469959, 160), 256);
    assert_ne!(
        expected,
        oracle(&provider, 0..44_117, ratio(117453, 40), 256)
    );
    for chunk in [193, 239] {
        exact(
            &read_chunks(&after, &mut provider, 8008, 256, chunk, false),
            &expected,
            "endpoint 44.1 kHz reverse blocks",
        );
    }
    assert_eq!(before, immutable);
}
