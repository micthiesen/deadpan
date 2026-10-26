//! Request-local detected targets. Only selecting one submits an ordinary target save.
use super::*;
use crate::project::targets::{ProposalJob, ProposalOrigin, ProposalOutcome, ProposedTargets};

#[derive(Clone, Copy)]
pub(super) struct Proposal {
    pub(super) number: u8,
    pub(super) kind: DetectorKind,
    source_region: deadpan_analysis::NormalizedRect,
    pub(super) region: TargetRegion,
}

#[derive(Default)]
pub(super) struct Proposals {
    pub(super) pending: Option<u64>,
    pub(super) status: String,
    span: Option<SourceSpan>,
    pub(super) origin: Option<ProposalOrigin>,
}

pub(super) struct Acceptance {
    pub(super) id: TargetId,
    center: [ExactRatio; 2],
    number: u8,
}

fn page_numbers(entries: &mut [PickerTarget], page: usize) {
    for (index, entry) in entries.iter_mut().enumerate() {
        entry.number = (index / 9 == page).then_some((index % 9 + 1) as u8);
    }
}

fn matching_job(camera: &CameraSession, job: &ProposalJob) -> bool {
    camera.draft.phase() == CameraPhase::TargetPicker
        && camera.proposals.pending == Some(job.ticket)
        && job.selection == ProposalSelection::Both
        && camera.session == job.session
        && camera.revision == job.revision
        && camera.picture.as_ref() == Some(&(job.asset.clone(), job.pts))
}

/// A complete Both batch is appended atomically. Selection numbers belong to
/// each detector, while keyboard numbers belong to the currently visible page.
fn picker_proposals(
    found: ProposedTargets,
    geometry: &deadpan_render::PictureGeometry,
    layer: usize,
) -> Result<(Vec<PickerTarget>, ProposalOrigin), String> {
    let faces = found
        .faces
        .ok_or("The complete face collection is missing.")?;
    let regions = found
        .salient_regions
        .ok_or("The complete salient-region collection is missing.")?;
    deadpan_cli::picture_proposals::validate_faces(&faces)?;
    deadpan_cli::picture_proposals::validate_salient_regions(&regions)?;
    match &found.origin {
        ProposalOrigin::Worker {
            faces,
            salient_regions,
        } => {
            faces
                .as_ref()
                .ok_or("Face detector provenance is missing.")?
                .validate(DetectorKind::Faces)?;
            salient_regions
                .as_ref()
                .ok_or("Saliency detector provenance is missing.")?
                .validate(DetectorKind::SalientRegions)?;
        }
        #[cfg(any(test, feature = "ui-harness"))]
        ProposalOrigin::Scripted => {}
    }
    let mut entries = Vec::with_capacity(faces.len() + regions.len());
    let faces = faces
        .into_iter()
        .enumerate()
        .map(|(index, face)| (DetectorKind::Faces, index, face.region));
    let regions = regions
        .into_iter()
        .enumerate()
        .map(|(index, region)| (DetectorKind::SalientRegions, index, region.region));
    for (kind, index, source_region) in faces.chain(regions) {
        let number = u8::try_from(index + 1).map_err(|_| "Too many target proposals.")?;
        let region = deadpan_analysis::target_region(&source_region);
        let point = region
            .center
            .map(|value| f64::from(value) / f64::from(TARGET_UNITS));
        let center = geometry
            .source_to_input(layer, point)
            .map_err(|error| error.to_string())?
            .map(projected_point)
            .transpose()?;
        entries.push(PickerTarget {
            number: None,
            label: format!("{} {number} · proposal", proposal_kind_label(kind)),
            saved: None,
            point: None,
            center,
            proposal: Some(Proposal {
                number,
                kind,
                source_region,
                region,
            }),
        });
    }
    Ok((entries, found.origin))
}

fn origin_label(origin: &ProposalOrigin) -> &'static str {
    match origin {
        ProposalOrigin::Worker { .. } => "",
        #[cfg(any(test, feature = "ui-harness"))]
        ProposalOrigin::Scripted => "Scripted: ",
    }
}

impl CameraSession {
    pub(super) fn picker_page(&mut self, reverse: bool) {
        let pages = self.picker.len().div_ceil(9).max(1);
        self.picker_page = if reverse {
            (self.picker_page + pages - 1) % pages
        } else {
            (self.picker_page + 1) % pages
        };
        page_numbers(&mut self.picker, self.picker_page);
    }

    fn retain_pending_proposals(&mut self) -> bool {
        if self.proposals.pending.is_none() {
            return false;
        }
        self.proposals.status = "Targets are still being found for this captured picture. Saved targets and fixed points remain available.".into();
        true
    }

