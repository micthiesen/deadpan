use super::*;
use crate::TransitionKind;

type Scene = fn(u32, u32) -> [u8; 3];

fn picture(color: impl Fn(u32, u32) -> [u8; 3]) -> PictureSignature {
    let rgba: Vec<u8> = (0..18)
        .flat_map(|y| (0..32).map(move |x| (x, y)))
        .flat_map(|(x, y)| {
            let [r, g, b] = color(x, y);
            [r, g, b, 255]
        })
        .collect();
    PictureSignature::from_rgba(&rgba, 32, 18, 32 * 4).unwrap()
}

fn warm(x: u32, _: u32) -> [u8; 3] {
    [200, 120 + ((x * 3 % 192) as u8) / 4, 40]
}

fn cool(_: u32, y: u32) -> [u8; 3] {
    [10, 20, 90 + (y * 4) as u8]
}

fn black(_: u32, _: u32) -> [u8; 3] {
    [0; 3]
}

fn blended(
    from: Scene,
    to: Scene,
    before: usize,
    count: usize,
    after: usize,
) -> Vec<PictureSignature> {
    (0..before)
        .map(|_| picture(from))
        .chain((1..=count).map(|step| {
            picture(|x, y| {
                let a = from(x, y);
                let b = to(x, y);
                std::array::from_fn(|channel| {
                    ((usize::from(a[channel]) * (count + 1 - step)
                        + usize::from(b[channel]) * step
                        + count.div_ceil(2))
                        / (count + 1)) as u8
                })
            })
        }))
        .chain((0..after).map(|_| picture(to)))
        .collect()
}

fn qualify(all: &[PictureSignature], requested: RangeInclusive<usize>) -> ContextShotQualification {
    let window = context_shot_window(requested.clone(), all.len()).unwrap();
    qualify_context(&all[window.clone()], window.start, all.len(), requested).unwrap()
}

#[test]
fn constant_context_reports_complete_real_coverage_and_policy() {
    let all = vec![picture(warm); 400];
    let result = qualify(&all, 160..=180);
    assert_eq!(result.rule, "deadpan-context-shots-1");
    assert_eq!(result.signature_version, SIGNATURE_VERSION);
    assert_eq!(result.requested, 160..=180);
    assert_eq!(result.measured, 35..306);
    assert_eq!(result.checked_pictures, 271);
    assert_eq!(result.coverage.padding_before, 125);
    assert_eq!(result.coverage.padding_after, 125);
    assert!(!result.coverage.starts_at_source_start);
    assert!(!result.coverage.ends_at_source_end);
    assert!(result.transition.is_none());
}

#[test]
fn real_source_edges_clip_padding_without_fabricated_signatures() {
    let all = vec![picture(warm); 9];
    let result = qualify(&all, 0..=8);
    assert_eq!(result.measured, 0..9);
    assert_eq!(result.checked_pictures, 9);
    assert_eq!(result.coverage.padding_before, 0);
    assert_eq!(result.coverage.padding_after, 0);
    assert!(result.coverage.starts_at_source_start);
    assert!(result.coverage.ends_at_source_end);
    assert!(result.transition.is_none());
    // Those same nine pictures are insufficient when the physical source has
    // additional unmeasured pictures. An arbitrary scan end is not an edge.
    assert!(matches!(
        qualify_context(&all, 0, 1000, 0..=8),
        Err(ShotError::Invalid(_))
    ));
}

#[test]
fn missing_left_or_right_padding_is_refused() {
    let all = vec![picture(warm); 400];
    let window = context_shot_window(160..=180, all.len()).unwrap();
    for incomplete in [window.start + 1..window.end, window.start..window.end - 1] {
        assert!(matches!(
            qualify_context(
                &all[incomplete.clone()],
                incomplete.start,
                all.len(),
                160..=180
            ),
            Err(ShotError::Invalid(_))
        ));
    }
}

#[test]
fn interval_and_total_allocation_bounds_are_checked_before_measurement() {
    assert_eq!(CONTEXT_SHOT_PADDING, 125);
    assert_eq!(context_shot_window(126..=387, 1000).unwrap(), 1..513);
    assert_eq!(context_shot_window(126..=388, 1000), Err(ShotError::Limit));
    assert!(context_shot_window(0..=0, 0).is_err());
    assert!(context_shot_window(0..=10, 10).is_err());
    let reversed = RangeInclusive::new(3, 2);
    assert!(context_shot_window(reversed, 10).is_err());
    assert!(qualify_context(&[], 0, 1, 0..=0).is_err());
    let oversized = vec![picture(warm); MAX_CONTEXT_SHOT_SIGNATURES + 1];
    assert_eq!(
        qualify_context(&oversized, 0, oversized.len(), 0..=0),
        Err(ShotError::Limit)
    );
    let single = [picture(warm)];
    assert!(qualify_context(&single, usize::MAX, usize::MAX, 0..=0).is_err());
    assert_eq!(
        context_shot_window(usize::MAX - 1..=usize::MAX - 1, usize::MAX).unwrap(),
        usize::MAX - 126..usize::MAX
    );
}

