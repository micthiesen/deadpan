//! Saving transcripts: annotations outside document history.

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
}