    fn reject_stale_proposal_result(&mut self, ticket: u64) {
        if self.proposals.pending == Some(ticket) {
            self.proposals.pending = None;
            self.proposals.status = "The captured picture changed before detection finished. No proposals were added. Open targets again to retry.".into();
        }
    }

    pub(super) fn close_proposals(&mut self) {
        self.proposals = Proposals::default();
        self.picker.retain(|entry| entry.proposal.is_none());
        self.picker_page = 0;
        page_numbers(&mut self.picker, 0);
    }

    fn proposal_framing(&self, acceptance: &Acceptance) -> Result<(), String> {
        let pose = FramingPose {
            center_x: acceptance.center[0],
            center_y: acceptance.center[1],
            ..self.draft.pose()
        };
        if let Some(follow) = &self.follow {
            let mut follow = follow.clone();
            follow.target = acceptance.id.clone();
            follow.center = acceptance.center;
            followed_framing(&follow, pose)?;
        } else {
            self.framing(pose, self.draft.reset_requested())?;
        }
        Ok(())
    }

    /// Called only after the exact target-save receipt and new picture exist.
    pub(super) fn accept_saved_proposal(&mut self, acceptance: Acceptance) -> Result<(), String> {
        self.proposal_framing(&acceptance)?;
        let pose = FramingPose {
            center_x: acceptance.center[0],
            center_y: acceptance.center[1],
            ..self.draft.pose()
        };
        let effect = self.draft.set_pose(pose);
        if let CameraEffect::Rejected(error) = effect {
            return Err(error.to_string());
        }
        if let Some(follow) = &mut self.follow {
            follow.target = acceptance.id.clone();
            follow.center = [self.draft.pose().center_x, self.draft.pose().center_y];
        }
        self.selected_target = Some(acceptance.id);
        self.fields.sync(self.draft.pose());
        Ok(())
    }

    fn proposal_reply(
        &mut self,
        ticket: u64,
        refusal: Option<&str>,
        saved: Option<&crate::project::targets::Saved>,
    ) {
        if let Some(pending) = self.proposals.pending
            && (ticket > pending || (ticket == pending && refusal.is_some()))
        {
            self.proposals.pending = None;
            let reason = refusal.unwrap_or("A newer target request superseded this detection. Close and reopen targets to retry.");
            self.proposals.status = format!(
                "Finding targets failed: {reason} Saved targets and fixed points remain available."
            );
        }
        let accepted_save = self.proposal_acceptance.as_ref().is_some_and(|acceptance| {
            saved.is_some_and(|saved| {
                saved.session == self.session
                    && saved.base == self.revision
                    && saved.id == acceptance.id
            })
        });
        if let Some(saving) = self.saving
            && ((ticket > saving && !accepted_save) || (ticket == saving && refusal.is_some()))
            && let Some(acceptance) = self.proposal_acceptance.take()
        {
            self.draft.target_unavailable(acceptance.number);
        }
    }
}

impl DeadpanApp {
    pub(super) fn start_camera_proposals(&mut self) {
        if let Some(camera) = &mut self.camera {
            if camera.retain_pending_proposals() {
                return;
            }
            camera.close_proposals();
        }
        let result = (|| -> Result<(u64, SourceSpan), String> {
            let camera = self.camera.as_ref().ok_or("Camera is closed.")?;
            if self.macros.recording() {
                return Err("Detected target proposals cannot be recorded. Save or cancel recording, then open targets again.".into());
            }
            if self.targets.proposal_zoom.is_some() {
                return Err("Targets are already being found for a Zoom command.".into());
            }
            if camera.display_aspect.is_none() {
                return Err("Target proposals require the exact Original picture.".into());
            }
            let (asset, pts) = camera
                .picture
                .clone()
                .ok_or("This picture has no Original moment.")?;
            let span = self.target_span(&asset, pts)?;
            let (session, revision) = (camera.session, camera.revision.clone());
            let ticket = self
                .submit_target(Operation::DetectPictureProposals {
                    selection: ProposalSelection::Both,
                    ticket: 0,
                    session,
                    revision,
                    asset,
                    pts,
                })
                .ok_or_else(|| {
                    self.error
                        .clone()
                        .unwrap_or_else(|| "Picture analysis could not start.".into())
                })?;
            Ok((ticket, span))
        })();
        if let Some(camera) = &mut self.camera {
            match result {
                Ok((ticket, span)) => {
                    camera.proposals.pending = Some(ticket);
                    camera.proposals.span = Some(span);
                    camera.proposals.status =
                        "Finding faces and salient regions in this exact Original picture…".into();
                }
                Err(reason) => {
                    camera.proposals.status = format!(
                        "Target proposals unavailable: {reason} Saved targets and fixed points remain available."
                    )
                }
            }
        }
    }

