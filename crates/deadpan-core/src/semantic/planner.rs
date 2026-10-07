//! Incremental pure planning. A host commits the returned Compound only after
//! validating its frozen bank, live revision and every ordinary leaf boundary.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use crate::{
    AudioTimingId, CapturedEditSlice, Command, CommandRequest, EditError, EditErrorCode,
    FrameRange, GroupSelectionIdentities, LeafEdit, MAX_COMPOUND_CAPTURE_BYTES,
    MAX_COMPOUND_DOCUMENT_BYTES, MAX_COMPOUND_STEPS, MAX_DOCUMENT_JSON_BYTES, MarkId, NodeId,
    NodeKind, ProjectDocument, ProjectFrame, RegisterName, RegisterValue,
    RepeatSelectionIdentities, ResolvedStep, ResolvedTransaction, RevisionId, SemanticInstruction,
    SemanticMotion, SemanticObjectSelection, SemanticObjectTarget, SemanticProgram,
    SemanticSelector, SemanticTextObject, SliceCaptureSelection, SliceIdentityRequirements,
    SlicePasteIdentities, SourceNode, SpeechMotion, SpeechTimeline, SplitIdentities,
    compound::wire,
};

use super::{MAX_SEMANTIC_CALL_DEPTH, MAX_SEMANTIC_INSTRUCTION_FUEL};

mod beat_object;
mod content;
mod gag_edit;
mod group;
mod pause;
pub use pause::copied_moment_audio;
mod repeat;
mod retime;
mod role_repeat;
mod selection;
mod split_edit;

