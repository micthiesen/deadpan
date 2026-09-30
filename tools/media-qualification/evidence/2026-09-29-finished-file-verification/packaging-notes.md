# Evidence packaging correction

The first packer attempt failed its marker count assertion after writing and
auditing the archive. It included the six canonical input marker observations
alongside the 18 encoded-file reader observations. All 24 had zero error. The
corrected summary explicitly selects manual FFmpeg, ordinary FFmpeg and
AVFoundation. No media result changed and no native check was rerun.
