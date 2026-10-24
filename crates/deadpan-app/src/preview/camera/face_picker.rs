//! Request-local face proposals. Only selecting one submits an ordinary target save.
use super::*;
use crate::project::targets::{FaceJob, FaceOutcome};

#[derive(Clone, Copy)]
pub(super) struct Proposal {
    pub(super) number: u8,
    face: deadpan_cli::faces::DetectedFace,
    pub(super) region: TargetRegion,
}

#[derive(Default)]
pub(super) struct Faces {
    pub(super) pending: Option<u64>,
    pub(super) status: String,
    span: Option<SourceSpan>,
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

fn matching_job(camera: &CameraSession, job: &FaceJob) -> bool {
    camera.draft.phase() == CameraPhase::TargetPicker
        && camera.faces.pending == Some(job.ticket)
        && camera.session == job.session
        && camera.revision == job.revision
        && camera.picture.as_ref() == Some(&(job.asset.clone(), job.pts))
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

    pub(super) fn close_faces(&mut self) {
        self.faces = Faces::default();
        self.picker.retain(|entry| entry.face.is_none());
        self.picker_page = 0;
        page_numbers(&mut self.picker, 0);
    }

    fn face_framing(&self, acceptance: &Acceptance) -> Result<(), String> {
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
    pub(super) fn accept_saved_face(&mut self, acceptance: Acceptance) -> Result<(), String> {
        self.face_framing(&acceptance)?;
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

    fn face_reply(
        &mut self,
        ticket: u64,
        refusal: Option<&str>,
        saved: Option<&crate::project::targets::Saved>,
    ) {
        if let Some(pending) = self.faces.pending
            && (ticket > pending || (ticket == pending && refusal.is_some()))
        {
            self.faces.pending = None;
            let reason = refusal.unwrap_or("A newer target request superseded this detection. Close and reopen targets to retry.");
            self.faces.status = format!(
                "Finding faces failed: {reason} Saved targets and fixed points remain available."
            );
        }
        let accepted_save = self.face_acceptance.as_ref().is_some_and(|acceptance| {
            saved.is_some_and(|saved| {
                saved.session == self.session
                    && saved.base == self.revision
                    && saved.id == acceptance.id
            })
        });
        if let Some(saving) = self.saving
            && ((ticket > saving && !accepted_save) || (ticket == saving && refusal.is_some()))
            && let Some(acceptance) = self.face_acceptance.take()
        {
            self.draft.target_unavailable(acceptance.number);
        }
    }
}

impl DeadpanApp {
    pub(super) fn start_camera_faces(&mut self) {
        if let Some(camera) = &mut self.camera {
            camera.close_faces();
        }
        let result = (|| -> Result<(u64, SourceSpan), String> {
            let camera = self.camera.as_ref().ok_or("Camera is closed.")?;
            if self.macros.recording() {
                return Err("Face proposals cannot be recorded. Save or cancel recording, then open targets again.".into());
            }
            if self.targets.face_zoom.is_some() {
                return Err("Faces are already being found for a Zoom command.".into());
            }
            if camera.display_aspect.is_none() {
                return Err("Face proposals require the exact Original picture.".into());
            }
            let (asset, pts) = camera
                .picture
                .clone()
                .ok_or("This picture has no Original moment.")?;
            let span = self.target_span(&asset, pts)?;
            let (session, revision) = (camera.session, camera.revision.clone());
            let ticket = self
                .submit_target(Operation::DetectFaces {
                    ticket: 0,
                    session,
                    revision,
                    asset,
                    pts,
                })
                .ok_or_else(|| {
                    self.error
                        .clone()
                        .unwrap_or_else(|| "Face detection could not start.".into())
                })?;
            Ok((ticket, span))
        })();
        if let Some(camera) = &mut self.camera {
            match result {
                Ok((ticket, span)) => {
                    camera.faces.pending = Some(ticket);
                    camera.faces.span = Some(span);
                    camera.faces.status = "Finding faces in this exact Original picture…".into();
                }
                Err(reason) => {
                    camera.faces.status = format!(
                        "Faces unavailable: {reason} Saved targets and fixed points remain available."
                    )
                }
            }
        }
    }

    pub(in crate::preview) fn finish_camera_faces(&mut self) {
        let Some(camera) = self.camera.as_ref() else {
            return;
        };
        let Some(job) = self.targets.faces().filter(|job| matching_job(camera, job)) else {
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
            return;
        }
        let proposals = (|| -> Result<Vec<PickerTarget>, String> {
            let faces = match outcome {
                FaceOutcome::Found(faces) => faces,
                FaceOutcome::Unavailable(reason) | FaceOutcome::Failed(reason) => {
                    return Err(reason);
                }
                FaceOutcome::Cancelled => return Err("Detection was cancelled.".into()),
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
            let span = camera
                .faces
                .span
                .ok_or("The face proposal range changed.")?;
            let asset = camera
                .picture
                .as_ref()
                .ok_or("The Original picture changed.")?
                .0
                .clone();
            faces
                .into_iter()
                .enumerate()
                .map(|(index, face)| {
                    let number = u8::try_from(index + 1).map_err(|_| "Too many face proposals.")?;
                    let target = deadpan_cli::faces::face_target(
                        &face,
                        format!("Face {number}"),
                        asset.clone(),
                        span.start().time_base,
                        camera.display_aspect,
                        span.start().ticks,
                        span.end().ticks,
                    )
                    .map_err(|error| error.to_string())?;
                    let point = target
                        .region
                        .center
                        .map(|v| f64::from(v) / f64::from(TARGET_UNITS));
                    let center = geometry
                        .source_to_input(camera.layer + usize::from(picture.framing_gap), point)
                        .map_err(|error| error.to_string())?
                        .map(projected_point)
                        .transpose()?;
                    Ok(PickerTarget {
                        number: None,
                        label: format!("Face {number} · proposal"),
                        saved: None,
                        point: None,
                        center,
                        face: Some(Proposal {
                            number,
                            face,
                            region: target.region,
                        }),
                    })
                })
                .collect()
        })();
        let camera = self.camera.as_mut().expect("camera checked");
        camera.faces.pending = None;
        match proposals {
            Ok(proposals) => {
                camera.faces.status = if proposals.is_empty() {
                    "No faces found in this picture. Saved targets and fixed points remain available.".into()
                } else {
                    format!(
                        "{} face proposals. Selecting one saves that target; Enter applies framing.",
                        proposals.len()
                    )
                };
                camera.picker.extend(proposals);
                page_numbers(&mut camera.picker, camera.picker_page);
            }
            Err(reason) => {
                camera.faces.status = format!(
                    "Finding faces failed: {reason} Saved targets and fixed points remain available."
                )
            }
        }
    }

    pub(super) fn save_camera_face(&mut self, number: u8) {
        let result = (|| -> Result<(u64, Acceptance), String> {
            if self.macros.recording() {
                return Err("A detected face cannot be accepted while recording. Save or cancel recording first.".into());
            }
            let camera = self.camera.as_ref().ok_or("Camera is closed.")?;
            let entry = camera
                .picker
                .iter()
                .find(|entry| entry.number == Some(number))
                .ok_or("The target page changed.")?;
            let Proposal {
                number: face_number,
                face,
                ..
            } = entry.face.ok_or("This entry is not a face proposal.")?;
            let center = entry
                .center
                .ok_or("That face is outside the selected beat's visible input.")?;
            let workspace = self.workspace.as_ref().ok_or("Open a project first.")?;
            let (id, label) = fresh_face_target(&workspace.document, face_number)?;
            let span = camera
                .faces
                .span
                .ok_or("The face proposal range changed.")?;
            let asset = camera
                .picture
                .as_ref()
                .ok_or("The Original picture changed.")?
                .0
                .clone();
            let target = deadpan_cli::faces::face_target(
                &face,
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
            camera.face_framing(&acceptance)?;
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
                        .unwrap_or_else(|| "The face target could not be saved.".into())
                })?;
            Ok((ticket, acceptance))
        })();
        match result {
            Ok((ticket, acceptance)) => {
                if let Some(camera) = &mut self.camera {
                    camera.saving = Some(ticket);
                    camera.face_acceptance = Some(acceptance);
                }
                self.message =
                    Some("Saving the selected face target… Framing remains unsaved.".into());
            }
            Err(reason) => {
                if let Some(camera) = &mut self.camera {
                    camera.draft.target_unavailable(number);
                }
                self.error = Some(reason);
            }
        }
    }

    pub(super) fn camera_face_reply(&mut self, ticket: u64, refusal: Option<&str>) {
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
            camera.face_reply(ticket, refusal, saved.as_ref());
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
            faces: Faces::default(),
            face_acceptance: None,
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
                    region,
                    TargetSource::Initial,
                )
            }),
            point: (!proposed && number >= 64).then_some([0.4, 0.5]),
            center: Some([ratio(2, 5), ratio(1, 2)]),
            face: proposed.then(|| Proposal {
                number: (number % 64 + 1) as u8,
                region,
                face: deadpan_cli::faces::DetectedFace {
                    region: deadpan_analysis::NormalizedRect::new(0.35, 0.4, 0.1, 0.2).unwrap(),
                    confidence: 0.8,
                },
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
        camera.picker.extend((69..133).map(|n| entry(n, true)));
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
        for page in 0..15 {
            assert_eq!(camera.picker_page, page);
            for entry in camera.picker.iter().filter(|e| e.number.is_some()) {
                assert!((1..=9).contains(&entry.number.unwrap()));
                assert!(reached.insert(entry.label.clone()));
            }
            camera.picker_page(false);
        }
        assert_eq!(reached.len(), 133);
        assert_eq!(camera.picker_page, 0);
        camera.picker_page(true);
        assert_eq!(camera.picker_page, 14);
        camera.close_faces();
        assert_eq!(camera.picker.len(), 69);
        assert_eq!(camera.picker_page, 0);
        assert!(camera.picker.iter().all(|e| e.face.is_none()));
    }

    #[test]
    fn face_reply_requires_live_picker_request_and_exact_session_revision_picture() {
        let mut camera = session();
        camera.draft.input(CameraKey::RefreshTargets, None);
        camera.faces.pending = Some(20);
        let job = FaceJob {
            ticket: 20,
            session: 7,
            revision: camera.revision.clone(),
            asset: AssetId::new("video").unwrap(),
            pts: 1001,
            outcome: None,
        };
        assert!(matching_job(&camera, &job));
        for wrong in [
            FaceJob {
                ticket: 19,
                ..job.clone()
            },
            FaceJob {
                session: 8,
                ..job.clone()
            },
            FaceJob {
                revision: RevisionId::new("r2").unwrap(),
                ..job.clone()
            },
            FaceJob {
                asset: AssetId::new("other").unwrap(),
                ..job.clone()
            },
            FaceJob {
                pts: 2002,
                ..job.clone()
            },
        ] {
            assert!(!matching_job(&camera, &wrong));
        }
        camera.draft.input(CameraKey::RefreshTargets, None);
        assert!(!matching_job(&camera, &job));
        camera.close_faces();
        camera.draft.input(CameraKey::RefreshTargets, None);
        camera.faces.pending = Some(21);
        assert!(!matching_job(&camera, &job));
        camera.face_reply(21, Some("busy"), None);
        assert!(camera.faces.pending.is_none());
        assert!(camera.faces.status.contains("busy"));
        camera.faces.pending = Some(22);
        camera.face_reply(23, None, None);
        assert!(camera.faces.pending.is_none());
        assert!(camera.faces.status.contains("superseded"));
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
            clock: FramingClock::OwnerOutput,
            scale_transition: None,
        });
        let prior_follow = camera.follow.clone();
        camera.draft.input(CameraKey::RefreshTargets, None);
        camera.draft.input(CameraKey::Digit(4), None);
        let before = camera.draft.pose();
        camera.selected_target = Some(TargetId::new("prior").unwrap());
        camera.saving = Some(30);
        camera.face_acceptance = Some(Acceptance {
            id: TargetId::new("new").unwrap(),
            center: [ratio(3, 4), ratio(1, 4)],
            number: 4,
        });
        camera.face_reply(29, Some("older failure"), None);
        assert!(camera.face_acceptance.is_some());
        camera.face_reply(30, Some("target limit"), None);
        assert!(camera.face_acceptance.is_none());
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
        camera.face_acceptance = Some(Acceptance {
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
        camera.face_reply(31, Some("revision changed"), Some(&saved));
        assert_eq!(camera.face_acceptance.as_ref().unwrap().id, accepted);
        camera.face_reply(32, None, Some(&saved));
        assert_eq!(camera.face_acceptance.as_ref().unwrap().id, accepted);
        let superseded = crate::project::targets::Saved {
            id: TargetId::new("other").unwrap(),
            ..saved.clone()
        };
        camera.face_reply(33, None, Some(&superseded));
        assert!(camera.face_acceptance.is_none());
        camera.face_acceptance = Some(Acceptance {
            id: accepted,
            center: [ratio(3, 4), ratio(1, 4)],
            number: 4,
        });
        camera.face_reply(30, Some("save refused"), Some(&saved));
        assert!(camera.face_acceptance.is_none());
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
            clock: FramingClock::RetainedOutput {
                offset: ratio(1, 3),
                duration: ratio(17, 2),
            },
            scale_transition: Some(deadpan_core::FollowScaleTransition {
                factor: ratio(2, 1),
                curve: deadpan_core::framing_zoom::ZoomCurve::Smoothstep,
            }),
        };
        camera.follow = Some(follow.clone());
        let scale = camera.draft.pose().scale;
        let acceptance = Acceptance {
            id: TargetId::new("face-1").unwrap(),
            center: [ratio(3, 4), ratio(1, 4)],
            number: 7,
        };
        camera.face_framing(&acceptance).unwrap();
        assert_eq!(camera.follow, Some(follow.clone()));
        camera.accept_saved_face(acceptance).unwrap();
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
                .accept_saved_face(Acceptance {
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
}
