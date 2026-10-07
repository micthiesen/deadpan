//! Shared destination preparation and rename semantics for ephemeral and journaled hosts.

use std::{ffi::OsStr, io, path::Path};

use super::*;

pub(super) struct StageControl<'a> {
    pub cancelled: &'a AtomicBool,
    pub deadline: Instant,
    pub live: &'a dyn Fn() -> Result<(), PublicationDiagnostic>,
}
impl StageControl<'_> {
    pub fn check(&self) -> Result<(), PublicationDiagnostic> {
        control(self.cancelled, self.deadline)?;
        (self.live)()
    }
    fn rename_guard(&self) -> Result<(), filesystem::FsError> {
        (self.live)().map_err(|_| {
            filesystem::FsError::invalid("publication_revoked", "publication permit was revoked")
        })
    }
}

pub(super) struct Names {
    pub publication_id: String,
    pub report: String,
    pub movie_partial: String,
    pub report_partial: String,
}

pub(super) struct Preparation<'a> {
    pub package: &'a Path,
    pub destination: &'a Path,
    pub names: Names,
    pub recoverable: bool,
}

/// Both files are complete and checked, but neither rename has occurred.
/// The owned descriptors keep their locks through all subsequent stages.
pub(super) struct PreparedFiles {
    pub movie: filesystem::PartialFile,
    pub report: filesystem::PartialFile,
    pub receipt: PublicationReceipt,
    pub directory: Option<filesystem::DirectoryEvidence>,
    report_ready: bool,
}

