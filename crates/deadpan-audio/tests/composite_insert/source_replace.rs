use super::*;

fn replace(
    document: &ProjectDocument,
    parent: &str,
    selected: Range<i64>,
    inserted: i64,
    name: &str,
) -> ProjectDocument {
    let NodeKind::Source { source } = source(ntsc(), inserted).kind else {
        unreachable!()
    };
    let range = FrameRange::new(ProjectFrame(selected.start), ProjectFrame(selected.end)).unwrap();
    let required = document
        .source_replacement(&id(parent), range)
        .unwrap()
        .required_ids;
    edit(
        document,
        name,
        Command::ReplaceSource {
            parent: id(parent),
            range,
            source,
            id: id(name),
            label: "Replacement".into(),
            identities: SplitIdentities {
                nodes: (0..required).map(|i| id(&format!("{name}-{i}"))).collect(),
            },
            timing: AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
        },
    )
}

#[test]
fn ntsc_replacement_retains_source_roomtone_and_outer_suffix_samples_in_one_shift() {
    for leaf in [
        source(ntsc(), 6),
        BeatNode::hold("Room tone", room(6, 700..921)),
    ] {
        let original = document(
            ntsc(),
            &["prefix", "group", "tail"],
            vec![
                ("prefix", source(ntsc(), 1)),
                ("group", BeatNode::sequence("Group", vec![id("lead")])),
                ("lead", leaf),
                ("tail", source(ntsc(), 2)),
            ],
        );
        for inserted in [1, 2, 4] {
            // Original global [2,4) lies inside the same group child.
            let after = replace(&original, "group", 2..4, inserted, "replace");
            let mut provider = Provider::new();
            let mut before_audio = renderer(&original, &mut provider);
            let mut after_audio = renderer(&after, &mut provider);
            for (old, new) in [(0, 0), (1, 1), (4, 2 + inserted), (7, 5 + inserted)] {
                let old = ntsc().audio_boundary(ProjectFrame(old)).unwrap().0;
                let new = ntsc().audio_boundary(ProjectFrame(new)).unwrap().0;
                let expected = read(&mut before_audio, &mut provider, old, 128);
                assert!(expected.iter().flatten().any(|sample| sample.abs() > 0.001));
                assert_eq!(read(&mut after_audio, &mut provider, new, 128), expected);
            }
            // The fresh replacement keeps the ordinary absolute sample phase.
            let expected = expected(&provider, ratio(-1, 5), 128);
            check_reads(&after, &mut provider, 3203, &expected);
        }
    }
}

#[test]
fn replacement_before_repeated_and_preserved_suffix_retains_their_entries() {
    let original = document(
        ntsc(),
        &["a", "repeated", "stretch"],
        vec![
            ("a", source(ntsc(), 3)),
            ("voice", source(ntsc(), 2)),
            ("repeated", repeat("voice", 2)),
            ("raw", source(ntsc(), 4)),
            ("stretch", preserve("raw", 4, 5)),
        ],
    );
    let after = replace(&original, "root", 1..3, 4, "replace");
    let mut provider = Provider::new();
    let mut before_audio = renderer(&original, &mut provider);
    let mut after_audio = renderer(&after, &mut provider);
    for (old, new) in [(3, 5), (5, 7), (7, 9)] {
        let old = ntsc().audio_boundary(ProjectFrame(old)).unwrap().0;
        let new = ntsc().audio_boundary(ProjectFrame(new)).unwrap().0;
        let expected = read(&mut before_audio, &mut provider, old, 128);
        assert!(expected.iter().flatten().any(|sample| sample.abs() > 0.001));
        assert_eq!(read(&mut after_audio, &mut provider, new, 128), expected);
    }
}