    pub(in crate::preview) fn finish_camera_proposals(&mut self) {
        let Some(camera) = self.camera.as_ref() else {
            return;
        };
        let Some(job) = self
            .targets
            .proposals()
            .filter(|job| matching_job(camera, job))
        else {
            return;
        };
        let Some(outcome) = job.outcome.clone() else {
            return;
        };
        // Camera's exact picture ticket, scope, cursor and revision must still be
        // installed. A different picture never lends proposals to this session.
        let valid = self.view == View::Sequence
            && self.sequence_scope == camera.navigation_scope
            && self.sequence_cursor == camera.cursor
            && self.inspected_target_matches(&camera.scope.node, camera.scoped.as_ref())
            && self.workspace.as_ref().is_some_and(|workspace| {
                workspace.session == camera.session
                    && workspace.document.revision_id() == &camera.revision
            })
            && self
                .presentation
                .picture()
                .is_some_and(|picture| picture.tier == crate::worker::PictureTier::Original)
            && self.presentation.stable_sequence_ticket(
                camera.session,
                &camera.revision,
                ProjectFrame(camera.cursor as i64),
            ) == Some(camera.ticket);
        if !valid {
            if let Some(camera) = &mut self.camera {
                // This exact completed result is no longer admissible. Release
                // only its pending ticket so a later `f` can capture a fresh
                // picture; never cancel or clear a newer request here.
                camera.reject_stale_proposal_result(job.ticket);
            }
            return;
        }
        let proposals = (|| -> Result<(Vec<PickerTarget>, ProposalOrigin), String> {
            let found = match outcome {
                ProposalOutcome::Found(found) => found,
                ProposalOutcome::Unavailable(reason) | ProposalOutcome::Failed(reason) => {
                    return Err(reason);
                }
                ProposalOutcome::Cancelled => return Err("Detection was cancelled.".into()),
            };
            let picture = self.presentation.picture().ok_or("The picture changed.")?;
            let frame = picture
                .frame
                .as_ref()
                .ok_or("The Original picture is unavailable.")?;
            let [width, height] = picture
                .canvas
                .map(|(w, h)| [w, h])
                .ok_or("Camera requires Your edit.")?;
            let geometry = deadpan_render::PictureGeometry::composed(
                frame.metadata(),
                picture.picture_context.as_deref(),
                [width, height],
                [width, height],
                FitMode::Fit,
                &render_layers(picture)?,
            )
            .map_err(|error| error.to_string())?;
            camera
                .proposals
                .span
                .ok_or("The captured proposal range changed.")?;
            let layer = camera
                .layer
                .checked_add(usize::from(picture.framing_gap))
                .ok_or("The selected input scope is invalid.")?;
            picker_proposals(found, &geometry, layer)
        })();
        let camera = self.camera.as_mut().expect("camera checked");
        camera.proposals.pending = None;
        match proposals {
            Ok((proposals, origin)) => {
                let faces = proposals
                    .iter()
                    .filter(|entry| {
                        entry
                            .proposal
                            .as_ref()
                            .is_some_and(|p| p.kind == DetectorKind::Faces)
                    })
                    .count();
                let regions = proposals.len() - faces;
                let source = origin_label(&origin);
                camera.proposals.status = if proposals.is_empty() {
                    format!(
                        "{source}No faces or salient regions found in this picture. Saved targets and fixed points remain available."
                    )
                } else {
                    format!(
                        "{source}{faces} face and {regions} region proposals. Selecting one saves that target; Enter applies framing."
                    )
                };
                camera.proposals.origin = Some(origin);
                camera.picker.extend(proposals);
                page_numbers(&mut camera.picker, camera.picker_page);
            }
            Err(reason) => {
                camera.proposals.status = format!(
                    "Finding targets failed: {reason} Saved targets and fixed points remain available."
                )
            }
        }
    }

