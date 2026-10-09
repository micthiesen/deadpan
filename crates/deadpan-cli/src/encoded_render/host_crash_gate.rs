//! Explicitly compiled qualification instrumentation. Ordinary builds contain
//! neither this adjacent configuration nor the pause. Only real intermediate worker
//! progress reaches this seam; it cannot manufacture a completed candidate.

use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use crate::render_worker::protocol::RenderIdentity;

const CONFIG_NAME: &str = "render-host-crash-gate.json";
const MAX_CONFIG: u64 = 4096;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Configuration {
    schema_version: u32,
    stage: String,
    identity: RenderIdentity,
}

impl Configuration {
    fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1
            || !["encoding", "verification"].contains(&self.stage.as_str())
            || self.identity.request_id.as_str() != format!("host-crash-{}", self.stage)
            || self.identity.attempt_id.as_str() != format!("host-crash-{}-initial", self.stage)
        {
            return Err(
                "qualification configuration must bind an exact initial crash attempt".into(),
            );
        }
        Ok(())
    }

    fn matches(&self, stage: &str, identity: &RenderIdentity) -> bool {
        self.stage == stage && self.identity == *identity
    }
}

struct Gate {
    directory: PathBuf,
    configuration: Configuration,
}

fn load_configuration(path: &Path) -> Result<Option<Configuration>, String> {
    let descriptor = match rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::CLOEXEC
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::NOFOLLOW,
        rustix::fs::Mode::empty(),
    ) {
        Ok(descriptor) => descriptor,
        Err(rustix::io::Errno::NOENT) => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let file = File::from(descriptor);
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_CONFIG {
        return Err("invalid adjacent qualification configuration extent".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_CONFIG + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if u64::try_from(bytes.len()).map_err(|error| error.to_string())? > MAX_CONFIG {
        return Err("qualification configuration grew past its bound".into());
    }
    let configuration: Configuration =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    configuration.validate()?;
    Ok(Some(configuration))
}

fn load_gate() -> Result<Option<Gate>, String> {
    let executable = std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .map_err(|error| error.to_string())?;
    let directory = executable
        .parent()
        .ok_or("qualification executable has no parent")?;
    let Some(configuration) = load_configuration(&directory.join(CONFIG_NAME))? else {
        return Ok(None);
    };
    let scratch = std::fs::canonicalize("/tmp").map_err(|error| error.to_string())?;
    if !directory.starts_with(scratch)
        || !executable
            .file_name()
            .is_some_and(|name| name == "qualify_render_host_crash")
    {
        return Err("qualification gate requires its own canonical scratch executable".into());
    }
    Ok(Some(Gate {
        directory: directory.to_owned(),
        configuration,
    }))
}

pub(super) fn after_progress(
    stage: &str,
    identity: &RenderIdentity,
    completed: u64,
    total: u64,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<(), String> {
    if completed == 0 || completed >= total {
        return Ok(());
    }
    static GATE: OnceLock<Result<Option<Gate>, String>> = OnceLock::new();
    let Some(gate) = GATE.get_or_init(load_gate).as_ref().map_err(Clone::clone)? else {
        return Ok(());
    };
    if !gate.configuration.matches(stage, identity) {
        return Ok(());
    }
    let directory = &gate.directory;
    let result = (|| -> std::io::Result<()> {
        let mut witness = tempfile::NamedTempFile::new_in(directory)?;
        serde_json::to_writer(
            &mut witness,
            &serde_json::json!({"schema_version": 1, "stage": stage,
                "pid": std::process::id(), "identity": identity,
                "completed": completed, "total": total,
                "progress_flushed": true, "maximum_pause_seconds": 60}),
        )?;
        witness.flush()?;
        witness.as_file().sync_all()?;
        witness.persist_noclobber(directory.join("gate-ready.json"))?;
        File::open(directory)?.sync_all()
    })();
    result.map_err(|e| format!("qualification progress witness: {e}"))?;
    let gate_deadline = deadline.min(Instant::now() + Duration::from_secs(60));
    loop {
        // The ordinary control reader remains live. SIGKILL of the host closes
        // its descriptor and sets this same cancellation flag through EOF.
        // Never resume the encode/inspection after a qualification pause.
        if cancelled.load(Ordering::Acquire) {
            return Err("render qualification gate cancelled by control termination".into());
        }
        let Some(left) = gate_deadline.checked_duration_since(Instant::now()) else {
            return Err("render qualification gate exceeded its monotonic deadline".into());
        };
        std::thread::park_timeout(left.min(Duration::from_millis(5)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadpan_jobs::{AttemptId, RequestId};

    fn configuration() -> serde_json::Value {
        serde_json::json!({"schema_version":1,"stage":"verification","identity":{
            "request_id":"host-crash-verification",
            "attempt_id":"host-crash-verification-initial"}})
    }

    #[test]
    fn gate_binds_initial_attempt_and_skips_retry_and_other_stage() {
        let config: Configuration = serde_json::from_value(configuration()).expect("config");
        config.validate().expect("initial attempt");
        assert!(config.matches("verification", &config.identity));
        assert!(!config.matches("encoding", &config.identity));
        let retry = RenderIdentity {
            request_id: config.identity.request_id.clone(),
            attempt_id: AttemptId::new("host-crash-verification-retry").expect("retry"),
        };
        assert!(!config.matches("verification", &retry));
        let unrelated = RenderIdentity {
            request_id: RequestId::new("another-job").expect("job"),
            attempt_id: config.identity.attempt_id.clone(),
        };
        assert!(!config.matches("verification", &unrelated));
    }

    #[test]
    fn adjacent_gate_configuration_rejects_unknown_fields_and_retry_binding() {
        let mut unknown = configuration();
        unknown["directory"] = serde_json::json!("/tmp/another-case");
        assert!(serde_json::from_value::<Configuration>(unknown).is_err());
        let mut retry = configuration();
        retry["identity"]["attempt_id"] = serde_json::json!("host-crash-verification-retry");
        assert!(
            serde_json::from_value::<Configuration>(retry)
                .expect("typed config")
                .validate()
                .is_err()
        );
    }

    #[test]
    fn adjacent_gate_configuration_is_optional_bounded_and_never_follows_symlinks() {
        let directory = tempfile::tempdir().expect("scratch");
        let path = directory.path().join(CONFIG_NAME);
        assert!(load_configuration(&path).expect("absent").is_none());
        std::fs::write(&path, serde_json::to_vec(&configuration()).expect("json"))
            .expect("write config");
        assert!(load_configuration(&path).expect("valid config").is_some());
        let link = directory.path().join("link.json");
        std::os::unix::fs::symlink(&path, &link).expect("link");
        assert!(load_configuration(&link).is_err());
        std::fs::write(
            &path,
            vec![b' '; usize::try_from(MAX_CONFIG + 1).expect("bound")],
        )
        .expect("oversized config");
        assert!(load_configuration(&path).is_err());
    }
}
