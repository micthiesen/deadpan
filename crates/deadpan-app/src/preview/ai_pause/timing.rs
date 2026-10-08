//! Read-only timing facts from an admitted plan or an accepted sampling map.
//! Shortening accepted footage keeps its original sampling speed.

use deadpan_core::{
    ExactRatio, ExtensionDirection, FrameDuration, FrameRate, GeneratedSamplingMap, RevisionId,
    ScopedNodeTarget, TimeError,
};
use deadpan_jobs::{BridgeGenerationPlan, GenerationPlan};
use eframe::egui;

use crate::project::generation::Job;

pub(super) fn current_binding(
    session: u64,
    revision: &RevisionId,
    target: &ScopedNodeTarget,
    current_session: u64,
    current_revision: &RevisionId,
    current_target: Option<&ScopedNodeTarget>,
) -> bool {
    session == current_session && revision == current_revision && Some(target) == current_target
}

pub(super) fn current_job_plan<'a>(
    job: &'a Job,
    session: u64,
    revision: &RevisionId,
    target: &ScopedNodeTarget,
) -> Option<&'a BridgeGenerationPlan> {
    current_binding(
        job.session,
        &job.revision,
        &job.target,
        session,
        revision,
        Some(target),
    )
    .then_some(job.plan.as_ref())
    .flatten()
}