    pub(super) fn save_camera_proposal(&mut self, number: u8) {
        let result = (|| -> Result<(u64, Acceptance), String> {
            if self.macros.recording() {
                return Err("A detected target cannot be accepted while recording. Save or cancel recording first.".into());
            }
            let camera = self.camera.as_ref().ok_or("Camera is closed.")?;
            let entry = camera
                .picker
                .iter()
                .find(|entry| entry.number == Some(number))
                .ok_or("The target page changed.")?;
            let Proposal {
                number: proposal_number,
                kind,
                source_region,
                ..
            } = entry
                .proposal
                .ok_or("This entry is not a detected proposal.")?;
            let center = entry
                .center
                .ok_or("That target is outside the selected beat's visible input.")?;
            let workspace = self.workspace.as_ref().ok_or("Open a project first.")?;
            let (id, label) = fresh_proposal_target(&workspace.document, kind, proposal_number)?;
            let span = camera
                .proposals
                .span
                .ok_or("The target proposal range changed.")?;
            let asset = camera
                .picture
                .as_ref()
                .ok_or("The Original picture changed.")?
                .0
                .clone();
            let target = deadpan_cli::picture_proposals::region_target(
                &source_region,
                label,
                asset,
                span.start().time_base,
                camera.display_aspect,
                span.start().ticks,
                span.end().ticks,
            )
            .map_err(|error| error.to_string())?;
            let acceptance = Acceptance {
                id: id.clone(),
                center,
                number,
            };
            camera.proposal_framing(&acceptance)?;
            let (session, revision) = (camera.session, camera.revision.clone());
            let ticket = self
                .submit_target(Operation::Save {
                    ticket: 0,
                    session,
                    revision,
                    id,
                    target: Box::new(target),
                })
                .ok_or_else(|| {
                    self.error
                        .clone()
                        .unwrap_or_else(|| "The detected target could not be saved.".into())
                })?;
            Ok((ticket, acceptance))
        })();
        match result {
            Ok((ticket, acceptance)) => {
                if let Some(camera) = &mut self.camera {
                    camera.saving = Some(ticket);
                    camera.proposal_acceptance = Some(acceptance);
                }
                self.message =
                    Some("Saving the selected detected target… Framing remains unsaved.".into());
            }
            Err(reason) => {
                if let Some(camera) = &mut self.camera {
                    camera.draft.target_unavailable(number);
                }
                self.error = Some(reason);
            }
        }
    }

