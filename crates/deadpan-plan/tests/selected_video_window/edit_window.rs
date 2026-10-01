use super::*;

#[test]
fn editorial_window_metadata_preserves_vfr_selection_and_rounded_picture_tail() {
    for origin in [-10010, 13013] {
        let context = span(origin, origin + 12012);
        let original = document(
            context,
            &["source"],
            vec![(
                "source",
                source(context, natural_selection(EndpointPolicy::HoldAdjacent)),
            )],
        );
        let mut wire = serde_json::to_value(&original).unwrap();
        wire["nodes"]["source"]["kind"]["source"]["edit_window"] =
            serde_json::to_value(SourceEditWindow::new(ExactRatio::ZERO, ratio(3, 2)).unwrap())
                .unwrap();
        let declared = ProjectDocument::from_json(&wire.to_string()).unwrap();
        let before = RenderPlan::compile(&original).unwrap();
        let after = RenderPlan::compile(&declared).unwrap();
        assert_eq!(before.duration(), duration(2));
        assert_eq!(after.duration(), before.duration());
        let source_index = vfr_index(origin);
        for (frame, ticks, identity) in [(0, origin + 5005, 3), (1, origin + 7007, 4)] {
            let old = before.picture(ProjectFrame(frame)).unwrap();
            let new = after.picture(ProjectFrame(frame)).unwrap();
            assert_eq!(new.picture, old.picture);
            assert_eq!(new.framing, old.framing);
            assert_eq!(
                source_fields(&new.picture).0.ticks,
                ExactRatio::integer(ticks)
            );
            assert_eq!(
                new.picture
                    .select_source_frame(&source_index)
                    .unwrap()
                    .identity,
                SourceFrameId(identity)
            );
        }
    }
}
