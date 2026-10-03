//! Partial composite Repeat retains provider coordinates and live framing owners.
use super::*;

#[test]
fn partial_retime_repeat_keeps_source_and_owner_clocks_on_every_play() {
    let mut window = BeatNode::sequence("window", vec![]);
    window.kind = NodeKind::Retime {
        child: id("source"),
        duration: duration(8),
        mapping: range(2, 10),
        pitch: PitchPolicy::FollowSpeed,
        purpose: RetimePurpose::Partition,
    };
    let before = document(
        &["window"],
        vec![
            ("window", creep(window, 3)),
            ("source", creep(source(12, 0, 12012), 5)),
        ],
    );
    let selection = SliceCaptureSelection::Range { range: range(1, 4) };
    let query = before.repeat_selection(&id("root"), &selection, 3).unwrap();
    let tx = apply(
        &before,
        &CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision("repeat-selection"),
            command: Command::RepeatSelection {
                parent: id("root"),
                selection,
                plays: 3,
                identities: RepeatSelectionIdentities {
                    repeat: id("repeat"),
                    group: Some(id("body")),
                    split: SplitIdentities {
                        nodes: (0..query.required_split_ids)
                            .map(|n| id(&format!("split-{n}")))
                            .collect(),
                    },
                },
                timing: timing("repeat-selection"),
            },
        },
    )
    .unwrap();
    let after = tx.forward.apply(&before).unwrap();
    assert_eq!(tx.inverse.apply(&after).unwrap(), before);
    let plan = RenderPlan::compile(&after).unwrap();
    let old = RenderPlan::compile(&before).unwrap();
    // Exact old frame identity, first play, two added plays, then the old suffix.
    for (output, original) in [0, 1, 2, 3, 1, 2, 3, 1, 2, 3, 4, 5, 6, 7]
        .into_iter()
        .enumerate()
        .rev()
    {
        let sample = plan
            .picture(ProjectFrame(i64::try_from(output).unwrap()))
            .unwrap();
        let was = old.picture(ProjectFrame(original)).unwrap();
        assert_eq!(sample.picture, was.picture);
        assert_eq!(
            picture_ticks(&sample),
            ratio(i128::from(2 * original + 5) * 1001, 2)
        );
        for (label, local, frames, end_scale) in [
            ("source", ratio(i128::from(2 * original + 5), 2), 12, 5),
            ("window", ratio(i128::from(2 * original + 1), 2), 8, 3),
        ] {
            let layers: Vec<_> = sample
                .framing
                .iter()
                .filter(|layer| {
                    after.nodes()[&layer.instance.node].label == label && layer.pose.is_some()
                })
                .collect();
            assert_eq!(layers.len(), 1, "owner {label} applies once");
            assert_eq!(layers[0].local_position, local);
            assert_eq!(layers[0].duration, duration(frames));
            assert_eq!(
                layers[0].pose.unwrap().scale,
                linear_scale_at(local, frames, end_scale)
            );
            layers[0].instance.validate(&after).unwrap();
        }
        sample.instance.validate(&after).unwrap();
    }
}
