//! Helpers for spawning tool subprocesses from the Tauri GUI.
//!
//! On Windows, spawning a console program from a GUI app without
//! `CREATE_NO_WINDOW` briefly shows a conhost window; if the user closes
//! that window the child dies without surfacing useful output. We also
//! redirect stdin from /dev/null so tools that probe stdin don't block.
//!
//! Call `harden_for_gui(&mut cmd)` on every `tokio::process::Command` before
//! `.spawn()` so the behavior is uniform across modules.

use std::process::Stdio;
use tokio::process::Command;

/// Windows flag — suppresses the conhost window and prevents the subprocess
/// from inheriting the (non-existent) GUI app's console.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn harden_for_gui(cmd: &mut Command) {
    cmd.stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
}
