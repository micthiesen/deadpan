//! Zero-time groups have structure and an exact child slot, but no endpoints.
use super::*;

pub(super) const ENDPOINT_REASON: &str =
    "Empty groups contain no pictures or audio; In and Out cannot be adjusted.";
pub(super) const PLACEMENT_REASON: &str = "Empty groups insert at Sequence seams only. Use j/k to choose a slot; Move, Replace and frame interiors are unavailable.";
pub(super) fn is_forest(source: &Source) -> bool {
    matches!(source, Source::Edited { copied, .. }
        if matches!(copied.slice().selection(), deadpan_core::SliceCaptureSelection::Children { .. }))
}
pub(super) fn is_structural(source: &Source) -> bool {
    matches!(source, Source::Edited { copied, range }
        if range.duration().frames() == 0
            && *range == copied.slice().range()
            && matches!(copied.slice().selection(), deadpan_core::SliceCaptureSelection::Child { .. } | deadpan_core::SliceCaptureSelection::Children { .. }))
}

pub(super) fn initial_slot(
    seams: &[u64],
    children: &[NodeId],
    selected: Option<&NodeId>,
    at: u64,
) -> Option<usize> {
    selected
        .and_then(|selected| children.iter().position(|child| child == selected))
        .filter(|slot| seams.get(*slot) == Some(&at))
        .or_else(|| seams.iter().position(|seam| *seam == at))
}

pub(super) fn source_card(ui: &mut egui::Ui, draft: &Draft) {
    let Source::Edited { copied, range } = &draft.proposal.source else {
        return;
    };
    let forest = is_forest(&draft.proposal.source);
    if forest {
        ui.strong("Empty group contents");
    } else {
        let name = copied.child_label().unwrap_or("Untitled");
        ui.strong(format!("Empty group ‘{name}’"));
    }
    ui.label("0 frames · structure only");
    ui.label(format!(
        "{} · Edit boundary {}",
        copied.source_path().join(" / "),
        range.start().0
    ));
    ui.label("No included pictures or audio.");
    ui.weak(ENDPOINT_REASON);
    if let Some(view) = &draft.source_view {
        let document = view.media().admitted().document();
        // The source-only view has a neutral project root and imported wrapper.
        // Show the exact forest's roots, or the captured group's children.
        if let Some(wrapper) = document.children(document.root()).next()
            && let Some(group) = if forest {
                Some(wrapper)
            } else {
                document.children(wrapper).next()
            }
        {
            let mut nested = document.children(group);
            let names: Vec<_> = nested
                .by_ref()
                .take(4)
                .filter_map(|child| document.nodes().get(child).map(|node| node.label.as_str()))
                .collect();
            if !names.is_empty() {
                ui.label(format!(
                    "Inside: {}{}",
                    names.join(", "),
                    if nested.next().is_some() { ", …" } else { "" }
                ));
            }
        }
    }
    ui.weak(PLACEMENT_REASON);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captured_empty_child_keeps_its_slot_among_equal_time_boundaries() {
        let children: Vec<_> = ["lead", "left", "middle", "right", "tail"]
            .into_iter()
            .map(|name| NodeId::new(name).unwrap())
            .collect();
        let seams = [0, 2, 2, 2, 2, 8];
        for slot in 1..=3 {
            assert_eq!(
                initial_slot(&seams, &children, Some(&children[slot]), 2),
                Some(slot)
            );
            assert_eq!(
                placement_at(&seams, &children, Some(slot), 2),
                Ok(Destination::Slot(slot))
            );
        }
        assert_eq!(initial_slot(&seams, &children, None, 2), Some(1));
        assert_eq!(initial_slot(&seams, &children, Some(&children[2]), 4), None);
        assert_eq!(initial_slot(&[0], &[], None, 0), Some(0));
    }
}
