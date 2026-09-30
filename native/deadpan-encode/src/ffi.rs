//! Narrow synchronous FFI. No input slice or control pointer survives a call.
//! The native context borrows the owned file descriptor until close; the Rc
//! marker conservatively keeps the complete session on its creation thread.

use std::ffi::{c_char, c_int, c_void};
use std::fs::File;
use std::marker::PhantomData;
use std::os::fd::AsRawFd;
use std::ptr::NonNull;
use std::rc::Rc;

use rustix::fs::{OFlags, fcntl_getfl, fstat};

use crate::{
    AUDIO_FRAME_SAMPLES, AUDIO_SAMPLE_RATE, Control, EncodeContract, EncodeError, EncodeLimits,
    EncodeReport, EncoderInfo, EncoderMode,
};

#[repr(C)]
struct Config {
    abi_version: u32,
    width: u32,
    height: u32,
    fps_num: u32,
    fps_den: u32,
    video_frames: u64,
    audio_samples: u64,
    video_bitrate: u64,
    gop_frames: u32,
    b_frames: u32,
    mode: u32,
    maximum_output_bytes: u64,
    maximum_packets: u64,
    maximum_packet_bytes: u64,
}

#[repr(C)]
struct NativeControl {
    opaque: *mut c_void,
    cancelled: extern "C" fn(*mut c_void) -> c_int,
    timeout_millis: u64,
}

#[repr(C)]
#[derive(Default)]
struct Info {
    abi_version: u32,
    avcodec_version: u32,
    avformat_version: u32,
    avutil_version: u32,
    movie_timescale: u32,
    video_time_base_num: u32,
    video_time_base_den: u32,
    audio_time_base_num: u32,
    audio_time_base_den: u32,
    audio_frame_size: u32,
    video_profile: i32,
    video_has_b_frames: i32,
    video_max_b_frames: i32,
    video_gop_size: i32,
    audio_profile: i32,
    audio_initial_padding: i32,
    audio_trailing_padding: i32,
    requested_mode: u32,
    video_bitrate: u64,
    audio_bitrate: u64,
    maximum_moov_bytes: u64,
}

#[repr(C)]
#[derive(Default)]
struct Report {
    info: Info,
    video_frames: u64,
    audio_samples: u64,
    video_packets: u64,
    audio_packets: u64,
    output_bytes: u64,
    packet_bytes: u64,
    video_duration_from_contract_packets: u64,
    faststart_read_opens: u32,
    faststart_read_closes: u32,
    video_eof: u32,
    audio_eof: u32,
}

#[repr(C)]
struct Error {
    code: [c_char; 48],
    message: [c_char; 256],
}

