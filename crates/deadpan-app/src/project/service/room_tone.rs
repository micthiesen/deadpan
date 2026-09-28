//! Prepare an identity-bound source range without changing authored state.

use deadpan_core::SourceAudio;
use deadpan_media::source_import_timing::derive_source_audio_moment;

use super::*;
use crate::project::RoomToneSelection;

impl Service {
    pub(super) fn prepare_room_tone(
        &mut self,
        expected_session: u64,
        expected_revision: RevisionId,
        ticket: u64,
        selection: RoomToneSelection,
    ) -> Result<()> {
        self.check_context(expected_session, &expected_revision)?;
        let workspace = self.workspace.as_ref().ok_or("Open a project first")?;
        let (asset, qualification) = match &selection {
            RoomToneSelection::Original {
                asset,
                qualification,
                ..
            } => (asset, qualification),
            RoomToneSelection::Exact {
                source,
                qualification,
            } => (&source.asset, qualification),
        };
        let registered = workspace
            .sources
            .get(asset)
            .ok_or("Room-tone source is no longer registered in this project")?;
        if registered.receipt.id() != qualification {
            return Err("Room-tone source qualification changed; select the source again".into());
        }
        // Recheck the revision-bound store admission instead of trusting a stale
        // alias or a descriptor copied from another source/session. This reads
        // retained evidence only; playback owns fresh verified byte snapshots.
        let receipt = self
            .store
            .as_ref()
            .ok_or("Open a project first")?
            .registered_source(&expected_revision, asset)
            .map_err(display)?;
        if &receipt != registered.receipt.as_ref() {
            return Err("Room-tone source differs from its registered qualification".into());
        }
        let source = match selection {
            RoomToneSelection::Original {
                asset, ordinals, ..
            } => {
                let video = receipt
                    .snapshot()
                    .video()
                    .ok_or("Room-tone moment requires a qualified Original picture range")?;
                let audio = receipt
                    .snapshot()
                    .audio()
                    .ok_or("This Original has no qualified audio for room tone")?;
                let span =
                    derive_source_audio_moment(video.index(), audio, ordinals).map_err(display)?;
                SourceAudio { asset, span }
            }
            RoomToneSelection::Exact { source, .. } => source,
        };
        let audition = Arc::new(deadpan_playback::AudioRange::new(
            workspace.document.presentation_basis().frame_rate,
            source.asset.clone(),
            Arc::new(receipt),
            source.span,
        )?);
        self.room_tone = Some(PreparedRoomTone {
            ticket,
            session: expected_session,
            revision: expected_revision,
            source,
            audition,
        });
        self.message = Some("Room-tone source range prepared. The project is unchanged.".into());
        Ok(())
    }
}
