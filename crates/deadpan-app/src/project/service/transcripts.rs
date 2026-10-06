//! Saving transcripts and speech activity: annotations outside document
//! history.

use super::*;

impl Service {
    /// Store a transcript for the current session's Original and publish it
    /// with the workspace. The document revision and history are unchanged.
    pub(super) fn save_transcript_command(
        &mut self,
        expected_session: u64,
        attempt: u64,
        key: deadpan_store::TranscriptKey,
        transcript: Arc<deadpan_analysis::Transcript>,
    ) {
        let result = (|| -> Result<Arc<Workspace>> {
            let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
            if workspace.session != expected_session {
                return Err("The transcript belongs to a different project session".into());
            }
            require_original(workspace, &key.content, key.audio_stream)?;
            let store = self.store.as_ref().ok_or("Open a project first")?;
            store.save_transcript(&key, &transcript).map_err(display)?;
            // Publish the preferred stored transcript with the person's
            // corrections, the same words operators and macros read, so
            // motions and edits never use different words.
            let words = deadpan_cli::speech::stored_words(store, &key.content)
                .ok_or("The saved transcript could not be read back")?;
            Ok(Arc::new(workspace.with_transcript(Arc::new(
                crate::project::OriginalTranscript::new(words),
            ))))
        })();
        let error = match result {
            Ok(workspace) => {
                self.workspace = Some(workspace);
                None
            }
            Err(error) => Some(error.to_string()),
        };
        self.transcript_save = Some(crate::project::TranscriptSave {
            session: expected_session,
            attempt,
            error,
        });
    }

    /// Store speech activity for the current session's Original and publish
    /// the preferred stored activity with the workspace. The document
    /// revision and history are unchanged.
    pub(super) fn save_speech_activity_command(
        &mut self,
        expected_session: u64,
        attempt: u64,
        key: deadpan_store::SpeechActivityKey,
        activity: Arc<deadpan_analysis::SpeechActivity>,
    ) {
        let result = (|| -> Result<Arc<Workspace>> {
            let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
            if workspace.session != expected_session {
                return Err("The speech activity belongs to a different project session".into());
            }
            require_original(workspace, &key.content, key.audio_stream)?;
            let store = self.store.as_ref().ok_or("Open a project first")?;
            store
                .save_speech_activity(&key, &activity)
                .map_err(display)?;
            let (key, activity) = deadpan_cli::activity::stored_activity(store, &key.content)
                .unwrap_or_else(|| (key, (*activity).clone()));
            let (pauses, corrections_error) = deadpan_cli::speech::corrected_pauses(
                store,
                &key.content,
                key.audio_stream,
                &activity,
            );
            Ok(Arc::new(workspace.with_speech_activity(Arc::new(
                crate::project::OriginalActivity {
                    key,
                    activity,
                    pauses,
                    corrections_error,
                },
            ))))
        })();
        let error = match result {
            Ok(workspace) => {
                self.workspace = Some(workspace);
                None
            }
            Err(error) => Some(error.to_string()),
        };
        self.activity_save = Some(crate::project::TranscriptSave {
            session: expected_session,
            attempt,
            error,
        });
    }

