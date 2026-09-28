// Cooperative subprocess creation on platforms without atomic pipe CLOEXEC.

use std::io;
use std::process::{Child, Command};

/// Spawn a configured command without overlapping another participating launch.
///
/// macOS creates standard-library pipes before setting their close-on-exec flags.
/// Serialize the complete spawn call, including that setup, so another launch
/// using this boundary cannot inherit a transient pipe endpoint. Other platforms
/// use their ordinary spawn path.
///
/// This is cooperative within this process. Direct standard-library or foreign
/// process launches do not participate. It neither changes the command's streams,
/// environment, or process group nor contains children that retain inherited I/O.
/// Do not attach `CommandExt::pre_exec` callbacks: they run in the forked child
/// while the launch lock is held and must not reenter this boundary or other
/// arbitrary Rust code. Configure the command with its ordinary builder methods.
/// The lock covers creation only; no waits or pipe pumping run under it.
pub fn spawn(command: &mut Command) -> io::Result<Child> {
    #[cfg(target_os = "macos")]
    let _launch = SPAWN_LOCK
        .lock()
        .map_err(|_| io::Error::other("process launch lock poisoned"))?;
    command.spawn()
}

#[cfg(target_os = "macos")]
static SPAWN_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
