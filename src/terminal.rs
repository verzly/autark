use std::ffi::OsString;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};

use crate::tools;

/// The shell needs its own console window, even though Autark itself has none.
const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

/// Autark's own tools come first, so the shell has mise, git and every installed tool by name.
fn autark_path(root: &Path) -> OsString {
    let mut dirs = vec![
        root.join("bin"),
        root.join("runtime").join("mise-data").join("shims"),
    ];
    dirs.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    std::env::join_paths(dirs).unwrap_or_default()
}

/// Opens the machine's terminal in the Autark root with Autark's PATH and a short banner.
pub fn open(root: &Path) -> Result<()> {
    // The console host is whatever Windows uses by default, so this lands in Windows Terminal where it is set.
    let banner = "Write-Host 'Autark terminal · Autark bin and mise shims are on PATH' -ForegroundColor DarkGray";
    let mut command = Command::new("powershell.exe");
    command.args(["-NoLogo", "-NoExit", "-Command", banner]);
    tools::environment(&mut command, root);
    command
        .env("PATH", autark_path(root))
        .env("AUTARK_ROOT", root)
        .current_dir(root)
        .creation_flags(CREATE_NEW_CONSOLE);
    command.spawn().context("cannot open the terminal")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::autark_path;
    use std::path::Path;

    #[test]
    fn autark_tools_come_before_the_machine_path() {
        let parts: Vec<String> = std::env::split_paths(&autark_path(Path::new("C:\\Autark")))
            .map(|part| part.display().to_string())
            .collect();
        assert_eq!(parts[0], "C:\\Autark\\bin");
        assert_eq!(parts[1], "C:\\Autark\\runtime\\mise-data\\shims");
    }
}
