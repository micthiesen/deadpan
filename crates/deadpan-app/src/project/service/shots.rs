//! Saving shot analysis: an annotation outside document history.

use super::*;

impl Service {
    /// Store a shot analysis of the current session's Original and publish
    /// it with the workspace. The document revision and history are
    /// unchanged.
    pub(super) fn save_shot_analysis_command(
        &mut self,
        expected_session: u64,
        attempt: u64,
        key: deadpan_store::ShotAnalysisKey,
        analysis: Arc<deadpan_analysis::ShotAnalysis>,
    ) {
        let result = (|| -> Result<Arc<Workspace>> {
            let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
            if workspace.session != expected_session {
                return Err("The shot analysis belongs to a different project session".into());
            }
            let pictures = original_pictures(workspace, &key)?;
            if analysis.pictures() != pictures {
                return Err(format!(
                    "The shot analysis measured {} pictures; the Original has {pictures}",
                    analysis.pictures()
                ));
            }
            let store = self.store.as_ref().ok_or("Open a project first")?;
            store.save_shot_analysis(&key, &analysis).map_err(display)?;
            Ok(Arc::new(workspace.with_shot_analysis(Arc::new(
                crate::project::OriginalShots {
                    key,
                    analysis: (*analysis).clone(),
                },
            ))))
        })();
        let error = match result {
            Ok(workspace) => {
                self.workspace = Some(workspace);
                None
            }
            Err(error) => Some(error),
        };
        self.shot_save = Some(crate::project::TranscriptSave {
            session: expected_session,
            attempt,
            error,
        });
    }
}

/// Shots are saved only for the session's ready Original, its qualified
/// picture stream and the current signature, so the published analysis
/// always describes the pictures on screen. Returns the qualified picture
/// count.
fn original_pictures(workspace: &Workspace, key: &deadpan_store::ShotAnalysisKey) -> Result<usize> {
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
    let video = receipt
        .snapshot()
        .video()
        .ok_or("The Original has no qualified pictures")?;
    if receipt.original().content().to_string() != key.content
        || video.index().stream_index() != key.video_stream
        || key.signature_version != deadpan_analysis::SIGNATURE_VERSION
    {
        return Err("The analysis belongs to different media than the project's Original".into());
    }
    Ok(video.index().index().frames().len())
}