/// Exact Visual ownership. Time preserves oriented boundaries, including empty
/// intervals; Object retains its checked group identity. A finished selection
/// remains independent of the cursor and selected child.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SemanticVisualSelection {
    Time {
        anchor: ProjectFrame,
        head: ProjectFrame,
        extending: bool,
    },
    Object {
        selection: SemanticObjectSelection,
        extending: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SemanticCaptureProvenance {
    pub timing: AudioTimingId,
    pub parent: NodeId,
    /// Absolute bounds of the effective parent at capture entry.
    pub bounds: FrameRange,
    /// Ordinary ancestor path, excluding root and including effective parent.
    pub scope: Vec<NodeId>,
    pub scope_labels: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticContext {
    pub parent: NodeId,
    /// Absolute Edit boundary, including either endpoint of the ordinary scope.
    pub cursor: ProjectFrame,
    /// Explicit direct-child selection, independent of the cursor. None means
    /// no selected beat, including when nonempty children surround the cursor.
    pub selected_child: Option<NodeId>,
    pub visual_selection: Option<SemanticVisualSelection>,
}

/// Frozen register contents and the version the host must admit at commit.
#[derive(Debug, Clone, Copy)]
pub struct SemanticRegisterBank<'a> {
    pub entries: &'a BTreeMap<RegisterName, Arc<RegisterValue>>,
    pub version: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticAllocationRequest {
    Group {
        step_index: usize,
        required_split_ids: usize,
    },
    Ungroup {
        step_index: usize,
    },
    SetRepeatPlays {
        step_index: usize,
    },
    /// A gap change with one fresh node per independent gap Hold.
    SetRepeatGaps {
        step_index: usize,
        branches: usize,
    },
    /// A parameter-only edit, such as a new Repeat's escalation.
    ParameterEdit {
        step_index: usize,
    },
    InsertPause {
        step_index: usize,
        required_split_ids: usize,
    },
    Repeat {
        step_index: usize,
        required_split_ids: usize,
        needs_group: bool,
    },
    Cut {
        step_index: usize,
        required_split_ids: usize,
    },
    Yank {
        step_index: usize,
    },
    PasteEdited {
        step_index: usize,
        requirements: SliceIdentityRequirements,
        required_split_ids: usize,
    },
    PasteOriginal {
        step_index: usize,
        required_split_ids: usize,
    },
    /// An adjacent Source Roll, with one fresh crop wrapper when needed.
    Roll {
        step_index: usize,
        needs_wrapper: bool,
    },
    /// One new root sound event.
    Sound {
        step_index: usize,
    },
    /// One fresh Retime wrapper around the selected beat.
    WrapRetime {
        step_index: usize,
    },
    /// Fresh identities for one Explode, per [`crate::ExplodeRequirements`].
    Explode {
        step_index: usize,
        nodes: usize,
        marks: usize,
    },
    /// Fresh identities for `rib` keeping a beat's attachments on its first
    /// play. Logical marks move without allocating replacement identities.
    Isolation {
        step_index: usize,
        nodes: usize,
        marks: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticAllocation {
    Group {
        new_revision: RevisionId,
        identities: GroupSelectionIdentities,
    },
    Ungroup {
        new_revision: RevisionId,
    },
    SetRepeatPlays {
        new_revision: RevisionId,
    },
    SetRepeatGaps {
        new_revision: RevisionId,
        nodes: Vec<NodeId>,
    },
    ParameterEdit {
        new_revision: RevisionId,
    },
    InsertPause {
        new_revision: RevisionId,
        id: NodeId,
        split: crate::SplitIdentities,
    },
    Repeat {
        new_revision: RevisionId,
        identities: RepeatSelectionIdentities,
    },
    Cut {
        new_revision: RevisionId,
        capture_revision: RevisionId,
        split_identities: SplitIdentities,
    },
    Yank {
        capture_revision: RevisionId,
    },
    PasteEdited {
        new_revision: RevisionId,
        identities: SlicePasteIdentities,
        split_identities: SplitIdentities,
    },
    PasteOriginal {
        new_revision: RevisionId,
        node: NodeId,
        split_identities: SplitIdentities,
    },
    Roll {
        new_revision: RevisionId,
        wrapper: Option<NodeId>,
    },
    Sound {
        new_revision: RevisionId,
        id: crate::SoundId,
    },
    WrapRetime {
        new_revision: RevisionId,
        id: NodeId,
    },
    Explode {
        new_revision: RevisionId,
        identities: crate::OccurrenceIdentities,
    },
    Isolation {
        new_revision: RevisionId,
        identities: crate::OccurrenceIdentities,
    },
}

/// Ordered instruction-entry trace. Call rows precede their expanded bodies and
/// include the final context after all their repetitions. Motions that clamp to
/// the same boundary still consume one fuel unit and retain a trace row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticTrace {
    pub instruction: SemanticInstruction,
    pub before_revision: RevisionId,
    pub before_scope: FrameRange,
    pub before: SemanticContext,
    pub after: SemanticContext,
    pub resolved_range: Option<FrameRange>,
    pub resolved_parent: Option<NodeId>,
    pub capture: Option<SemanticCaptureProvenance>,
    /// Exact staged capture target for Yank or Cut. Together with `resolved_parent`
    /// this distinguishes whole children, including empty ones, from ranges.
    pub resolved_selection: Option<SliceCaptureSelection>,
    /// The old interval removed by a replacement. `resolved_range` contains
    /// the imported interval in the resulting document.
    pub removed_range: Option<FrameRange>,
    /// Exact staged direct-child label for a whole-child Yank or Cut capture.
    pub captured_child_label: Option<String>,
    pub depth: usize,
}

#[derive(Debug)]
pub struct SemanticPlan {
    /// Motion-only programs produce no request. A Yank-only request updates the
    /// bank without changing `document`'s revision or creating authored history.
    pub request: Option<CommandRequest>,
    pub document: ProjectDocument,
    pub context: SemanticContext,
    /// Final explicit selection, also retained in `context.selected_child`.
    pub selected_child: Option<NodeId>,
    /// Final writes only, including the unnamed alias of each named cut.
    pub register_writes: BTreeMap<RegisterName, Arc<RegisterValue>>,
    pub trace: Vec<SemanticTrace>,
}

/// Resolve and apply each instruction once against the preceding staged state.
/// The allocator supplies identities only; it must not publish authored state.
/// On any error the input document and bank remain untouched. The host retains
/// responsibility for historical identity uniqueness and measured media admission.
/// Word and sentence selectors are unavailable; see [`plan_semantic_with_speech`].
pub fn plan_semantic(
    document: &ProjectDocument,
    context: &SemanticContext,
    program: &SemanticProgram,
    registers: SemanticRegisterBank<'_>,
    new_revision: RevisionId,
    allocate: impl FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    resolve_original: impl FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
) -> Result<SemanticPlan, EditError> {
    plan_semantic_with_speech(
        document,
        context,
        program,
        registers,
        new_revision,
        allocate,
        resolve_original,
        |_| Err(speech_unavailable()),
        |_, _| Err(pause_unavailable()),
    )
}

/// The error for pause insertion without a host picture resolver.
pub fn pause_unavailable() -> EditError {
    EditError::new(
        EditErrorCode::SelectionUnavailable,
        "pause insertion needs the project's measured pictures",
    )
}

/// Where a new silent freeze pause goes, which decides the picture it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PauseSite {
    /// A pause inserted at an Edit boundary holds the picture before it.
    Boundary { at: ProjectFrame },
    /// A gap of `repeat` holds the last picture of its first play, shown at
    /// `frame`. Composition at and above the Repeat stays live on the gap and
    /// must not be captured again.
    RepeatGap { repeat: NodeId, frame: ProjectFrame },
    /// A pause at `at` that plays the `frames` before it backwards. With
    /// `bounce` it starts one picture earlier, so the picture before `at`
    /// is not shown twice; the provider then covers `frames - 1`.
    Reverse {
        at: ProjectFrame,
        frames: crate::FrameDuration,
        bounce: bool,
    },
    /// A pause standing in for the `frames` before `at`: the same pictures
    /// played forward, with a tone in place of their sound.
    Bleep {
        at: ProjectFrame,
        frames: crate::FrameDuration,
        frequency_hz: u32,
        level: crate::GainDb,
    },
}

/// The frozen picture a pause inserted at a boundary shows, resolved by the
/// host from the staged document's picture before that boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PauseProvider {
    pub video: crate::HoldVideo,
    pub picture_context: Option<crate::CapturedFraming>,
    /// Silence for an ordinary pause; reversed audio or a tail otherwise.
    pub audio: crate::HoldAudio,
}

/// The error for word and sentence selectors without a transcript.
pub fn speech_unavailable() -> EditError {
    EditError::new(
        EditErrorCode::SelectionUnavailable,
        "words are not ready: the Original has no transcript yet",
    )
}

/// [`plan_semantic`] with recognized speech. `resolve_speech` projects the
/// transcript through a staged document onto the Edit clock; it is called at
/// most once per staged document, only when an instruction needs speech.
#[allow(clippy::too_many_arguments)]
pub fn plan_semantic_with_speech(
    document: &ProjectDocument,
    context: &SemanticContext,
    program: &SemanticProgram,
    registers: SemanticRegisterBank<'_>,
    new_revision: RevisionId,
    allocate: impl FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    resolve_original: impl FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
    resolve_speech: impl FnMut(&ProjectDocument) -> Result<Arc<SpeechTimeline>, EditError>,
    resolve_pause: impl FnMut(&ProjectDocument, PauseSite) -> Result<PauseProvider, EditError>,
) -> Result<SemanticPlan, EditError> {
    program.validate()?;
    document.validate()?;
    crate::command::check_revision(
        document,
        document.project_id(),
        document.revision_id(),
        &new_revision,
    )?;
    let bounds = validate_context(document, context)?;
    let document_bytes = wire::size(document, MAX_DOCUMENT_JSON_BYTES)?;
    let mut revisions: BTreeSet<_> = document
        .audio_bindings()
        .allocation_ids()
        .into_iter()
        .cloned()
        .collect();
    revisions.extend(
        document
            .audio_lineage()
            .values()
            .map(|lineage| lineage.allocation.clone()),
    );
    for node in document.nodes().values() {
        if let NodeKind::Repeat { iterations, .. } = &node.kind {
            revisions.extend(
                iterations
                    .segments()
                    .map(|(revision, _, _)| revision.clone()),
            );
        }
    }
    revisions.insert(document.revision_id().clone());
    if !revisions.insert(new_revision.clone()) {
        return Err(identity(
            "macro outer revision reuses an existing allocation",
        ));
    }
    let child_ends = child_ends(document, &context.parent, bounds)?;
    let mut planner = Planner {
        current: document.clone(),
        context: context.clone(),
        bounds,
        child_indices: child_indices(&child_ends),
        child_ends,
        bank: registers.entries,
        inputs: BTreeMap::new(),
        writes: BTreeMap::new(),
        steps: Vec::new(),
        trace: Vec::new(),
        calls: Vec::new(),
        nodes: occupied_nodes(document),
        marks: document.marks().keys().cloned().collect(),
        revisions,
        document_bytes,
        captured_bytes: 0,
        allocate,
        resolve_original,
        resolve_speech,
        resolve_pause,
        speech: None,
    };
    planner.execute(program)?;
    let selected_child = planner.context.selected_child.clone();
    let request = if planner.steps.is_empty() {
        None
    } else {
        if planner.steps.iter().any(|step| step.edit().is_some()) {
            planner.current.revision_id = new_revision.clone();
            charge(
                &mut planner.document_bytes,
                wire::size(&planner.current, MAX_DOCUMENT_JSON_BYTES)?,
                MAX_COMPOUND_DOCUMENT_BYTES,
                "macro staged document byte limit",
            )?;
        }
        Some(CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision,
            command: Command::Compound {
                transaction: ResolvedTransaction::new(
                    registers.version,
                    planner.inputs,
                    planner.steps,
                )?,
            },
        })
    };
    Ok(SemanticPlan {
        request,
        document: planner.current,
        context: planner.context,
        selected_child,
        register_writes: planner.writes,
        trace: planner.trace,
    })
}