    pub(super) fn camera_proposal_reply(&mut self, ticket: u64, refusal: Option<&str>) {
        let saved = self
            .targets
            .saved()
            .filter(|saved| {
                self.workspace
                    .as_ref()
                    .is_some_and(|workspace| workspace.document.revision_id() == &saved.revision)
            })
            .cloned();
        if let Some(camera) = &mut self.camera {
            camera.proposal_reply(ticket, refusal, saved.as_ref());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ratio(n: i128, d: i128) -> ExactRatio {
        ExactRatio::new(n, d).unwrap()
    }

    fn session() -> CameraSession {
        let pose = FramingPose::new(ratio(1, 3), ratio(2, 3), ratio(3, 2)).unwrap();
        CameraSession {
            session: 7,
            revision: RevisionId::new("r1").unwrap(),
            scope: InstancePath {
                node: NodeId::new("source").unwrap(),
                repeats: Vec::new(),
            },
            navigation_scope: SequenceScope::default(),
            scoped: None,
            ticket: Ticket {
                transport: None,
                source: 1,
                request: 2,
            },
            cursor: 3,
            entry: None,
            entry_pose: Some(pose),
            entry_layers: Vec::new(),
            draft: CameraDraft::new(pose).unwrap(),
            steps: CameraPanSteps {
                horizontal: [ratio(1, 100), ExactRatio::ZERO],
                vertical: [ExactRatio::ZERO, ratio(1, 100)],
            },
            picker: Vec::new(),
            picker_page: 0,
            proposals: Proposals::default(),
            proposal_acceptance: None,
            committing: false,
            fields: super::super::super::camera_fields::CameraFields::new(pose),
            field_keys: Vec::new(),
            field_error: None,
            local_position: ratio(7, 2),
            duration: deadpan_core::FrameDuration::new(12).unwrap(),
            layer: 0,
            picture: Some((AssetId::new("video").unwrap(), 1001)),
            display_aspect: Some(ratio(16, 9)),
            follow: None,
            selected_target: None,
            region_purpose: None,
            pointer: super::super::region_pointer_ui::State::default(),
            saving: None,
            rebase: None,
        }
    }

    fn entry(number: usize, proposed: bool) -> PickerTarget {
        let region = TargetRegion {
            center: [400_000, 500_000],
            size: [100_000, 200_000],
        };
        PickerTarget {
            number: None,
            label: format!("entry-{number}"),
            saved: (!proposed && number < 64).then(|| {
                (
                    TargetId::new(format!("target-{number}")).unwrap(),
                    region.into(),
                    TargetSource::Initial,
                )
            }),
            point: (!proposed && number >= 64).then_some([0.4, 0.5]),
            center: Some([ratio(2, 5), ratio(1, 2)]),
            proposal: proposed.then(|| Proposal {
                number: ((number - 69) % 64 + 1) as u8,
                region,
                kind: if number < 133 {
                    DetectorKind::Faces
                } else {
                    DetectorKind::SalientRegions
                },
                source_region: deadpan_analysis::NormalizedRect::new(0.35, 0.4, 0.1, 0.2).unwrap(),
            }),
        }
    }

    #[test]
    fn pages_reach_all_saved_fixed_and_detected_entries_without_async_renumbering() {
        let mut camera = session();
        camera.picker = (0..69).map(|n| entry(n, false)).collect();
        page_numbers(&mut camera.picker, 0);
        let before: Vec<_> = camera
            .picker
            .iter()
            .map(|e| {
                (
                    e.label.clone(),
                    e.number,
                    e.saved.as_ref().map(|(id, ..)| id.clone()),
                )
            })
            .collect();
        camera.picker.extend((69..197).map(|n| entry(n, true)));
        page_numbers(&mut camera.picker, 0);
        assert_eq!(
            camera.picker[..69]
                .iter()
                .map(|e| (
                    e.label.clone(),
                    e.number,
                    e.saved.as_ref().map(|(id, ..)| id.clone())
                ))
                .collect::<Vec<_>>(),
            before
        );
        let mut reached = std::collections::BTreeSet::new();
        for page in 0..22 {
            assert_eq!(camera.picker_page, page);
            for entry in camera.picker.iter().filter(|e| e.number.is_some()) {
                assert!((1..=9).contains(&entry.number.unwrap()));
                assert!(reached.insert(entry.label.clone()));
            }
            camera.picker_page(false);
        }
        assert_eq!(reached.len(), 197);
        for (index, entry) in camera.picker[69..].iter().enumerate() {
            let proposal = entry.proposal.unwrap();
            assert_eq!(
                proposal.kind,
                if index < 64 {
                    DetectorKind::Faces
                } else {
                    DetectorKind::SalientRegions
                }
            );
            assert_eq!(usize::from(proposal.number), index % 64 + 1);
        }
        assert_eq!(camera.picker_page, 0);
        camera.picker_page(true);
        assert_eq!(camera.picker_page, 21);
        camera.close_proposals();
        assert_eq!(camera.picker.len(), 69);
        assert_eq!(camera.picker_page, 0);
        assert!(camera.picker.iter().all(|e| e.proposal.is_none()));
    }

    #[test]
    fn face_reply_requires_live_picker_request_and_exact_session_revision_picture() {
        let mut camera = session();
        camera.draft.input(CameraKey::RefreshTargets, None);
        camera.proposals.pending = Some(20);
        let job = ProposalJob {
            ticket: 20,
            session: 7,
            revision: camera.revision.clone(),
            asset: AssetId::new("video").unwrap(),
            pts: 1001,
            selection: ProposalSelection::Both,
            outcome: None,
        };
        assert!(matching_job(&camera, &job));
        for wrong in [
            ProposalJob {
                selection: ProposalSelection::Faces,
                ..job.clone()
            },
            ProposalJob {
                selection: ProposalSelection::SalientRegions,
                ..job.clone()
            },
            ProposalJob {
                ticket: 19,
                ..job.clone()
            },
            ProposalJob {
                session: 8,
                ..job.clone()
            },
            ProposalJob {
                revision: RevisionId::new("r2").unwrap(),
                ..job.clone()
            },
            ProposalJob {
                asset: AssetId::new("other").unwrap(),
                ..job.clone()
            },
            ProposalJob {
                pts: 2002,
                ..job.clone()
            },
        ] {
            assert!(!matching_job(&camera, &wrong));
        }
        camera.draft.input(CameraKey::RefreshTargets, None);
        assert!(!matching_job(&camera, &job));
        camera.close_proposals();
        camera.draft.input(CameraKey::RefreshTargets, None);
        camera.proposals.pending = Some(21);
        assert!(!matching_job(&camera, &job));
        camera.proposal_reply(21, Some("busy"), None);
        assert!(camera.proposals.pending.is_none());
        assert!(camera.proposals.status.contains("busy"));
        camera.proposals.pending = Some(22);
        camera.proposal_reply(23, None, None);
        assert!(camera.proposals.pending.is_none());
        assert!(camera.proposals.status.contains("superseded"));
    }

    #[test]
    fn refused_face_save_leaves_pose_follow_and_picker_retryable() {
        let mut camera = session();
        camera.follow = Some(FollowDraft {
            target: TargetId::new("prior").unwrap(),
            center: [camera.draft.pose().center_x, camera.draft.pose().center_y],
            base_scale: camera.draft.pose().scale,
            base_live: ExactRatio::ONE,
            base_fallback: FramingPose::identity(),
            centering: FollowCentering::Unconstrained,
            clock: FramingClock::OwnerOutput,
            scale_transition: None,
        });
        let prior_follow = camera.follow.clone();
        camera.draft.input(CameraKey::RefreshTargets, None);
        camera.draft.input(CameraKey::Digit(4), None);
        let before = camera.draft.pose();
        camera.selected_target = Some(TargetId::new("prior").unwrap());
        camera.saving = Some(30);
        camera.proposal_acceptance = Some(Acceptance {
            id: TargetId::new("new").unwrap(),
            center: [ratio(3, 4), ratio(1, 4)],
            number: 4,
        });
        camera.proposal_reply(29, Some("older failure"), None);
        assert!(camera.proposal_acceptance.is_some());
        camera.proposal_reply(30, Some("target limit"), None);
        assert!(camera.proposal_acceptance.is_none());
        assert_eq!(camera.draft.pose(), before);
        assert_eq!(
            camera.selected_target,
            Some(TargetId::new("prior").unwrap())
        );
        assert_eq!(camera.draft.phase(), CameraPhase::TargetPicker);
        assert_eq!(
            camera.draft.input(CameraKey::Digit(4), None),
            CameraEffect::SelectTarget(4)
        );
        // A new selection may succeed after the refusal; no missing target was installed.
        assert_eq!(camera.follow, prior_follow);
    }

    #[test]
    fn a_later_refusal_preserves_the_successful_face_save_acceptance() {
        let mut camera = session();
        camera.saving = Some(30);
        let accepted = TargetId::new("face-1").unwrap();
        camera.proposal_acceptance = Some(Acceptance {
            id: accepted.clone(),
            center: [ratio(3, 4), ratio(1, 4)],
            number: 4,
        });
        let saved = crate::project::targets::Saved {
            session: camera.session,
            base: camera.revision.clone(),
            revision: RevisionId::new("r2").unwrap(),
            id: accepted.clone(),
        };
        // The service retains this successful receipt when a queued request
        // against the previous revision refuses before the UI consumes it.
        camera.proposal_reply(31, Some("revision changed"), Some(&saved));
        assert_eq!(camera.proposal_acceptance.as_ref().unwrap().id, accepted);
        camera.proposal_reply(32, None, Some(&saved));
        assert_eq!(camera.proposal_acceptance.as_ref().unwrap().id, accepted);
        let superseded = crate::project::targets::Saved {
            id: TargetId::new("other").unwrap(),
            ..saved.clone()
        };
        camera.proposal_reply(33, None, Some(&superseded));
        assert!(camera.proposal_acceptance.is_none());
        camera.proposal_acceptance = Some(Acceptance {
            id: accepted,
            center: [ratio(3, 4), ratio(1, 4)],
            number: 4,
        });
        camera.proposal_reply(30, Some("save refused"), Some(&saved));
        assert!(camera.proposal_acceptance.is_none());
    }

    #[test]
    fn accepted_face_changes_subject_and_center_but_preserves_follow_scale_and_clock() {
        let mut camera = session();
        let follow = FollowDraft {
            target: TargetId::new("prior").unwrap(),
            center: [camera.draft.pose().center_x, camera.draft.pose().center_y],
            base_scale: ratio(3, 2),
            base_live: ratio(1, 1),
            base_fallback: FramingPose::identity(),
            centering: FollowCentering::CanvasClamped,
            clock: FramingClock::RetainedOutput {
                offset: ratio(1, 3),
                duration: ratio(17, 2),
            },
            scale_transition: Some(deadpan_core::FollowScaleTransition {
                factor: ratio(2, 1),
                curve: deadpan_core::framing_zoom::ZoomCurve::Smoothstep,
                range: None,
            }),
        };
        camera.follow = Some(follow.clone());
        let scale = camera.draft.pose().scale;
        let acceptance = Acceptance {
            id: TargetId::new("face-1").unwrap(),
            center: [ratio(3, 4), ratio(1, 4)],
            number: 7,
        };
        camera.proposal_framing(&acceptance).unwrap();
        assert_eq!(camera.follow, Some(follow.clone()));
        camera.accept_saved_proposal(acceptance).unwrap();
        let after = camera.follow.as_ref().unwrap();
        assert_eq!(after.target, TargetId::new("face-1").unwrap());
        assert_eq!(camera.draft.pose().scale, scale);
        assert_eq!(after.base_scale, follow.base_scale);
        assert_eq!(after.base_live, follow.base_live);
        assert_eq!(after.base_fallback, follow.base_fallback);
        assert_eq!(after.clock, follow.clock);
        assert_eq!(after.scale_transition, follow.scale_transition);
        let previous = camera.draft.pose();
        assert!(
            camera
                .accept_saved_proposal(Acceptance {
                    id: TargetId::new("invalid").unwrap(),
                    center: [ratio(18, 1), ratio(1, 4)],
                    number: 1
                })
                .is_err()
        );
        assert_eq!(camera.draft.pose(), previous);
        assert_eq!(
            camera.follow.as_ref().unwrap().target,
            TargetId::new("face-1").unwrap()
        );
    }
    fn geometry(layers: &[FramingLayer]) -> deadpan_render::PictureGeometry {
        use deadpan_render::{
            FrameMetadata, Primaries, Rotation, SampleAspectRatio, SourceColor, Transfer,
        };
        let meta = FrameMetadata {
            width: 4,
            height: 4,
            row_stride_bytes: 16,
            clean_aperture: None,
            sample_aspect_ratio: SampleAspectRatio::SQUARE,
            rotation: Rotation::None,
            color: SourceColor {
                transfer: Transfer::Srgb,
                primaries: Primaries::Rec709,
            },
            pts: SourceTimestamp {
                ticks: 1001,
                time_base: deadpan_core::SourceTimeBase::new(1, 30000).unwrap(),
            },
        };
        deadpan_render::PictureGeometry::framed(&meta, [4, 4], [4, 4], FitMode::Fit, layers)
            .unwrap()
    }

    fn found(
        faces: Option<Vec<deadpan_cli::picture_proposals::DetectedFace>>,
        regions: Option<Vec<deadpan_cli::picture_proposals::DetectedSalientRegion>>,
    ) -> ProposedTargets {
        ProposedTargets {
            faces,
            salient_regions: regions,
            origin: ProposalOrigin::Scripted,
        }
    }

    #[test]
    fn mixed_proposals_keep_source_rectangles_and_project_only_the_selected_input() {
        use deadpan_analysis::NormalizedRect;
        use deadpan_cli::picture_proposals::{DetectedFace, DetectedSalientRegion};
        let source_face = NormalizedRect::new(0.7, 0.4, 0.1, 0.2).unwrap();
        let source_region = NormalizedRect::new(0.05, 0.4, 0.1, 0.2).unwrap();
        let layers = [
            FramingLayer::new([ratio(3, 4), ratio(1, 2)], ExactRatio::ONE).unwrap(),
            FramingLayer::identity(),
        ];
        let (entries, origin) = picker_proposals(
            found(
                Some(vec![DetectedFace {
                    region: source_face,
                    confidence: 0.8,
                }]),
                Some(vec![DetectedSalientRegion {
                    region: source_region,
                }]),
            ),
            &geometry(&layers),
            1,
        )
        .unwrap();
        assert_eq!(origin, ProposalOrigin::Scripted);
        assert_eq!(origin_label(&origin), "Scripted: ");
        assert_eq!(entries.len(), 2);
        let face = entries[0].proposal.unwrap();
        let region = entries[1].proposal.unwrap();
        assert_eq!((face.kind, face.number), (DetectorKind::Faces, 1));
        assert_eq!(
            (region.kind, region.number),
            (DetectorKind::SalientRegions, 1)
        );
        assert_eq!(face.source_region, source_face);
        assert_eq!(region.source_region, source_region);
        assert_eq!(face.region.center, [750000, 500000]);
        assert_eq!(face.region.size, [100000, 200000]);
        assert_eq!(region.region.center, [100000, 500000]);
        assert_eq!(entries[0].center, Some([ratio(1, 2), ratio(1, 2)]));
        assert!(entries[1].center.is_none());
        assert!(
            entries
                .iter()
                .all(|e| e.saved.is_none() && e.point.is_none() && e.number.is_none())
        );
    }

    #[test]
    fn both_is_complete_even_when_one_or_both_kinds_are_empty() {
        use deadpan_cli::picture_proposals::DetectedSalientRegion;
        let region = DetectedSalientRegion {
            region: deadpan_analysis::NormalizedRect::new(0.2, 0.3, 0.1, 0.2).unwrap(),
        };
        let geometry = geometry(&[]);
        let (entries, origin) =
            picker_proposals(found(Some(vec![]), Some(vec![region])), &geometry, 0).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].proposal.unwrap().kind,
            DetectorKind::SalientRegions
        );
        assert_eq!(entries[0].proposal.unwrap().number, 1);
        assert_eq!(origin, ProposalOrigin::Scripted);
        assert!(
            picker_proposals(found(Some(vec![]), Some(vec![])), &geometry, 0)
                .unwrap()
                .0
                .is_empty()
        );
        assert!(picker_proposals(found(None, Some(vec![])), &geometry, 0).is_err());
        assert!(picker_proposals(found(Some(vec![]), None), &geometry, 0).is_err());
        let repeated = found(Some(vec![]), Some(vec![region, region]));
        assert!(picker_proposals(repeated, &geometry, 0).is_err());
    }

