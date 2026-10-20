//! Checks that only physical Apple hardware can pass.
//!
//! GitHub's macOS runners are virtual machines. They report no CPU family,
//! which the encoder runtime identity bound by every Render requires, and
//! paravirtualize VideoToolbox and Metal, whose pictures then differ from
//! physical hardware. Such checks skip on a hypervisor guest unless
//! `DEADPAN_REQUIRE_PHYSICAL_MAC=1` (set by `cargo xtask check` and `gate`)
//! makes the guest a failure. Callers keep every other assertion running.
//! Test files in other crates include this file by path.

use std::sync::OnceLock;

/// Whether this machine must skip `check`, which needs a physical Mac.
pub fn skip_without_physical_mac(check: &str) -> bool {
    static GUEST: OnceLock<bool> = OnceLock::new();
    let guest = *GUEST.get_or_init(|| {
        std::process::Command::new("/usr/sbin/sysctl")
            .args(["-n", "kern.hv_vmm_present"])
            .output()
            .is_ok_and(|output| output.status.success() && output.stdout.trim_ascii() == b"1")
    });
    if !guest {
        return false;
    }
    assert!(
        std::env::var_os("DEADPAN_REQUIRE_PHYSICAL_MAC").is_none_or(|value| value != "1"),
        "DEADPAN_REQUIRE_PHYSICAL_MAC=1: {check} needs a physical Mac, not a virtual machine"
    );
    eprintln!("skipped: {check} needs a physical Mac, not a virtual machine");
    true
}