struct Planner<'a, F, R, S, P> {
    current: ProjectDocument,
    context: SemanticContext,
    bounds: (ProjectFrame, ProjectFrame),
    /// Updated only after an authored leaf. Frame motions use a binary search
    /// instead of walking the entire staged document for every instruction.
    child_ends: Vec<(NodeId, ProjectFrame)>,
    child_indices: BTreeMap<NodeId, usize>,
    bank: &'a BTreeMap<RegisterName, Arc<RegisterValue>>,
    inputs: BTreeMap<RegisterName, Option<Arc<RegisterValue>>>,
    writes: BTreeMap<RegisterName, Arc<RegisterValue>>,
    steps: Vec<ResolvedStep>,
    trace: Vec<SemanticTrace>,
    calls: Vec<RegisterName>,
    nodes: BTreeSet<NodeId>,
    marks: BTreeSet<MarkId>,
    revisions: BTreeSet<RevisionId>,
    document_bytes: usize,
    captured_bytes: usize,
    allocate: F,
    resolve_original: R,
    resolve_speech: S,
    resolve_pause: P,
    /// Speech for the staged document after this many resolved steps.
    speech: Option<(usize, Arc<SpeechTimeline>)>,
}

impl<F, R, S, P> Planner<'_, F, R, S, P>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
    S: FnMut(&ProjectDocument) -> Result<Arc<SpeechTimeline>, EditError>,
    P: FnMut(&ProjectDocument, PauseSite) -> Result<PauseProvider, EditError>,
{
    /// Project speech through the current staged document once per change.
    fn ensure_speech(&mut self) -> Result<(), EditError> {
        if self
            .speech
            .as_ref()
            .is_some_and(|(steps, _)| *steps == self.steps.len())
        {
            return Ok(());
        }
        let speech = (self.resolve_speech)(&self.current)?;
        self.speech = Some((self.steps.len(), speech));
        Ok(())
    }

    fn speech(&self) -> Result<&SpeechTimeline, EditError> {
        self.speech
            .as_ref()
            .filter(|(steps, _)| *steps == self.steps.len())
            .map(|(_, speech)| speech.as_ref())
            .ok_or_else(speech_unavailable)
    }

    /// Speech whose words are available.
    fn words(&self) -> Result<&SpeechTimeline, EditError> {
        let speech = self.speech()?;
        speech.require(crate::SpeechUnit::Word)?;
        Ok(speech)
    }

    fn execute(&mut self, program: &SemanticProgram) -> Result<(), EditError> {
        for instruction in program.instructions() {
            if self.trace.len() == MAX_SEMANTIC_INSTRUCTION_FUEL {
                return Err(limit("macro instruction fuel exhausted"));
            }
            let index = self.trace.len();
            self.trace.push(SemanticTrace {
                instruction: instruction.clone(),
                before_revision: self.current.revision_id().clone(),
                before_scope: FrameRange::new(self.bounds.0, self.bounds.1)
                    .map_err(crate::DocumentError::from)?,
                before: self.context.clone(),
                after: self.context.clone(),
                resolved_range: None,
                resolved_parent: None,
                capture: None,
                resolved_selection: None,
                removed_range: None,
                captured_child_label: None,
                depth: self.calls.len(),
            });
            if instruction.uses_speech() {
                self.ensure_speech()?;
            }
            match instruction {
                SemanticInstruction::MoveFrames { forward, count } => {
                    self.move_context(SemanticMotion::Frames {
                        forward: *forward,
                        count: *count,
                    })?;
                }
                SemanticInstruction::MoveBeats { forward, count } => {
                    self.move_context(SemanticMotion::Beats {
                        forward: *forward,
                        count: *count,
                    })?;
                }
                SemanticInstruction::MoveScope { end } => {
                    self.move_context(SemanticMotion::Scope { end: *end })?;
                }
                SemanticInstruction::MoveWords {
                    forward,
                    count,
                    end,
                } => {
                    self.move_context(SemanticMotion::Words {
                        forward: *forward,
                        count: *count,
                        end: *end,
                    })?;
                }
                SemanticInstruction::MoveSentences { forward, count } => {
                    self.move_context(SemanticMotion::Sentences {
                        forward: *forward,
                        count: *count,
                    })?;
                }
                SemanticInstruction::MovePauses { forward, count } => {
                    self.move_context(SemanticMotion::Pauses {
                        forward: *forward,
                        count: *count,
                    })?;
                }
                SemanticInstruction::MoveShots { forward, count } => {
                    self.move_context(SemanticMotion::Shots {
                        forward: *forward,
                        count: *count,
                    })?;
                }
                SemanticInstruction::SelectSpeech { object } => {
                    self.context = self.speech()?.select_object(
                        &self.context,
                        self.bounds,
                        *object,
                        self.current.presentation_basis().frame_rate,
                    )?;
                    let target = self.resolve_selector(SemanticSelector::VisualSelection)?;
                    self.trace[index].resolved_parent = Some(target.parent);
                    self.trace[index].resolved_range = Some(target.range);
                    self.trace[index].resolved_selection = target.selection;
                }
                SemanticInstruction::SelectObject { object } => {
                    self.charge_resolution()?;
                    self.context = self
                        .current
                        .select_semantic_object(&self.context, *object)?;
                    let target = self.resolve_selector(SemanticSelector::VisualSelection)?;
                    self.trace[index].resolved_parent = Some(target.parent);
                    self.trace[index].resolved_range = Some(target.range);
                    self.trace[index].resolved_selection = target.selection;
                }
                SemanticInstruction::BeginSelection => {
                    self.context.visual_selection = Some(SemanticVisualSelection::Time {
                        anchor: self.context.cursor,
                        head: self.context.cursor,
                        extending: true,
                    });
                }
                SemanticInstruction::FinishSelection => {
                    self.finish_selection()?;
                }
                SemanticInstruction::ClearSelection => {
                    self.context.visual_selection = None;
                }
                SemanticInstruction::Yank { selector, register } => {
                    self.capture_selector(index, *register, *selector, false)?;
                }
                SemanticInstruction::Cut { selector, register } => {
                    self.capture_selector(index, *register, *selector, true)?;
                }
                SemanticInstruction::Group { selector, label } => {
                    self.group(index, *selector, label)?;
                }
                SemanticInstruction::Ungroup => self.ungroup(index)?,
                SemanticInstruction::Explode => self.explode(index)?,
                SemanticInstruction::Duplicate { selector } => {
                    self.duplicate(index, *selector)?;
                }
                SemanticInstruction::Repeat {
                    selector,
                    plays,
                    escalation,
                } => {
                    self.repeat(index, *selector, plays.get(), *escalation)?;
                }
                SemanticInstruction::SetRepeatPlays { plays } => {
                    self.set_repeat_plays(index, plays.get())?;
                }
                SemanticInstruction::SetAudio { change } => {
                    self.set_audio(index, *change)?;
                }
                SemanticInstruction::SplitEdit { kind, length } => {
                    self.split_edit(index, *kind, *length)?;
                }
                SemanticInstruction::DeleteRole { role } => {
                    self.delete_role(index, *role)?;
                }
                SemanticInstruction::RoleRepeat { role, plays, trim } => {
                    self.role_repeat(index, *role, plays.get(), *trim)?;
                }
                SemanticInstruction::SetRepeat {
                    plays,
                    gaps,
                    escalation,
                } => {
                    self.set_repeat(index, *plays, gaps.as_deref(), *escalation)?;
                }
                SemanticInstruction::SetRoomTone { register } => {
                    self.set_room_tone(index, *register)?;
                }
                SemanticInstruction::SetCutaway { register, fit } => {
                    self.set_cutaway(index, *register, *fit)?;
                }
                SemanticInstruction::SetCaption {
                    text,
                    placement,
                    delay,
                    reveal,
                } => {
                    self.set_caption(index, text, *placement, *delay, *reveal)?;
                }
                SemanticInstruction::Bleep {
                    register,
                    frequency_hz,
                    level,
                } => {
                    self.bleep(index, *register, *frequency_hz, *level)?;
                }
                SemanticInstruction::Lift { register } => {
                    self.lift(index, *register)?;
                }
                SemanticInstruction::InsertReverse { length, bounce } => {
                    self.insert_reverse(index, *length, *bounce)?;
                }
                SemanticInstruction::Tail { length, effect } => {
                    self.tail(index, *length, *effect)?;
                }
                SemanticInstruction::Gag { recipe } => {
                    // The expansion runs as ordinary instructions on the
                    // staged document, with their own trace entries and fuel.
                    if recipe.frames_its_pause()
                        && self
                            .current
                            .insert_time_target(self.context.cursor)
                            .is_ok_and(|target| target.parent != self.context.parent)
                    {
                        return Err(EditError::new(
                            EditErrorCode::SelectionUnavailable,
                            format!(
                                "{} frames its pause, which here would land inside a nested group; open that group with Enter first",
                                recipe.name()
                            ),
                        ));
                    }
                    let visual = self.context.visual_selection.is_some();
                    let expansion = SemanticProgram::new(
                        recipe.expand(visual, self.current.presentation_basis().frame_rate)?,
                    )?;
                    self.execute(&expansion)?;
                }
                SemanticInstruction::InsertPause { length, black } => {
                    self.insert_pause(index, *length, *black)?;
                }
                SemanticInstruction::InsertAiPause { length } => {
                    self.insert_ai_pause(index, *length)?;
                }
                SemanticInstruction::SetFraming { framing } => {
                    self.set_framing(index, framing.as_deref().cloned())?;
                }
                SemanticInstruction::SetAudioLag { offset } => {
                    self.set_audio_lag(index, *offset)?;
                }
                SemanticInstruction::SetAudioEdges { side, policy } => {
                    self.set_audio_edges(index, *side, *policy)?;
                }
                SemanticInstruction::SetGag { recipe, parameters } => {
                    self.set_gag(index, recipe, parameters)?;
                }
                SemanticInstruction::Retime { speed, pitch, wrap } => {
                    self.retime(index, *speed, *pitch, *wrap)?;
                }
                SemanticInstruction::Pitch { semitones } => {
                    self.pitch(index, *semitones)?;
                }
                SemanticInstruction::SetHoldDuration { length } => {
                    self.set_hold_duration(index, *length)?;
                }
                SemanticInstruction::CutFrames {
                    operation,
                    register,
                } => {
                    let range = operation.resolve(
                        &self.current,
                        &self.context.parent,
                        self.context.cursor,
                    )?;
                    self.capture_instruction(
                        index,
                        *register,
                        selection::ResolvedTarget::local(
                            &self.context.parent,
                            SliceCaptureSelection::Range { range },
                            range,
                        ),
                        true,
                    )?;
                }
                SemanticInstruction::Call { register, count } => {
                    self.call(*register, count.get())?;
                }
                SemanticInstruction::YankBeat { register } => {
                    self.capture_selector(index, *register, SemanticSelector::SelectedBeat, false)?;
                }
                SemanticInstruction::YankSelection { register } => {
                    self.capture_selector(
                        index,
                        *register,
                        SemanticSelector::VisualSelection,
                        false,
                    )?;
                }
                SemanticInstruction::CutSelection { register } => {
                    self.capture_selector(
                        index,
                        *register,
                        SemanticSelector::VisualSelection,
                        true,
                    )?;
                }
                SemanticInstruction::ReplaceSelection { register } => {
                    self.replace_selection(index, *register)?;
                }
                SemanticInstruction::Paste { register, before } => {
                    self.paste_instruction(index, *register, *before)?;
                }
            }
            self.trace[index].after = self.context.clone();
        }
        Ok(())
    }

    fn call(&mut self, register: RegisterName, count: u32) -> Result<(), EditError> {
        if self.calls.contains(&register) {
            return Err(invalid("recursive macro call is forbidden"));
        }
        if self.calls.len() == MAX_SEMANTIC_CALL_DEPTH {
            return Err(limit("macro call depth exceeds 16"));
        }
        let value = if let Some(value) = self.writes.get(&register) {
            value.clone()
        } else {
            let value = self.bank.get(&register).cloned();
            self.inputs.insert(register, value.clone());
            value.ok_or_else(|| invalid("the called macro register is empty"))?
        };
        let RegisterValue::Macro { program } = value.as_ref() else {
            return Err(invalid(
                "the called register contains copied content, not a macro",
            ));
        };
        // Each repetition must execute this many instructions even when its
        // motions are no-ops. Reject enormous counts without entering the loop.
        let minimum = usize::try_from(count)
            .ok()
            .and_then(|count| count.checked_mul(program.instructions().len()))
            .ok_or_else(|| limit("macro count exceeds instruction fuel"))?;
        if minimum > MAX_SEMANTIC_INSTRUCTION_FUEL - self.trace.len() {
            return Err(limit("macro count exceeds remaining instruction fuel"));
        }
        let direct_steps = program
            .instructions()
            .iter()
            .map(|instruction| match instruction {
                // An escalating Repeat stages its wrap and its escalation.
                SemanticInstruction::Repeat {
                    escalation: Some(_),
                    ..
                } => 2,
                // A recipe stages at most three leaves.
                SemanticInstruction::Gag { .. } => 2,
                // At most four part edits and the relabel.
                SemanticInstruction::SetGag { .. } => 5,
                // Marks on the beat and both neighbors, a start and an end.
                SemanticInstruction::SetAudioEdges { .. } => 5,
                // A Roll and a cutaway.
                SemanticInstruction::SplitEdit { .. } => 2,
                // A mute and one sound per later play, or one cutaway.
                SemanticInstruction::RoleRepeat { plays, .. } => {
                    usize::try_from(plays.get()).unwrap_or(usize::MAX)
                }
                SemanticInstruction::SetRepeat {
                    plays,
                    gaps,
                    escalation,
                } => {
                    usize::from(plays.is_some())
                        + usize::from(gaps.is_some())
                        + usize::from(escalation.is_some())
                }
                _ => usize::from(matches!(
                    instruction,
                    SemanticInstruction::CutFrames { .. }
                        | SemanticInstruction::InsertPause { .. }
                        | SemanticInstruction::InsertAiPause { .. }
                        | SemanticInstruction::InsertReverse { .. }
                        | SemanticInstruction::Lift { .. }
                        | SemanticInstruction::Bleep { .. }
                        | SemanticInstruction::Tail { .. }
                        | SemanticInstruction::SetFraming { .. }
                        | SemanticInstruction::Retime { .. }
                        | SemanticInstruction::Pitch { .. }
                        | SemanticInstruction::SetHoldDuration { .. }
                        | SemanticInstruction::SetAudioLag { .. }
                        | SemanticInstruction::SetAudio { .. }
                        | SemanticInstruction::DeleteRole { .. }
                        | SemanticInstruction::Yank { .. }
                        | SemanticInstruction::Cut { .. }
                        | SemanticInstruction::Repeat { .. }
                        | SemanticInstruction::SetRepeatPlays { .. }
                        | SemanticInstruction::SetRoomTone { .. }
                        | SemanticInstruction::SetCutaway { .. }
                        | SemanticInstruction::SetCaption { .. }
                        | SemanticInstruction::Group { .. }
                        | SemanticInstruction::Ungroup
                        | SemanticInstruction::Explode
                        | SemanticInstruction::Duplicate { .. }
                        | SemanticInstruction::YankBeat { .. }
                        | SemanticInstruction::Paste { .. }
                        | SemanticInstruction::YankSelection { .. }
                        | SemanticInstruction::CutSelection { .. }
                        | SemanticInstruction::ReplaceSelection { .. }
                )),
            })
            .sum();
        let minimum_steps = usize::try_from(count)
            .ok()
            .and_then(|count| count.checked_mul(direct_steps))
            .ok_or_else(|| limit("macro count exceeds resolved editing steps"))?;
        if minimum_steps > MAX_COMPOUND_STEPS - self.steps.len() {
            return Err(limit("macro count exceeds 1024 resolved editing steps"));
        }
        // The selected Arc stays frozen for every counted repetition, even if
        // this body writes over its own register. A later Call reads that write.
        self.calls.push(register);
        for _ in 0..count {
            self.execute(program)?;
        }
        self.calls.pop();
        Ok(())
    }

    fn cut(
        &mut self,
        register: RegisterName,
        target: &selection::ResolvedTarget,
    ) -> Result<Arc<CapturedEditSlice>, EditError> {
        let selection = target.selection()?;
        let parent = &target.parent;
        if self.steps.len() == MAX_COMPOUND_STEPS {
            return Err(limit("macro exceeds 1024 resolved editing steps"));
        }
        let size = wire::size(&self.current, MAX_DOCUMENT_JSON_BYTES)?;
        charge(
            &mut self.document_bytes,
            size,
            MAX_COMPOUND_DOCUMENT_BYTES,
            "macro staged document byte limit",
        )?;
        charge(
            &mut self.captured_bytes,
            size,
            MAX_COMPOUND_CAPTURE_BYTES,
            "macro captured document byte limit",
        )?;
        let child_slot = match selection {
            SliceCaptureSelection::Child { node } => {
                Some(self.current.sequence_children(parent, node, node)?.first)
            }
            SliceCaptureSelection::Children { first, last } => {
                Some(self.current.sequence_children(parent, first, last)?.first)
            }
            SliceCaptureSelection::Range { .. } => None,
        };
        let required_split_ids = match selection {
            SliceCaptureSelection::Range { range } => {
                self.current.range_deletion(parent, *range)?.required_ids
            }
            SliceCaptureSelection::Child { .. } | SliceCaptureSelection::Children { .. } => 0,
        };
        let allocation = (self.allocate)(SemanticAllocationRequest::Cut {
            step_index: self.steps.len(),
            required_split_ids,
        })?;
        let SemanticAllocation::Cut {
            new_revision,
            capture_revision,
            split_identities,
        } = allocation
        else {
            return Err(invalid("macro cut requires a Cut allocation"));
        };
        if split_identities.nodes.len() != required_split_ids {
            return Err(invalid(
                "macro cut requires exactly its preflight Split identities",
            ));
        }
        for revision in [&new_revision, &capture_revision] {
            if !self.revisions.insert(revision.clone()) {
                return Err(identity("macro reuses a revision or capture allocation"));
            }
        }
        for node in &split_identities.nodes {
            if !self.nodes.insert(node.clone()) {
                return Err(identity("macro reuses a Split node identity"));
            }
        }
        let slice = Arc::new(CapturedEditSlice::capture_selection_with(
            &self.current,
            parent,
            selection,
            target.attachments,
            AudioTimingId {
                allocation: capture_revision,
                ordinal: 0,
            },
        )?);
        let range = slice.range();
        let timing = AudioTimingId {
            allocation: new_revision.clone(),
            ordinal: 0,
        };
        let command = match selection {
            SliceCaptureSelection::Range { range } => Command::DeleteRange {
                parent: parent.clone(),
                range: *range,
                identities: split_identities,
                timing,
            },
            SliceCaptureSelection::Child { node } => Command::DeleteRipple {
                node: node.clone(),
                timing,
            },
            SliceCaptureSelection::Children { first, last } => Command::DeleteChildren {
                parent: parent.clone(),
                first: first.clone(),
                last: last.clone(),
                timing,
            },
        };
        let delete = LeafEdit::new(new_revision, command)?;
        let applied = crate::apply(&self.current, &delete.request(&self.current))?;
        let next = applied.forward.apply(&self.current)?;
        charge(
            &mut self.document_bytes,
            wire::size(&next, MAX_DOCUMENT_JSON_BYTES)?,
            MAX_COMPOUND_DOCUMENT_BYTES,
            "macro staged document byte limit",
        )?;
        let value = Arc::new(RegisterValue::Edited {
            slice: slice.clone(),
        });
        self.writes.insert(register, value.clone());
        self.writes.insert(RegisterName::unnamed(), value);
        self.steps.push(ResolvedStep::Cut {
            name: register,
            slice: slice.clone(),
            delete,
        });
        self.current = next;
        let bounds = scope_bounds(&self.current, parent)?;
        let ends = child_ends(&self.current, parent, bounds)?;
        let selected = if let Some(slot) = child_slot {
            ends.get(slot)
                .or_else(|| ends.last())
                .map(|(node, _)| node.clone())
        } else {
            selected_child(&ends, range.start(), bounds)
        };
        self.continue_target(target, range.start(), selected)?;
        Ok(slice)
    }
}