impl Default for Error {
    fn default() -> Self {
        Self {
            code: [0; 48],
            message: [0; 256],
        }
    }
}
impl Error {
    fn into_error(self, control: &Control<'_>) -> EncodeError {
        if let Err(error) = control.check() {
            return error;
        }
        EncodeError::Native {
            code: string(&self.code),
            message: string(&self.message),
        }
    }
}
fn string(bytes: &[c_char]) -> String {
    let bytes: Vec<u8> = bytes
        .iter()
        .copied()
        .take_while(|&byte| byte != 0)
        .map(|byte| byte.to_ne_bytes()[0])
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

unsafe extern "C" {
    fn dp_encode_open(
        fd: c_int,
        config: *const Config,
        control: *const NativeControl,
        session: *mut *mut c_void,
        info: *mut Info,
        error: *mut Error,
    ) -> c_int;
    fn dp_encode_push_picture(
        session: *mut c_void,
        ordinal: u64,
        pts: i64,
        duration: i64,
        bytes: *const u8,
        length: u64,
        control: *const NativeControl,
        error: *mut Error,
    ) -> c_int;
    fn dp_encode_push_audio(
        session: *mut c_void,
        first_sample: u64,
        left: *const f32,
        right: *const f32,
        count: u32,
        control: *const NativeControl,
        error: *mut Error,
    ) -> c_int;
    fn dp_encode_finish(
        session: *mut c_void,
        control: *const NativeControl,
        report: *mut Report,
        error: *mut Error,
    ) -> c_int;
    fn dp_encode_close(session: *mut c_void);
}

extern "C" fn interrupted(opaque: *mut c_void) -> c_int {
    // SAFETY: each exported call receives the address of its live borrowed
    // Control. C invokes this only synchronously and clears it before return.
    let control = unsafe { &*opaque.cast::<Control<'_>>() };
    c_int::from(control.check().is_err())
}
fn native_control(control: &Control<'_>) -> Result<NativeControl, EncodeError> {
    Ok(NativeControl {
        opaque: std::ptr::from_ref(control).cast_mut().cast(),
        cancelled: interrupted,
        timeout_millis: control.remaining_millis()?,
    })
}

pub(super) struct Encoder {
    pointer: Option<NonNull<c_void>>,
    file: Option<File>,
    _same_thread: PhantomData<Rc<()>>,
}
impl Drop for Encoder {
    fn drop(&mut self) {
        if let Some(pointer) = self.pointer.take() {
            // SAFETY: this is the sole native context owner; C does not close
            // the borrowed descriptor, which remains live in self.file.
            unsafe { dp_encode_close(pointer.as_ptr()) };
        }
    }
}

impl Encoder {
    pub(super) fn open(
        file: File,
        contract: &EncodeContract,
        limits: EncodeLimits,
        control: &Control<'_>,
    ) -> Result<(Self, EncoderInfo), EncodeError> {
        limits.validate_for(contract)?;
        let ctl = native_control(control)?;
        let metadata = file.metadata()?;
        let flags = fcntl_getfl(&file).map_err(std::io::Error::from)?;
        let stat = fstat(&file).map_err(std::io::Error::from)?;
        if !metadata.is_file()
            || metadata.len() != 0
            || stat.st_nlink != 1
            || stat.st_uid != rustix::process::geteuid().as_raw()
            || stat.st_mode & 0o077 != 0
            || flags & OFlags::ACCMODE != OFlags::RDWR
            || flags.contains(OFlags::APPEND)
        {
            return Err(EncodeError::Configuration(
                "output must be an owned private empty singleton read/write regular file without append mode",
            ));
        }
        control.check()?;
        let [width, height] = contract.raster();
        let [fps_num, fps_den] = contract.frame_rate();
        let config = Config {
            abi_version: 1,
            width,
            height,
            fps_num,
            fps_den,
            video_frames: contract.video_frames(),
            audio_samples: contract.audio_samples(),
            video_bitrate: contract.policy().video_bitrate,
            gop_frames: contract.policy().gop_frames,
            b_frames: contract.policy().b_frames,
            mode: mode(contract.mode()),
            maximum_output_bytes: limits.maximum_output_bytes,
            maximum_packets: limits.maximum_packets,
            maximum_packet_bytes: limits.maximum_packet_bytes,
        };
        let mut pointer = std::ptr::null_mut();
        let mut info = Info::default();
        let mut error = Error::default();
        // SAFETY: Config is validated and matches encoder.h. Descriptor and
        // writable result structs remain live for this synchronous call. C
        // frees failed allocations and only returns an owned context on success.
        let result = unsafe {
            dp_encode_open(
                file.as_raw_fd(),
                &config,
                &ctl,
                &mut pointer,
                &mut info,
                &mut error,
            )
        };
        if result != 1 {
            return Err(error.into_error(control));
        }
        let inner = Self {
            pointer: Some(
                NonNull::new(pointer)
                    .ok_or(EncodeError::Evidence("successful open returned no context"))?,
            ),
            file: Some(file),
            _same_thread: PhantomData,
        };
        let info = info.admit(contract)?;
        control.check()?;
        Ok((inner, info))
    }

    pub(super) fn picture(
        &mut self,
        ordinal: u64,
        pts: i64,
        duration: i64,
        bytes: &[u8],
        control: &Control<'_>,
    ) -> Result<(), EncodeError> {
        let ctl = native_control(control)?;
        let mut error = Error::default();
        let length = u64::try_from(bytes.len())
            .map_err(|_| EncodeError::Input("picture length exceeds native field"))?;
        // SAFETY: public Progress validated exact frame clocks/length/codes.
        // This exclusive context and borrowed slice/control outlive the call;
        // C copies planes into its own bounded frame and retains no pointers.
        let result = unsafe {
            dp_encode_push_picture(
                self.pointer().as_ptr(),
                ordinal,
                pts,
                duration,
                bytes.as_ptr(),
                length,
                &ctl,
                &mut error,
            )
        };
        if result != 1 {
            return Err(error.into_error(control));
        }
        Ok(())
    }

    pub(super) fn audio(
        &mut self,
        first: u64,
        left: &[f32],
        right: &[f32],
        control: &Control<'_>,
    ) -> Result<(), EncodeError> {
        let ctl = native_control(control)?;
        let mut error = Error::default();
        let count = u32::try_from(left.len())
            .map_err(|_| EncodeError::Input("audio block exceeds native field"))?;
        // SAFETY: public Progress validated matching finite planar slices and
        // exact contiguous clocks. C copies at most 1024 samples per channel
        // and retains neither input nor control pointers after returning.
        let result = unsafe {
            dp_encode_push_audio(
                self.pointer().as_ptr(),
                first,
                left.as_ptr(),
                right.as_ptr(),
                count,
                &ctl,
                &mut error,
            )
        };
        if result != 1 {
            return Err(error.into_error(control));
        }
        Ok(())
    }

    pub(super) fn finish(
        &mut self,
        contract: &EncodeContract,
        limits: EncodeLimits,
        control: &Control<'_>,
    ) -> Result<EncodeReport, EncodeError> {
        let ctl = native_control(control)?;
        let mut report = Report::default();
        let mut error = Error::default();
        // SAFETY: all input was accepted exactly once. Results/control remain
        // borrowed only during this exclusive drain, trailer and relocation call.
        let result =
            unsafe { dp_encode_finish(self.pointer().as_ptr(), &ctl, &mut report, &mut error) };
        if result != 1 {
            return Err(error.into_error(control));
        }
        if report.video_frames != contract.video_frames()
            || report.audio_samples != contract.audio_samples()
            || report.video_packets != contract.video_frames()
            || report.audio_packets
                != contract
                    .audio_samples()
                    .div_ceil(u64::from(AUDIO_FRAME_SAMPLES))
                    + 1
            || report
                .video_packets
                .checked_add(report.audio_packets)
                .is_none_or(|value| value > limits.maximum_packets)
            || report.output_bytes == 0
            || report.output_bytes > limits.maximum_output_bytes
            || report.packet_bytes == 0
            || report.packet_bytes > report.output_bytes
            || report.video_duration_from_contract_packets > report.video_packets
            || report.faststart_read_opens != 1
            || report.faststart_read_closes != 1
            || report.video_eof != 1
            || report.audio_eof != 1
        {
            return Err(EncodeError::Evidence(
                "finished counts, EOF or same-descriptor fast-start evidence differs",
            ));
        }
        Ok(EncodeReport {
            info: report.info.admit(contract)?,
            video_frames: report.video_frames,
            audio_samples: report.audio_samples,
            video_packets: report.video_packets,
            audio_packets: report.audio_packets,
            output_bytes: report.output_bytes,
            packet_bytes: report.packet_bytes,
            video_duration_from_contract_packets: report.video_duration_from_contract_packets,
            faststart_read_opens: report.faststart_read_opens,
            faststart_read_closes: report.faststart_read_closes,
            video_eof: true,
            audio_eof: true,
        })
    }

    fn pointer(&self) -> NonNull<c_void> {
        self.pointer.expect("live Encoder owns its native context")
    }
    pub(super) fn into_file(mut self) -> File {
        if let Some(pointer) = self.pointer.take() {
            // SAFETY: close once before returning ownership of the borrowed fd.
            unsafe { dp_encode_close(pointer.as_ptr()) };
        }
        self.file
            .take()
            .expect("live Encoder owns its output descriptor")
    }
}

fn mode(mode: EncoderMode) -> u32 {
    match mode {
        EncoderMode::Hardware => 0,
        EncoderMode::Software => 1,
    }
}
impl Info {
    fn admit(self, contract: &EncodeContract) -> Result<EncoderInfo, EncodeError> {
        if self.abi_version != 1
            || self.avcodec_version == 0
            || self.avformat_version == 0
            || self.avutil_version == 0
            || self.movie_timescale != contract.policy().movie_timescale
            || self.video_time_base_num != 1
            || self.video_time_base_den != contract.frame_rate()[0]
            || self.audio_time_base_num != 1
            || self.audio_time_base_den != AUDIO_SAMPLE_RATE
            || self.audio_frame_size != AUDIO_FRAME_SAMPLES
            || self.requested_mode != mode(contract.mode())
            || self.video_profile != 100
            || self.audio_profile != 1
            || self.video_has_b_frames < 0
            || self.video_max_b_frames < 0
            || self.video_gop_size <= 0
            || self.audio_initial_padding < 0
            || self.audio_trailing_padding < 0
            || self.video_bitrate == 0
            || self.audio_bitrate != contract.policy().audio_bitrate
            || self.maximum_moov_bytes == 0
        {
            return Err(EncodeError::Evidence(
                "native codec configuration differs from required SDR policy",
            ));
        }
        Ok(EncoderInfo {
            abi_version: self.abi_version,
            avcodec_version: self.avcodec_version,
            avformat_version: self.avformat_version,
            avutil_version: self.avutil_version,
            movie_timescale: self.movie_timescale,
            video_time_base_num: self.video_time_base_num,
            video_time_base_den: self.video_time_base_den,
            audio_time_base_num: self.audio_time_base_num,
            audio_time_base_den: self.audio_time_base_den,
            audio_frame_size: self.audio_frame_size,
            video_profile: self.video_profile,
            video_has_b_frames: self.video_has_b_frames,
            video_max_b_frames: self.video_max_b_frames,
            video_gop_size: self.video_gop_size,
            audio_profile: self.audio_profile,
            audio_initial_padding: self.audio_initial_padding,
            audio_trailing_padding: self.audio_trailing_padding,
            requested_mode: contract.mode(),
            video_bitrate: self.video_bitrate,
            audio_bitrate: self.audio_bitrate,
            maximum_moov_bytes: self.maximum_moov_bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BFramePolicy;
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    use std::sync::atomic::AtomicBool;
    use std::time::{Duration, Instant};

    #[test]
    fn descriptor_admission_rejects_nonempty_readonly_append_and_multiple_links_before_native_allocation()
     {
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().join("candidate.partial");
        let make_contract = || {
            EncodeContract::new(
                [2, 2],
                [60, 1],
                1,
                800,
                EncoderMode::Hardware,
                BFramePolicy::TargetTwo,
            )
            .unwrap()
        };
        let cancelled = AtomicBool::new(false);
        let control = Control {
            cancelled: &cancelled,
            deadline: Instant::now() + Duration::from_secs(5),
        };
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .unwrap();
        assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
        file.write_all(b"preserve").unwrap();
        assert!(matches!(
            Encoder::open(file, &make_contract(), EncodeLimits::default(), &control),
            Err(EncodeError::Configuration(_))
        ));
        assert_eq!(std::fs::read(&path).unwrap(), b"preserve");
        let empty = scratch.path().join("empty");
        let empty_file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&empty)
            .unwrap();
        assert_eq!(
            empty_file.metadata().unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(fstat(&empty_file).unwrap().st_nlink, 1);
        for file in [
            File::open(&empty).unwrap(),
            OpenOptions::new()
                .read(true)
                .append(true)
                .open(&empty)
                .unwrap(),
        ] {
            assert!(matches!(
                Encoder::open(file, &make_contract(), EncodeLimits::default(), &control),
                Err(EncodeError::Configuration(_))
            ));
        }
        std::fs::hard_link(&empty, scratch.path().join("another-link")).unwrap();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&empty)
            .unwrap();
        assert_eq!(fstat(&file).unwrap().st_nlink, 2);
        assert!(matches!(
            Encoder::open(file, &make_contract(), EncodeLimits::default(), &control),
            Err(EncodeError::Configuration(_))
        ));
    }
}