#[derive(Debug, PartialEq, Eq)]
enum Operation {
    Bridge,
    Extension {
        direction: ExtensionDirection,
        context_frames: FrameDuration,
        generated_frames: FrameDuration,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Report {
    operation: Operation,
    current_frames: FrameDuration,
    sampled_frames: FrameDuration,
    project_rate: FrameRate,
    native_frames: FrameDuration,
    native_rate: FrameRate,
    inserted: ExactRatio,
    requested_span: ExactRatio,
    native_span: ExactRatio,
    native_movie: ExactRatio,
    speed: ExactRatio,
    material: bool,
}

impl Report {
    pub(super) fn candidate(plan: &GenerationPlan) -> Result<Self, TimeError> {
        match plan {
            GenerationPlan::Bridge(plan) => Self::planned(plan),
            GenerationPlan::Extension(plan) => Self::accepted(
                &plan.sampling_map().clone().into(),
                plan.sampling_map().output_frame_count(),
            ),
        }
    }

    pub(super) fn planned(plan: &BridgeGenerationPlan) -> Result<Self, TimeError> {
        Self::new(
            plan.project_frames(),
            plan.project_frames(),
            plan.project_frame_rate(),
            FrameDuration::new(i64::from(plan.native_frame_count()))?,
            plan.native_frame_rate(),
            plan.requested_boundary_duration(),
            plan.actual_boundary_duration(),
        )
    }

    pub(super) fn accepted(
        sampling: &GeneratedSamplingMap,
        current_frames: FrameDuration,
    ) -> Result<Self, TimeError> {
        // The accepted map names the originally sampled span even when the
        // current Hold exposes only its prefix. No request or model is needed.
        let (operation, requested_intervals, native_intervals) = match sampling {
            GeneratedSamplingMap::Bridge(map) => (
                Operation::Bridge,
                ExactRatio::integer(map.output_frame_count().frames())
                    .checked_add(ExactRatio::ONE)?,
                map.native_frame_count().frames() - 1,
            ),
            GeneratedSamplingMap::Extension(map) => (
                Operation::Extension {
                    direction: map.direction(),
                    context_frames: map.context_frame_count(),
                    generated_frames: map.generated_frame_count(),
                },
                ExactRatio::integer(map.output_frame_count().frames()),
                map.generated_frame_count().frames(),
            ),
        };
        let mut report = Self::new(
            current_frames,
            sampling.output_frame_count(),
            sampling.project_rate(),
            sampling.native_frame_count(),
            sampling.native_rate(),
            seconds(requested_intervals, sampling.project_rate())?,
            seconds(
                ExactRatio::integer(native_intervals),
                sampling.native_rate(),
            )?,
        )?;
        report.operation = operation;
        Ok(report)
    }

    fn new(
        current_frames: FrameDuration,
        sampled_frames: FrameDuration,
        project_rate: FrameRate,
        native_frames: FrameDuration,
        native_rate: FrameRate,
        requested_span: ExactRatio,
        native_span: ExactRatio,
    ) -> Result<Self, TimeError> {
        let speed = native_span.checked_div(requested_span)?;
        // Five percent is a disclosure threshold, not a generation quality
        // limit. Compare exactly; even smaller conversions remain in details.
        let twentieths = speed.checked_mul(ExactRatio::integer(20))?;
        let material =
            !twentieths.compare_integer(19).is_gt() || !twentieths.compare_integer(21).is_lt();
        Ok(Self {
            operation: Operation::Bridge,
            current_frames,
            sampled_frames,
            project_rate,
            native_frames,
            native_rate,
            inserted: seconds(ExactRatio::integer(current_frames.frames()), project_rate)?,
            requested_span,
            native_span,
            native_movie: seconds(ExactRatio::integer(native_frames.frames()), native_rate)?,
            speed,
            material,
        })
    }

    fn heading(&self, label: &str) -> String {
        if self.material {
            format!(
                "{label} timing\n{:.2}× motion speed",
                approximate(self.speed)
            )
        } else {
            format!("{label} timing")
        }
    }

    fn details(&self) -> Vec<String> {
        let mut lines = vec![format!(
            "Inserted pause: {} frames at {} fps = {}.",
            self.current_frames.frames(),
            rate(self.project_rate),
            interval(self.inserted)
        )];
        if self.current_frames != self.sampled_frames {
            lines.push(format!(
                "Showing the first {} of {} sampled pictures. Accepted motion speed is unchanged.",
                self.current_frames.frames(),
                self.sampled_frames.frames()
            ));
        }
        let requested = if self.current_frames == self.sampled_frames {
            "Requested"
        } else {
            "Originally requested"
        };
        match self.operation {
            Operation::Bridge => lines.extend([
                format!(
                    "{requested} boundary span: {} ({} frame intervals).",
                    interval(self.requested_span),
                    i128::from(self.sampled_frames.frames()) + 1
                ),
                format!(
                    "Native boundary span: {} ({} frame intervals).",
                    interval(self.native_span),
                    self.native_frames.frames() - 1
                ),
            ]),
            Operation::Extension {
                direction,
                context_frames,
                generated_frames,
            } => lines.extend([
                format!(
                    "Extension from {} context: {} retained pictures.",
                    match direction {
                        ExtensionDirection::FromLeft => "left",
                        ExtensionDirection::FromRight => "right",
                    },
                    context_frames.frames()
                ),
                format!(
                    "{requested} generated span: {} ({} frames).",
                    interval(self.requested_span),
                    self.sampled_frames.frames()
                ),
                format!(
                    "Native generated span: {} ({} frames).",
                    interval(self.native_span),
                    generated_frames.frames()
                ),
            ]),
        }
        lines.extend([
            format!(
                "Native movie: {} frames at {} fps = {}.",
                self.native_frames.frames(),
                rate(self.native_rate),
                interval(self.native_movie)
            ),
            format!(
                "Motion speed: {}× ({:.6}× native speed).",
                ratio(self.speed),
                approximate(self.speed)
            ),
            match self.operation {
                Operation::Bridge => "Boundary pictures are excluded; the interior is sampled with linear interpolation. Timing is before any outer Repeat or Retime.",
                Operation::Extension { .. } => "Conditioning context is excluded; only generated pictures are sampled, using clamped frame centers and linear interpolation. Timing is before any outer Repeat or Retime.",
            }.into(),
        ]);
        lines
    }
}

fn seconds(frames: ExactRatio, rate: FrameRate) -> Result<ExactRatio, TimeError> {
    frames.checked_mul(ExactRatio::new(
        i128::from(rate.denominator()),
        i128::from(rate.numerator()),
    )?)
}

fn rate(value: FrameRate) -> String {
    if value.denominator() == 1 {
        value.numerator().to_string()
    } else {
        format!("{}/{}", value.numerator(), value.denominator())
    }
}

fn ratio(value: ExactRatio) -> String {
    if value.denominator() == 1 {
        value.numerator().to_string()
    } else {
        format!("{}/{}", value.numerator(), value.denominator())
    }
}

fn approximate(value: ExactRatio) -> f64 {
    value.numerator() as f64 / value.denominator() as f64
}

fn interval(value: ExactRatio) -> String {
    format!("{} s ({:.3} s)", ratio(value), approximate(value))
}

/// Native collapsing controls support focus/Enter as well as pointing. Exact
/// facts are visible text, never available only through a pointer tooltip.
pub(super) fn show(
    ui: &mut egui::Ui,
    label: &str,
    report: Result<Report, TimeError>,
) -> egui::Response {
    let report = match report {
        Ok(report) => report,
        Err(error) => return ui.label(format!("{label} timing unavailable: {error}")),
    };
    // CollapsingHeader uses Extend rather than wrapping. The material
    // conversion gets its own short line, including at compact widths.
    let heading = egui::RichText::new(report.heading(label));
    let heading = if report.material {
        heading.color(crate::preview::style::WARNING)
    } else {
        heading.weak()
    };
    let shown = egui::CollapsingHeader::new(heading)
        .id_salt("timing-details")
        .show(ui, |ui| {
            for line in report.details() {
                ui.label(egui::RichText::new(line).size(12.0));
            }
        });
    if shown.header_response.clicked() {
        // Keep the heading above its expanded facts. A control at the bottom
        // of the inspector must not open all of its details below the clip.
        shown.header_response.scroll_to_me(Some(egui::Align::Min));
    } else if shown.header_response.gained_focus() {
        shown.header_response.scroll_to_me(None);
    }
    shown.header_response
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_core::{BridgeInterpolation, ExtensionSamplingMap};
    use deadpan_jobs::{
        AxisLimits, BridgeCapability, DimensionLimits, FrameCountFormula, NativeDimensions,
    };

    fn plan(frames: i64, project_rate: FrameRate) -> BridgeGenerationPlan {
        let capability = BridgeCapability::new(
            true,
            FrameRate::new(24, 1).unwrap(),
            FrameCountFormula::new(8, 1, 9, 257).unwrap(),
            DimensionLimits::new(
                AxisLimits::new(256, 1024, 64).unwrap(),
                AxisLimits::new(192, 1024, 64).unwrap(),
            ),
        );
        BridgeGenerationPlan::new(
            FrameDuration::new(frames).unwrap(),
            project_rate,
            &capability,
            NativeDimensions::new(512, 320).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn twelve_frame_pause_reports_four_distinct_intervals_and_actual_speed() {
        let plan = plan(12, FrameRate::new(24, 1).unwrap());
        assert_eq!(plan.native_frame_count(), 17);
        let report = Report::planned(&plan).unwrap();
        assert_eq!(report.inserted, ExactRatio::new(1, 2).unwrap());
        assert_eq!(report.requested_span, ExactRatio::new(13, 24).unwrap());
        assert_eq!(report.native_span, ExactRatio::new(2, 3).unwrap());
        assert_eq!(report.native_movie, ExactRatio::new(17, 24).unwrap());
        assert_eq!(report.speed, ExactRatio::new(16, 13).unwrap());
        assert!(report.material);
        assert_eq!(
            report.heading("Variant 2"),
            "Variant 2 timing\n1.23× motion speed"
        );
        let details = report.details().join("\n");
        assert!(details.contains("Inserted pause: 12 frames at 24 fps = 1/2 s"));
        assert!(details.contains("Requested boundary span: 13/24 s"));
        assert!(details.contains("Native boundary span: 2/3 s"));
        assert!(details.contains("Native movie: 17 frames at 24 fps = 17/24 s"));
    }

    #[test]
    fn fractional_project_rate_is_preserved_and_slowing_is_reported() {
        let plan = plan(12, FrameRate::new(30_000, 1_001).unwrap());
        let report = Report::planned(&plan).unwrap();
        assert_eq!(plan.native_frame_count(), 9);
        assert_eq!(
            report.requested_span,
            ExactRatio::new(13_013, 30_000).unwrap()
        );
        assert_eq!(report.speed, ExactRatio::new(10_000, 13_013).unwrap());
        assert!(report.material);
        assert!(report.details()[0].contains("30000/1001 fps"));
        assert_eq!(
            report.heading("Generation"),
            "Generation timing\n0.77× motion speed"
        );
    }

    #[test]
    fn exact_native_speed_still_exposes_boundary_and_movie_durations() {
        let plan = plan(15, FrameRate::new(24, 1).unwrap());
        let report = Report::planned(&plan).unwrap();
        assert_eq!(report.speed, ExactRatio::ONE);
        assert!(!report.material);
        assert_eq!(report.heading("Accepted"), "Accepted timing");
        assert_ne!(report.native_movie, report.native_span);
        assert!(
            report
                .details()
                .iter()
                .any(|line| line.contains("Motion speed: 1×"))
        );
    }

    #[test]
    fn reopening_and_shortening_keep_the_accepted_sampling_speed() {
        let plan = plan(12, FrameRate::new(24, 1).unwrap());
        let sampling: GeneratedSamplingMap = plan.sampling_map().unwrap().into();
        let bytes = serde_json::to_vec(&sampling).unwrap();
        let reopened: GeneratedSamplingMap = serde_json::from_slice(&bytes).unwrap();
        let original = Report::planned(&plan).unwrap();
        assert_eq!(
            Report::accepted(&reopened, plan.project_frames()).unwrap(),
            original
        );
        let shortened = Report::accepted(&reopened, FrameDuration::new(8).unwrap()).unwrap();
        assert_eq!(shortened.speed, original.speed);
        assert_eq!(shortened.requested_span, original.requested_span);
        assert_eq!(shortened.inserted, ExactRatio::new(1, 3).unwrap());
        assert!(
            shortened
                .details()
                .iter()
                .any(|line| line.contains("first 8 of 12 sampled pictures"))
        );
        assert!(
            shortened
                .details()
                .iter()
                .any(|line| line.contains("Originally requested boundary span: 13/24 s"))
        );
    }

    #[test]
    fn extension_timing_uses_generated_duration_and_reports_excluded_context() {
        for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
            for frames in [1, 8] {
                let map = ExtensionSamplingMap::new(
                    direction,
                    FrameRate::new(30_000, 1_001).unwrap(),
                    FrameRate::new(24, 1).unwrap(),
                    FrameDuration::new(9).unwrap(),
                    FrameDuration::new(8).unwrap(),
                    FrameDuration::new(frames).unwrap(),
                    BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
                )
                .unwrap();
                let sampling: GeneratedSamplingMap = map.into();
                let reopened =
                    serde_json::from_slice(&serde_json::to_vec(&sampling).unwrap()).unwrap();
                let report =
                    Report::accepted(&reopened, FrameDuration::new(frames).unwrap()).unwrap();
                assert_eq!(
                    report.requested_span,
                    ExactRatio::new(i128::from(frames) * 1_001, 30_000).unwrap()
                );
                assert_eq!(report.native_span, ExactRatio::new(1, 3).unwrap());
                assert_eq!(report.native_movie, ExactRatio::new(17, 24).unwrap());
                assert_eq!(
                    report.speed,
                    ExactRatio::new(10_000, i128::from(frames) * 1_001).unwrap()
                );
                let details = report.details().join("\n");
                let side = if direction == ExtensionDirection::FromLeft {
                    "left"
                } else {
                    "right"
                };
                assert!(details.contains(&format!(
                    "Extension from {side} context: 9 retained pictures."
                )));
                assert!(details.contains("Native generated span: 1/3 s"));
                assert!(details.contains("Conditioning context is excluded"));
                assert!(!details.contains("boundary span"));
                assert!(!details.contains("Boundary pictures"));
                if frames > 1 {
                    let shortened =
                        Report::accepted(&reopened, FrameDuration::new(1).unwrap()).unwrap();
                    assert_eq!(shortened.speed, report.speed);
                    assert_eq!(shortened.requested_span, report.requested_span);
                    let details = shortened.details().join("\n");
                    assert!(details.contains("first 1 of 8 sampled pictures"));
                    assert!(details.contains("Originally requested generated span:"));
                }
            }
        }
    }

    #[test]
    fn disclosure_threshold_is_inclusive_and_does_not_hide_exact_facts() {
        let source = Report::planned(&plan(12, FrameRate::new(24, 1).unwrap())).unwrap();
        for (speed, material) in [
            (94, true),
            (95, true),
            (96, false),
            (104, false),
            (105, true),
            (106, true),
        ] {
            let report = Report::new(
                source.current_frames,
                source.sampled_frames,
                source.project_rate,
                source.native_frames,
                source.native_rate,
                ExactRatio::ONE,
                ExactRatio::new(speed, 100).unwrap(),
            )
            .unwrap();
            assert_eq!(report.material, material, "{speed}");
            assert!(report.details().iter().any(|line| {
                line.contains(&format!("Motion speed: {}×", ratio(report.speed)))
            }));
        }
    }

    #[test]
    fn timing_binding_refuses_other_sessions_revisions_targets_and_no_selection() {
        let revision = RevisionId::new("revision").unwrap();
        let later = RevisionId::new("later").unwrap();
        let target = ScopedNodeTarget {
            node: deadpan_core::NodeId::new("hold").unwrap(),
            repeats: Vec::new(),
        };
        let other = ScopedNodeTarget {
            node: deadpan_core::NodeId::new("other").unwrap(),
            repeats: Vec::new(),
        };
        let mut scoped = target.clone();
        scoped.repeats.push(deadpan_core::RepeatEditStep {
            repeat: deadpan_core::NodeId::new("repeat").unwrap(),
            branch: deadpan_core::RepeatEditBranch::Default,
        });
        assert!(!current_binding(
            7,
            &revision,
            &target,
            7,
            &revision,
            Some(&scoped)
        ));
        assert!(current_binding(
            7,
            &revision,
            &target,
            7,
            &revision,
            Some(&target)
        ));
        assert!(!current_binding(
            7,
            &revision,
            &target,
            8,
            &revision,
            Some(&target)
        ));
        assert!(!current_binding(
            7,
            &revision,
            &target,
            7,
            &later,
            Some(&target)
        ));
        assert!(!current_binding(
            7,
            &revision,
            &target,
            7,
            &revision,
            Some(&other)
        ));
        assert!(!current_binding(7, &revision, &target, 7, &revision, None));
    }
}