fn child_indices(children: &[(NodeId, ProjectFrame)]) -> BTreeMap<NodeId, usize> {
    children
        .iter()
        .enumerate()
        .map(|(index, (node, _))| (node.clone(), index))
        .collect()
}

pub(super) fn scope_bounds(
    document: &ProjectDocument,
    parent: &NodeId,
) -> Result<(ProjectFrame, ProjectFrame), EditError> {
    let start = document.source_splice_boundary(parent, 0)?;
    let NodeKind::Sequence { children } = &document.nodes()[parent].kind else {
        unreachable!("source_splice_boundary admitted an ordinary Sequence")
    };
    let end = document.source_splice_boundary(parent, children.len())?;
    Ok((start, end))
}

pub(super) fn validate_context(
    document: &ProjectDocument,
    context: &SemanticContext,
) -> Result<(ProjectFrame, ProjectFrame), EditError> {
    let bounds = scope_bounds(document, &context.parent)?;
    validate_selection(document, context)?;
    selection::validate_context(document, context, bounds)?;
    Ok(bounds)
}

fn validate_selection(
    document: &ProjectDocument,
    context: &SemanticContext,
) -> Result<(), EditError> {
    let NodeKind::Sequence { children } = &document.nodes()[&context.parent].kind else {
        unreachable!("scope_bounds admitted an ordinary Sequence")
    };
    if context
        .selected_child
        .as_ref()
        .is_some_and(|selected| !children.contains(selected))
    {
        return Err(EditError::new(
            EditErrorCode::SelectionUnavailable,
            "the selected macro beat is not a direct child of its Sequence",
        ));
    }
    Ok(())
}