    #[test]
    fn worker_origin_preserves_both_actual_runtime_records_and_rejects_wrong_revision() {
        use deadpan_cli::picture_proposals::DetectorRuntime;
        let faces = DetectorRuntime {
            detector: DetectorKind::Faces,
            engine: deadpan_jobs::picture_proposals::FACE_ENGINE.into(),
            request_revision: 3,
        };
        let regions = DetectorRuntime {
            detector: DetectorKind::SalientRegions,
            engine: deadpan_jobs::picture_proposals::SALIENCY_ENGINE.into(),
            request_revision: 2,
        };
        let origin = ProposalOrigin::Worker {
            faces: Some(faces.clone()),
            salient_regions: Some(regions.clone()),
        };
        let mut batch = found(Some(vec![]), Some(vec![]));
        batch.origin = origin.clone();
        let (entries, reported) = picker_proposals(batch, &geometry(&[]), 0).unwrap();
        assert!(entries.is_empty());
        assert_eq!(reported, origin);
        assert_eq!(origin_label(&reported), "");
        for wrong in [
            ProposalOrigin::Worker {
                faces: None,
                salient_regions: Some(regions.clone()),
            },
            ProposalOrigin::Worker {
                faces: Some(faces.clone()),
                salient_regions: None,
            },
            ProposalOrigin::Worker {
                faces: Some(faces),
                salient_regions: Some(DetectorRuntime {
                    request_revision: 1,
                    ..regions
                }),
            },
        ] {
            let mut batch = found(Some(vec![]), Some(vec![]));
            batch.origin = wrong;
            assert!(picker_proposals(batch, &geometry(&[]), 0).is_err());
        }
    }

