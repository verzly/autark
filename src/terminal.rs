use std::ffi::OsString;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use crate::tools;

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

/// Windows Terminal, so the shell gets a real terminal instead of a bare console.
fn windows_terminal() -> Option<PathBuf> {
    Command::new("wt.exe")
        .arg("--version")
        .output()
        .ok()?
        .status
        .success()
        .then(|| PathBuf::from("wt.exe"))
}

/// The program to run. `system` is Autark's own default: bash when git brought one, PowerShell otherwise.
fn shell_program(requested: &str, shells: &tools::TerminalInfo) -> String {
    match requested {
        "powershell" => "powershell.exe".to_string(),
        "cmd" => "cmd.exe".to_string(),
        "bash" if shells.bash.is_some() => shells.bash.clone().unwrap_or_default(),
        "sh" if shells.sh.is_some() => shells.sh.clone().unwrap_or_default(),
        _ => match &shells.bash {
            Some(bash) if requested == "system" => bash.clone(),
            _ => "powershell.exe".to_string(),
        },
    }
}

/// Opens the chosen shell in the Autark root with Autark's PATH. Nothing else is injected.
pub fn open(root: &Path, requested: &str, shells: &tools::TerminalInfo) -> Result<()> {
    let program = shell_program(requested, shells);
    let mut command = match windows_terminal() {
        Some(wt) => {
            let mut command = Command::new(wt);
            command.arg("-d").arg(root).arg(&program);
            command
        }
        None => {
            let mut command = Command::new(&program);
            if program.eq_ignore_ascii_case("powershell.exe") {
                command.arg("-NoLogo");
            }
            command
        }
    };
    tools::environment(&mut command, root);
    command
        .env("PATH", autark_path(root))
        .env("AUTARK_ROOT", root)
        .current_dir(root)
        // Without Windows Terminal the shell still needs its own console window.
        .creation_flags(0x0000_0010);
    command.spawn().context("cannot open the terminal")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{autark_path, shell_program};
    use crate::tools::TerminalInfo;
    use std::path::Path;

    #[test]
    fn autark_tools_come_before_the_machine_path() {
        let parts: Vec<String> = std::env::split_paths(&autark_path(Path::new("C:\\Autark")))
            .map(|part| part.display().to_string())
            .collect();
        assert_eq!(parts[0], "C:\\Autark\\bin");
        assert_eq!(parts[1], "C:\\Autark\\runtime\\mise-data\\shims");
    }

    #[test]
    fn the_default_shell_follows_git() {
        let without = TerminalInfo::default();
        assert_eq!(shell_program("system", &without), "powershell.exe");
        let with = TerminalInfo {
            bash: Some("C:\\git\\bin\\bash.exe".to_string()),
            sh: Some("C:\\git\\usr\\bin\\sh.exe".to_string()),
        };
        assert_eq!(shell_program("system", &with), "C:\\git\\bin\\bash.exe");
        assert_eq!(shell_program("bash", &with), "C:\\git\\bin\\bash.exe");
        assert_eq!(shell_program("sh", &with), "C:\\git\\usr\\bin\\sh.exe");
        assert_eq!(shell_program("cmd", &with), "cmd.exe");
    }
}