impl PreparedFiles {
    pub fn prepare(
        candidate: &mut VerifiedCandidate,
        request: Preparation<'_>,
        controls: &StageControl<'_>,
        progress: &mut impl FnMut(PublicationStage),
        retained: &mut RetainedPublicationArtifacts,
    ) -> Result<Self, PublicationDiagnostic> {
        controls.check()?;
        let name = request.destination.file_name().ok_or_else(|| {
            PublicationDiagnostic::new("invalid_destination", "destination has no filename")
        })?;
        let movie_filename = name.to_str().ok_or_else(|| {
            PublicationDiagnostic::new("invalid_destination", "destination filename must be UTF-8")
        })?;
        if !request
            .destination
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("mp4"))
        {
            return Err(PublicationDiagnostic::new(
                "invalid_destination",
                "destination filename must end in .mp4",
            ));
        }
        let parent = request
            .destination
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let destination = filesystem::Destination::pin(parent, name).map_err(fs_error)?;
        let directory = if request.recoverable {
            Some(destination.evidence().map_err(fs_error)?)
        } else {
            None
        };
        progress(PublicationStage::CapturingProvenance);
        controls.check()?;
        let provenance = provenance::capture(
            request.package,
            candidate,
            controls.cancelled,
            controls.deadline,
        )
        .map_err(|error| PublicationDiagnostic::new(error.code(), error))?;
        controls.check()?;
        let contains_generated_pictures = provenance.has_generated();
        let report_destination = destination
            .for_name(OsStr::new(&request.names.report))
            .map_err(fs_error)?;
        let movie_bytes = candidate.report().movie_bytes;
        let movie_sha256 = candidate.report().movie_sha256.clone();
        let mut movie = if request.recoverable {
            destination.create_partial_named(OsStr::new(&request.names.movie_partial), movie_bytes)
        } else {
            destination.create_partial(movie_bytes)
        }
        .map_err(|error| {
            retained.partial_movie = error.partial_path().map(Path::to_path_buf);
            fs_error(error)
        })?;
        retained.partial_movie = Some(movie.path());
        progress(PublicationStage::CopyingDestination);
        let copied = candidate
            .copy_to(
                &mut GuardedWriter {
                    inner: movie
                        .writer(controls.cancelled, controls.deadline)
                        .map_err(fs_error)?,
                    controls,
                },
                controls.cancelled,
                controls.deadline,
            )
            .map_err(encoded_error)?;
        if copied != movie_bytes {
            return Err(PublicationDiagnostic::new(
                "destination_length_mismatch",
                "copied byte count differs from the verified movie",
            ));
        }
        controls.check()?;
        movie
            .seal(movie_bytes, controls.cancelled, controls.deadline)
            .map_err(fs_error)?;
        progress(PublicationStage::CheckingDestination);
        let actual = hash_guarded(
            movie
                .reader(controls.cancelled, controls.deadline)
                .map_err(fs_error)?,
            movie_bytes,
            controls,
        )?;
        if actual != movie_sha256 {
            return Err(PublicationDiagnostic::new(
                "destination_hash_mismatch",
                "destination readback differs from the verified private movie",
            ));
        }
        let report_wire = serialize_report(&LocalReport {
            schema_version: 1,
            application_version: env!("CARGO_PKG_VERSION"),
            publication_id: request.names.publication_id.clone(),
            movie_filename: movie_filename.into(),
            report_filename: request.names.report,
            scope: "verified_candidate_prepared_for_atomic_publication",
            destination_readback: ByteIdentity {
                bytes: movie_bytes,
                sha256: actual,
            },
            provenance,
        })?;
        controls.check()?;
        let report_bytes = u64::try_from(report_wire.len()).expect("bounded report length");
        let report_sha256 = digest(&report_wire);
        let mut report = if request.recoverable {
            report_destination
                .create_partial_named(OsStr::new(&request.names.report_partial), report_bytes)
        } else {
            report_destination.create_partial(report_bytes)
        }
        .map_err(|error| {
            retained.partial_report = error.partial_path().map(Path::to_path_buf);
            fs_error(error)
        })?;
        retained.partial_report = Some(report.path());
        progress(PublicationStage::WritingReport);
        GuardedWriter {
            inner: report
                .writer(controls.cancelled, controls.deadline)
                .map_err(fs_error)?,
            controls,
        }
        .write_all(&report_wire)
        .map_err(|error| io_error("report_write_failed", error))?;
        controls.check()?;
        report
            .seal(report_bytes, controls.cancelled, controls.deadline)
            .map_err(fs_error)?;
        if hash_guarded(
            report
                .reader(controls.cancelled, controls.deadline)
                .map_err(fs_error)?,
            report_bytes,
            controls,
        )? != report_sha256
        {
            return Err(PublicationDiagnostic::new(
                "report_hash_mismatch",
                "destination report readback differs from the captured provenance",
            ));
        }
        controls.check()?;
        Ok(Self {
            movie,
            report,
            receipt: PublicationReceipt {
                publication_id: request.names.publication_id,
                movie: destination.path(),
                report: report_destination.path(),
                movie_sha256,
                movie_bytes,
                report_sha256,
                report_bytes,
                contains_generated_pictures,
            },
            directory,
            report_ready: false,
        })
    }

    pub fn retained(&self) -> RetainedPublicationArtifacts {
        RetainedPublicationArtifacts {
            partial_movie: (!self.movie.is_published()).then(|| self.movie.path()),
            partial_report: (!self.report.is_published()).then(|| self.report.path()),
            published_report: self
                .report
                .is_published()
                .then(|| self.receipt.report.clone()),
        }
    }

    pub fn commit_report(
        &mut self,
        controls: &StageControl<'_>,
    ) -> Result<(), PublicationDiagnostic> {
        controls.check()?;
        self.report
            .commit_guarded(controls.cancelled, controls.deadline, || {
                controls.rename_guard()
            })
            .map_err(fs_error)?;
        controls.check()?;
        confirm_published_bytes(
            &mut self.report,
            &self.receipt.report_sha256,
            self.receipt.report_bytes,
            controls.cancelled,
            controls.deadline,
        )?;
        controls.check()?;
        self.report_ready = true;
        Ok(())
    }

    pub fn commit_movie(
        &mut self,
        controls: &StageControl<'_>,
    ) -> Result<PublicationOutcome, PublicationDiagnostic> {
        controls.check()?;
        if !self.report_ready {
            return Err(PublicationDiagnostic::new(
                "publication_phase",
                "the report has not completed publication and readback",
            ));
        }
        if let Err(error) = self.report.confirm_published() {
            if error.code() != "destination_changed" {
                return Err(fs_error(error));
            }
            confirm_published_bytes(
                &mut self.report,
                &self.receipt.report_sha256,
                self.receipt.report_bytes,
                controls.cancelled,
                controls.deadline,
            )?;
        }
        let finish_deadline = Instant::now() + POST_COMMIT_READBACK_BUDGET;
        match self
            .movie
            .commit_guarded(controls.cancelled, controls.deadline, || {
                controls.rename_guard()
            }) {
            Ok(()) => {
                match finish_committed(
                    &mut self.movie,
                    &mut self.report,
                    &self.receipt,
                    finish_deadline,
                ) {
                    Ok(()) => Ok(PublicationOutcome::Published(self.receipt.clone())),
                    Err(diagnostic) => Ok(PublicationOutcome::PublishedUnconfirmed {
                        receipt: self.receipt.clone(),
                        diagnostic,
                    }),
                }
            }
            Err(error) if error.published() => Ok(PublicationOutcome::PublishedUnconfirmed {
                receipt: self.receipt.clone(),
                diagnostic: fs_error(error),
            }),
            Err(error) => Err(fs_error(error)),
        }
    }
}

struct GuardedWriter<'a, 'b, W> {
    inner: W,
    controls: &'a StageControl<'b>,
}
impl<W: Write> Write for GuardedWriter<'_, '_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.controls.check().map_err(io::Error::other)?;
        self.inner.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.controls.check().map_err(io::Error::other)?;
        self.inner.flush()
    }
}

pub(super) fn hash_guarded(
    reader: impl Read,
    bytes: u64,
    controls: &StageControl<'_>,
) -> Result<Sha256, PublicationDiagnostic> {
    struct GuardedReader<'a, 'b, R> {
        inner: R,
        controls: &'a StageControl<'b>,
    }
    impl<R: Read> Read for GuardedReader<'_, '_, R> {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            self.controls.check().map_err(io::Error::other)?;
            self.inner.read(bytes)
        }
    }
    hash_reader(
        GuardedReader {
            inner: reader,
            controls,
        },
        bytes,
        controls.cancelled,
        controls.deadline,
    )
}