    /// Apply, undo or redo a correction of the Original's transcript or
    /// pauses, then publish the corrected analyses. The document revision and
    /// history are unchanged; corrections have their own Undo and Redo.
    pub(super) fn change_corrections_command(
        &mut self,
        request: crate::project::CorrectionRequest,
    ) {
        let crate::project::CorrectionRequest {
            expected_session,
            attempt,
            key,
            expected_version,
            change,
            transcript: seen_transcript,
            activity: seen_activity,
        } = request;
        let result = (|| -> Result<Arc<Workspace>> {
            let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
            if workspace.session != expected_session {
                return Err("The correction belongs to a different project session".into());
            }
            require_original(workspace, &key.content, key.audio_stream)?;
            // A correction is computed against the analyses the person saw;
            // a newer transcript or detection would misplace it.
            fn same<T>(seen: Option<&Arc<T>>, current: Option<&Arc<T>>) -> bool {
                match (seen, current) {
                    (Some(seen), Some(current)) => Arc::ptr_eq(seen, current),
                    (None, None) => true,
                    _ => false,
                }
            }
            if !same(seen_transcript.as_ref(), workspace.transcript.as_ref())
                || !same(seen_activity.as_ref(), workspace.speech_activity.as_ref())
            {
                return Err(
                    "The transcript or pauses changed while correcting; look again and retry"
                        .into(),
                );
            }
            let current = workspace
                .corrections
                .as_ref()
                .filter(|corrections| corrections.key == key)
                .ok_or("The Original's corrections are not loaded")?;
            let discard = matches!(
                change,
                deadpan_store::CorrectionChange::DiscardUnreadable { .. }
            );
            if let Some(error) = &current.error
                && !discard
            {
                return Err(format!(
                    "The stored corrections are unreadable, so they were not changed: {error}. Discard them first."
                ));
            }
            let store = self.store.as_ref().ok_or("Open a project first")?;
            let stored = store
                .change_analysis_corrections(&key, expected_version, change)
                .map_err(display)?;
            let transcript = match &workspace.transcript {
                Some(_) => Some(Arc::new(crate::project::OriginalTranscript::new(
                    deadpan_cli::speech::stored_words(store, &key.content).ok_or(
                        "The corrections were saved, but the transcript could not be read back; reopen the project",
                    )?,
                ))),
                None => None,
            };
            let activity = workspace.speech_activity.as_ref().map(|activity| {
                Arc::new(crate::project::OriginalActivity {
                    key: activity.key.clone(),
                    activity: activity.activity.clone(),
                    pauses: stored.corrections.apply_to_pauses(&activity.activity),
                    corrections_error: None,
                })
            });
            Ok(Arc::new(workspace.with_corrections(
                Arc::new(crate::project::OriginalCorrections {
                    key,
                    stored: Some(stored),
                    error: None,
                }),
                transcript,
                activity,
            )))
        })();
        let error = match result {
            Ok(workspace) => {
                self.workspace = Some(workspace);
                None
            }
            Err(error) => Some(error.to_string()),
        };
        self.correction_save = Some(crate::project::TranscriptSave {
            session: expected_session,
            attempt,
            error,
        });
    }
}

impl Service {
    /// Reread the Original's corrections after a remote client changed them
    /// through the live endpoint, and republish the corrected transcript and
    /// pauses exactly as a native `:correct` save does. An open correction
    /// sheet's next change then sees a newer version and is refused.
    pub(super) fn reload_corrections(&mut self) -> Result<()> {
        let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
        let store = self.store.as_ref().ok_or("Open a project first")?;
        let Some(corrections) = super::original_corrections(
            store,
            workspace.single_source.as_ref(),
            &workspace.sources,
        ) else {
            return Ok(());
        };
        let transcript =
            super::original_transcript(store, workspace.single_source.as_ref(), &workspace.sources);
        let activity =
            super::original_activity(store, workspace.single_source.as_ref(), &workspace.sources);
        self.workspace = Some(Arc::new(workspace.with_corrections(
            corrections,
            transcript,
            activity,
        )));
        Ok(())
    }
}

/// Analyses are saved only for the session's ready Original and the audio
/// stream it was analysed from, so the published annotation always describes
/// the media on screen.
fn require_original(workspace: &Workspace, content: &str, audio_stream: u32) -> Result<()> {
    let Some(deadpan_store::single_source::SingleSourceState::Ready { asset, .. }) =
        workspace.single_source.as_ref()
    else {
        return Err("The project has no ready Original to analyse".into());
    };
    let receipt = &workspace
        .sources
        .get(asset)
        .ok_or("The Original is not registered")?
        .receipt;
    let stream = receipt
        .snapshot()
        .audio()
        .map(|audio| audio.stream().stream_index);
    if receipt.original().content().to_string() != content || stream != Some(audio_stream) {
        return Err("The analysis belongs to different media than the project's Original".into());
    }
    Ok(())
}
