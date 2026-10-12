//! Complete independent attachment input for a checked treatment owner.
//! Saved sound clocks, current gates and event edges/gain stay on their own
//! paths. Original audio and owner treatments are separate graph operands.

use deadpan_plan::AudioSoundOwnerContext;

use super::*;

/// Independent beat attachments after time/pitch, current gates, event edges
/// and event gain, before any owner treatment or Original/root-event mixing.
/// Output remains f64 so later shared nonlinear processing need not round the
/// contribution sum. This handle does not grant final project delivery.
#[derive(Debug, Clone, Serialize)]
pub struct BeatSoundOwnerInputBlock {
    pub schema_version: u32,
    pub stage: &'static str,
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub owner: InstancePath,
    pub start: AudioSample,
    pub samples: Vec<[f64; 2]>,
    pub suppressed: Vec<Range<AudioSample>>,
}

struct OwnerRead {
    samples: Vec<[f64; 2]>,
    metadata: ReadBlock,
}

impl StageAudio {
    /// Read complete current and retained beat-sound input behind exterior
    /// crops. Saved move/copy journals transport complete processor input;
    /// delivery masks apply after the temporal owner's output. Signed windows
    /// may include history outside the nominal current allocation and return
    /// zero beyond retained support. Root SoundEvents remain outside this
    /// operand, including when the selected owner is the root.
    pub fn read_beat_sound_owner_input(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        context: &AudioSoundOwnerContext<'_>,
        start: AudioSample,
        frames: u32,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<BeatSoundOwnerInputBlock, StageAudioError> {
        check_cancel(cancelled)?;
        validate_timeout(timeout)?;
        let work = RefCell::new(ReadWork::default());
        let control = WorkControl {
            cancelled,
            deadline: Instant::now() + timeout,
            work: &work,
        };
        let block =
            self.read_beat_sound_owner_controlled(provider, context, start, frames, control)?;
        Ok(BeatSoundOwnerInputBlock {
            schema_version: 1,
            stage: "beat_sound_owner_input_before_owner_treatments",
            project_id: self.plan.metadata().project_id.clone(),
            revision_id: self.plan.metadata().revision_id.clone(),
            owner: context.owner().clone(),
            start,
            samples: block.samples,
            suppressed: block.metadata.suppressed,
        })
    }

    fn read_beat_sound_owner_controlled(
        &mut self,
        provider: &mut impl AudioSourceProvider,
        context: &AudioSoundOwnerContext<'_>,
        start: AudioSample,
        frames: u32,
        control: WorkControl<'_>,
    ) -> Result<OwnerRead, StageAudioError> {
        control.check()?;
        if !context.belongs_to(&self.plan) {
            return Err(StageAudioError::ForeignDomain);
        }
        let end = AudioSample(
            start
                .0
                .checked_add(i64::from(frames))
                .ok_or(TimeError::Overflow)?,
        );
        if frames == 0 || frames > MAX_OUTPUT_FRAMES {
            return Err(StageAudioError::Range);
        }
        control.spend_plan_work(context.construction_work())?;
        let plan = Arc::clone(&self.plan);
        let retained = self.prepare_sound_processing_plans_in_context(
            &plan,
            start..end,
            control,
            Some(context),
        )?;
        let mut voices =
            self.prepare_beat_sounds(&plan, &retained, start..end, control, Some(context))?;
        let mut mix = group_mix::GroupMix::plan(&plan, start, frames, false, control)?;
        for voice in &mut voices {
            voice.route_mix(&mut mix, control)?;
        }
        let routed = voices.iter().any(|voice| voice.has_routed());
        let reservation = (u64::from(frames) * if routed { 6 } else { 5 })
            .checked_add(mix.resident_frames()?)
            .ok_or(StageAudioError::Limit("sound owner input PCM"))?;
        self.make_room(reservation, false, control)?;
        self.active_frames += reservation;
        let result = (|| {
            mix.allocate();
            let mut metadata = ReadBlock {
                start,
                samples: vec![[0.0; 2]; frames as usize],
                suppressed: vec![start..end],
                exhausted: Vec::new(),
                dependencies: Dependencies::new(),
                relative_depth: 0,
            };
            self.render_beat_sounds(provider, &mut metadata, &mut mix, voices, control)?;
            let samples = mix.finish(control)?;
            control.check()?;
            Ok(OwnerRead { samples, metadata })
        })();
        self.active_frames -= reservation;
        result
    }
}