    #[test]
    fn pending_refresh_keeps_exact_capture_origin_entries_and_page_until_completion() {
        let mut camera = session();
        camera.draft.input(CameraKey::RefreshTargets, None);
        camera.picker = (0..69).map(|n| entry(n, false)).collect();
        camera.picker_page = 2;
        page_numbers(&mut camera.picker, 2);
        camera.proposals.pending = Some(17);
        let clock = deadpan_core::SourceTimeBase::new(1, 30000).unwrap();
        let span = SourceSpan::new(
            SourceTimestamp {
                ticks: 1001,
                time_base: clock,
            },
            SourceTimestamp {
                ticks: 9009,
                time_base: clock,
            },
        )
        .unwrap();
        camera.proposals.span = Some(span);
        camera.proposals.origin = Some(ProposalOrigin::Scripted);
        let entry = camera.draft.pose();
        assert!(camera.retain_pending_proposals());
        assert_eq!(camera.proposals.pending, Some(17));
        assert_eq!(camera.proposals.span, Some(span));
        assert_eq!(camera.proposals.origin, Some(ProposalOrigin::Scripted));
        assert_eq!(camera.picker_page, 2);
        assert_eq!(camera.picker.len(), 69);
        assert_eq!(camera.draft.pose(), entry);
        assert!(camera.proposals.status.contains("still being found"));
        camera.proposals.pending = None;
        assert!(!camera.retain_pending_proposals());
        camera.close_proposals();
        assert!(camera.proposals.origin.is_none());
        assert!(camera.proposals.span.is_none());
    }

