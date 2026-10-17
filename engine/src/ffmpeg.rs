//! Locating and launching the bundled ffmpeg and ffprobe binaries.
//!
//! The desktop app ships both tools next to its own executable. During
//! development and tests they are found on the PATH instead. Setting
//! `FOOTAGE_SELECTS_FFMPEG_DIR` points at a specific folder.

use std::path::PathBuf;
use std::process::Command;

fn exe_name(tool: &str) -> String {
    if cfg!(windows) {
        format!("{tool}.exe")
    } else {
        tool.to_string()
    }
}

/// Full path to `ffmpeg` or `ffprobe`, preferring the copy bundled with the app.
pub fn tool_path(tool: &str) -> PathBuf {
    let name = exe_name(tool);
    if let Ok(dir) = std::env::var("FOOTAGE_SELECTS_FFMPEG_DIR") {
        let p = PathBuf::from(dir).join(&name);
        if p.is_file() {
            return p;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join(&name);
            if p.is_file() {
                return p;
            }
        }
    }
    PathBuf::from(name)
}

/// A `Command` for the tool that never opens a console window on Windows.
pub fn command(tool: &str) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(tool_path(tool));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Returns the version line of ffmpeg, or None if it cannot be started.
pub fn ffmpeg_version() -> Option<String> {
    let out = command("ffmpeg").arg("-version").output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines().next().map(|s| s.to_string())
}