#[test]
fn a_one_picture_change_hidden_between_sparse_samples_is_rejected() {
    let mut all = vec![picture(warm); 32];
    all[11] = picture(cool);
    let sparse: Vec<_> = (0..=24)
        .step_by(3)
        .map(|index| all[index].clone())
        .collect();
    assert!(
        ShotAnalysis::from_signatures(&sparse)
            .unwrap()
            .boundaries()
            .is_empty()
    );
    assert!(
        ShotAnalysis::from_signatures(&all)
            .unwrap()
            .boundaries()
            .is_empty()
    );
    let result = qualify(&all, 0..=24);
    assert!(matches!(
        result.transition,
        Some(ContextShotTransition::Abrupt { picture: 11, .. })
    ));
}

#[test]
fn nearby_changes_and_initial_changes_are_not_navigation_suppressed() {
    let mut all = vec![picture(warm); 32];
    for signature in &mut all[2..5] {
        *signature = picture(cool);
    }
    assert!(matches!(
        qualify(&all, 0..=8).transition,
        Some(ContextShotTransition::Abrupt { picture: 2, .. })
    ));
    assert!(matches!(
        qualify(&all, 3..=8).transition,
        Some(ContextShotTransition::Abrupt { picture: 5, .. })
    ));
}

#[test]
fn equally_bright_different_pictures_still_have_an_abrupt_seam() {
    let before = picture(|x, _| if x < 16 { [255; 3] } else { [0; 3] });
    let after = picture(|x, _| if x < 16 { [0; 3] } else { [255; 3] });
    let change = context_seam_change(&before, &after).unwrap();
    assert!(change.cell >= MIN_CELL_CHANGE);
    assert_eq!(change.histogram, 0);
    assert!(context_seam_change(&before, &before).is_none());
}

#[test]
fn cut_outside_requested_interval_does_not_reject_a_post_cut_freeze() {
    let mut all = vec![picture(warm); 300];
    for signature in &mut all[150..] {
        *signature = picture(cool);
    }
    assert!(qualify(&all, 150..=150).transition.is_none());
    assert!(qualify(&all, 150..=160).transition.is_none());
    assert!(matches!(
        qualify(&all, 149..=150).transition,
        Some(ContextShotTransition::Abrupt { picture: 150, .. })
    ));
}

#[test]
fn full_dissolve_range_rejects_context_before_its_middle_boundary() {
    let all = blended(warm, cool, 160, 24, 160);
    let result = qualify(&all, 164..=165);
    let Some(ContextShotTransition::Gradual(found)) = result.transition else {
        panic!("expected gradual transition: {result:?}");
    };
    assert_eq!(found.kind, TransitionKind::Dissolve);
    assert!(
        found.range.start <= 165 && found.range.end > 164,
        "{found:?}"
    );
    assert!(found.boundary > 165, "context precedes midpoint: {found:?}");
    assert!(qualify(&all, 140..=145).transition.is_none());
    assert!(qualify(&all, 200..=205).transition.is_none());
}

#[test]
fn a_freeze_inside_a_fade_is_rejected_by_the_blended_range() {
    let cases: [(Scene, Scene, TransitionKind); 2] = [
        (warm, black, TransitionKind::FadeOut),
        (black, warm, TransitionKind::FadeIn),
    ];
    for (from, to, kind) in cases {
        let all = blended(from, to, 160, 24, 160);
        let result = qualify(&all, 172..=172);
        let Some(ContextShotTransition::Gradual(found)) = result.transition else {
            panic!("expected fade: {result:?}");
        };
        assert_eq!(found.kind, kind);
        assert!(found.range.contains(&172));
    }
}

#[test]
fn exact_padding_and_larger_windows_produce_the_same_absolute_result() {
    let all = blended(warm, cool, 200, 40, 200);
    for requested in [205..=205, 220..=223, 300..=301] {
        let bounded = qualify(&all, requested.clone());
        let whole = qualify_context(&all, 0, all.len(), requested).unwrap();
        assert_eq!(bounded.transition, whole.transition);
        assert_ne!(bounded.checked_pictures, whole.checked_pictures);
    }
}

#[test]
fn overlapping_gradual_candidates_are_not_globally_suppressed() {
    let all = blended(warm, cool, 100, 24, 100);
    let analysis = ShotAnalysis::from_signatures(&all).unwrap();
    assert_eq!(analysis.transitions().len(), 1);
    let raw = gradual::context_transitions(analysis.measures());
    assert!(raw.len() > 1, "expected independent candidates: {raw:?}");
    for candidate in raw {
        let requested = candidate.boundary..=candidate.boundary;
        assert!(
            qualify(&all, requested.clone()).transition.is_some(),
            "{requested:?}"
        );
    }
}
