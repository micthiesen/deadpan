use std::{
    cell::Cell,
    fs::{self, File},
    io::{Read, Write},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
const OUTPUT_LIMIT: u64 = 4 * 1024 * 1024;

pub struct Run {
    pub root: PathBuf,
    sequence: Cell<u32>,
    pub cli_hash: Value,
    pub test_hash: Value,
}

pub struct Output {
    pub status: ExitStatus,
    pub stdout: PathBuf,
    pub stderr: PathBuf,
}

impl Run {
    pub fn new() -> Result<Self> {
        let base = Path::new("/tmp/deadpan-relink-volumes-20261008");
        fs::create_dir_all(base)?;
        let root = tempfile::Builder::new()
            .prefix("run-")
            .tempdir_in(base)?
            .keep()
            .canonicalize()?;
        Ok(Self {
            root,
            sequence: Cell::new(0),
            cli_hash: hash(Path::new(env!("CARGO_BIN_EXE_deadpan-cli")))?,
            test_hash: hash(&std::env::current_exe()?)?,
        })
    }

    pub fn save(&self, name: &str, value: &Value) -> Result {
        let bytes = serde_json::to_vec_pretty(value)?;
        require(
            u64::try_from(bytes.len())? <= OUTPUT_LIMIT,
            "evidence JSON exceeds bound",
        )?;
        let mut file = File::create(self.root.join(name))?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        Ok(())
    }

    /// File-backed output, a finite deadline, and sole reaping ownership for
    /// every CLI, hdiutil and plist conversion process launched by this fixture.
    pub fn command(&self, label: &str, command: &mut Command) -> Result<Output> {
        let sequence = self
            .sequence
            .get()
            .checked_add(1)
            .ok_or("command counter overflow")?;
        self.sequence.set(sequence);
        let stem = format!("{sequence:03}-{label}");
        let stdout_path = self.root.join(format!("{stem}.stdout"));
        let stderr_path = self.root.join(format!("{stem}.stderr"));
        let stdout = File::create(&stdout_path)?;
        let stderr = File::create(&stderr_path)?;
        let mut receipt = json!({"program":command.get_program().to_string_lossy(),
            "arguments":command.get_args().map(|arg| arg.to_string_lossy()).collect::<Vec<_>>(),
            "stdout":stdout_path,"stderr":stderr_path,"maximum_seconds":60});
        self.save(&format!("{stem}.json"), &receipt)?;
        command
            .stdin(Stdio::null())
            .stdout(stdout.try_clone()?)
            .stderr(stderr.try_clone()?)
            .process_group(0);
        let mut child = OwnedChild {
            child: deadpan_native_process::spawn(command)?,
            reap_attempted: false,
        };
        let started = Instant::now();
        let deadline = started + Duration::from_secs(60);
        let work = (|| -> Result {
            loop {
                require(
                    stdout.metadata()?.len() <= OUTPUT_LIMIT
                        && stderr.metadata()?.len() <= OUTPUT_LIMIT,
                    "command output exceeds fixture bound",
                )?;
                if deadpan_native_process::exited_leader_has_no_other_members(&child.child)? {
                    return Ok(());
                }
                require(
                    Instant::now() < deadline,
                    "fixture command deadline exceeded",
                )?;
                std::thread::park_timeout(Duration::from_millis(10));
            }
        })();
        let cleanup = child.finish();
        receipt["seconds"] = json!(started.elapsed().as_secs_f64());
        receipt["stdout_hash"] = hash(&stdout_path)?;
        receipt["stderr_hash"] = hash(&stderr_path)?;
        if let Err(error) = &work {
            receipt["work_error"] = json!(error.to_string());
        }
        match &cleanup {
            Ok(status) => {
                receipt["exit"] = json!(status.to_string());
                receipt["success"] = json!(status.success());
            }
            Err(error) => receipt["cleanup_error"] = json!(error.to_string()),
        }
        self.save(&format!("{stem}.json"), &receipt)?;
        let status = match (work, cleanup) {
            (Ok(()), Ok(status)) => status,
            (Err(work), Err(cleanup)) => {
                return Err(format!("{label}: {work}; cleanup: {cleanup}").into());
            }
            (Err(error), _) | (_, Err(error)) => return Err(error),
        };
        Ok(Output {
            status,
            stdout: stdout_path,
            stderr: stderr_path,
        })
    }

    pub fn success(&self, label: &str, command: &mut Command) -> Result<PathBuf> {
        let output = self.command(label, command)?;
        require(
            output.status.success(),
            &format!(
                "{label} failed: {}; see {}",
                output.status,
                output.stderr.display()
            ),
        )?;
        Ok(output.stdout)
    }

    pub fn cli(&self, label: &str, arguments: &[&str]) -> Result<Output> {
        self.command(
            label,
            Command::new(env!("CARGO_BIN_EXE_deadpan-cli")).args(arguments),
        )
    }

    pub fn plist_json(&self, label: &str, path: &Path) -> Result<Value> {
        let output = self.success(
            label,
            Command::new("plutil")
                .args(["-convert", "json", "-o", "-"])
                .arg(path),
        )?;
        read_json(&output)
    }
}

struct OwnedChild {
    child: Child,
    reap_attempted: bool,
}
impl OwnedChild {
    fn finish(&mut self) -> Result<ExitStatus> {
        require(!self.reap_attempted, "child reap already attempted")?;
        let deadline = Instant::now() + Duration::from_secs(5);
        let group_error =
            deadpan_native_process::terminate_owned_group(&self.child, deadline).err();
        if let Some(group_error) = &group_error {
            deadpan_native_process::terminate_owned_leader(
                &self.child,
                Instant::now() + Duration::from_secs(2),
            )
            .map_err(|leader| format!("group cleanup: {group_error}; leader fallback: {leader}"))?;
        }
        self.reap_attempted = true;
        let status = self.child.wait()?;
        if let Some(error) = group_error {
            return Err(format!("owned group cleanup failed: {error}").into());
        }
        Ok(status)
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.reap_attempted
            && let Err(error) = self.finish()
        {
            eprintln!("relink fixture process cleanup remains unconfirmed: {error}");
        }
    }
}

pub fn hash(path: &Path) -> Result<Value> {
    use std::os::unix::fs::MetadataExt;
    let mut file = File::open(path)?;
    let before = file.metadata()?;
    require(
        before.is_file() && before.len() <= 1024 * 1024 * 1024,
        "hash input exceeds fixture bound",
    )?;
    let mut hasher = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = [0; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        bytes = bytes
            .checked_add(u64::try_from(read)?)
            .ok_or("hash size overflow")?;
        require(bytes <= before.len(), "hash input grew")?;
        hasher.update(&buffer[..read]);
    }
    let after = file.metadata()?;
    require(
        bytes == before.len()
            && before.len() == after.len()
            && before.mtime() == after.mtime()
            && before.mtime_nsec() == after.mtime_nsec()
            && before.ctime() == after.ctime()
            && before.ctime_nsec() == after.ctime_nsec(),
        "hash input changed",
    )?;
    Ok(json!({"path":path,"bytes":bytes,"sha256":hex(hasher.finalize())}))
}
pub fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
pub fn read_json(path: &Path) -> Result<Value> {
    let file = File::open(path)?;
    require(
        file.metadata()?.len() <= OUTPUT_LIMIT,
        "JSON exceeds fixture bound",
    )?;
    let mut bytes = Vec::new();
    file.take(OUTPUT_LIMIT + 1).read_to_end(&mut bytes)?;
    require(
        u64::try_from(bytes.len())? <= OUTPUT_LIMIT,
        "JSON grew past fixture bound",
    )?;
    Ok(serde_json::from_slice(&bytes)?)
}
pub fn text(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| "fixture path is not UTF-8".into())
}
pub fn require(value: bool, message: &str) -> Result {
    if value { Ok(()) } else { Err(message.into()) }
}
