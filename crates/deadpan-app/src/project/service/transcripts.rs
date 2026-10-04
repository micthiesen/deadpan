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
            // Publish the preferred stored transcript, the same one operators
            // and macros read, so motions and edits never use different words.
            let (key, transcript) = deadpan_cli::speech::stored_transcript(store, &key.content)
                .unwrap_or_else(|| (key, (*transcript).clone()));
            Ok(Arc::new(workspace.with_transcript(Arc::new(
                crate::project::OriginalTranscript { key, transcript },
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
            Ok(Arc::new(workspace.with_speech_activity(Arc::new(
                crate::project::OriginalActivity { key, activity },
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