    #[test]
    fn stale_result_release_preserves_entries_and_unblocks_the_pending_gate() {
        let mut camera = session();
        camera.draft.input(CameraKey::RefreshTargets, None);
        camera.picker = vec![entry(0, false), entry(64, false)];
        page_numbers(&mut camera.picker, 0);
        camera.proposals.pending = Some(17);
        let stale = ProposalJob {
            ticket: 17,
            session: camera.session,
            revision: camera.revision.clone(),
            asset: AssetId::new("video").unwrap(),
            pts: 1001,
            selection: ProposalSelection::Both,
            outcome: Some(ProposalOutcome::Found(ProposedTargets {
                faces: Some(Vec::new()),
                salient_regions: Some(Vec::new()),
                origin: ProposalOrigin::Scripted,
            })),
        };
        assert!(matching_job(&camera, &stale));
        camera.reject_stale_proposal_result(16);
        assert_eq!(camera.proposals.pending, Some(stale.ticket));
        let saved_and_fixed = camera
            .picker
            .iter()
            .map(|entry| (entry.label.clone(), entry.saved.clone(), entry.point))
            .collect::<Vec<_>>();

        // Exercise the ticket-release transition used by finish_camera_proposals.
        // This local state-machine test does not construct Presentation; the
        // exact-tier/ticket dispatch is covered by the UI replay.
        camera.reject_stale_proposal_result(stale.ticket);
        assert_eq!(camera.proposals.pending, None);
        assert!(camera.proposals.status.contains("picture changed"));
        assert!(!camera.retain_pending_proposals());
        assert!(!matching_job(&camera, &stale));
        assert_eq!(
            camera
                .picker
                .iter()
                .map(|entry| (entry.label.clone(), entry.saved.clone(), entry.point))
                .collect::<Vec<_>>(),
            saved_and_fixed
        );

        // Once the caller has a valid captured picture again, the pending gate
        // is open for a fresh identity. The old result cannot match that ticket.
        camera.close_proposals();
        camera.proposals.pending = Some(18);
        let retry = ProposalJob {
            ticket: 18,
            outcome: None,
            ..stale.clone()
        };
        assert!(matching_job(&camera, &retry));
        assert!(!matching_job(&camera, &stale));
    }
}