fn occupied_nodes(document: &ProjectDocument) -> BTreeSet<NodeId> {
    let mut nodes: BTreeSet<_> = document.nodes().keys().cloned().collect();
    nodes.extend(
        document
            .audio_lineage()
            .values()
            .map(|lineage| lineage.origin.clone()),
    );
    for layout in document.audio_bindings().timings.values() {
        nodes.extend(layout.nodes().keys().cloned());
        nodes.extend(
            layout
                .audio_lineage()
                .values()
                .map(|lineage| lineage.origin.clone()),
        );
    }
    nodes
}

fn child_ends(
    document: &ProjectDocument,
    parent: &NodeId,
    bounds: (ProjectFrame, ProjectFrame),
) -> Result<Vec<(NodeId, ProjectFrame)>, EditError> {
    let NodeKind::Sequence { children } = &document.nodes()[parent].kind else {
        unreachable!("the ordinary Sequence scope was already admitted")
    };
    let durations = document.durations()?;
    let mut start = bounds.0.0;
    let mut result = Vec::with_capacity(children.len());
    for child in children {
        let end = start
            .checked_add(durations[child].frames())
            .ok_or_else(|| {
                EditError::new(
                    EditErrorCode::TimingOverflow,
                    "macro child boundary overflow",
                )
            })?;
        result.push((child.clone(), ProjectFrame(end)));
        start = end;
    }
    Ok(result)
}

fn selected_child(
    child_ends: &[(NodeId, ProjectFrame)],
    cursor: ProjectFrame,
    bounds: (ProjectFrame, ProjectFrame),
) -> Option<NodeId> {
    if cursor == bounds.1 {
        return child_ends.last().map(|(node, _)| node.clone());
    }
    child_ends
        .get(child_ends.partition_point(|(_, end)| *end <= cursor))
        .map(|(node, _)| node.clone())
}

fn charge(
    total: &mut usize,
    amount: usize,
    maximum: usize,
    message: &str,
) -> Result<(), EditError> {
    *total = total
        .checked_add(amount)
        .filter(|value| *value <= maximum)
        .ok_or_else(|| limit(message))?;
    Ok(())
}

fn invalid(message: &str) -> EditError {
    EditError::new(EditErrorCode::InvalidCommand, message)
}
fn identity(message: &str) -> EditError {
    EditError::new(EditErrorCode::IdentityConflict, message)
}
fn limit(message: &str) -> EditError {
    EditError::new(EditErrorCode::LimitExceeded, message)
}

#[cfg(test)]
mod tests;
