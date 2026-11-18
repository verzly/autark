use std::ffi::OsString;
use std::fs;
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
/// `wt --version` pops a message box, so the exe is looked up on PATH instead.
fn windows_terminal() -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> =
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        dirs.push(PathBuf::from(local).join("Microsoft").join("WindowsApps"));
    }
    dirs.into_iter()
        .map(|dir| dir.join("wt.exe"))
        .find(|path| path.exists())
}

/// `system` is Autark's own default: bash when git brought one, PowerShell otherwise.
fn selected(requested: &str, shells: &tools::TerminalInfo) -> &'static str {
    match requested {
        "powershell" => "powershell",
        "cmd" => "cmd",
        "bash" if shells.bash.is_some() => "bash",
        "sh" if shells.sh.is_some() => "sh",
        _ if shells.bash.is_some() && requested == "system" => "bash",
        _ => "powershell",
    }
}

fn banner(root: &Path) -> Vec<String> {
    vec![
        format!("Autark v{}", env!("CARGO_PKG_VERSION")),
        format!("Autark terminal · {}", root.display()),
        "AGPL-3.0 · github.com/verzly/autark".to_string(),
        "© 2026 rozsazoltan · apps, tools and settings stay in this folder".to_string(),
        "Autark bin and mise shims are on PATH".to_string(),
    ]
}

/// A startup file is the only way to get a banner into a real interactive shell.
fn profile(root: &Path) -> Result<PathBuf> {
    let dir = root.join("runtime").join("terminal");
    fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let path = dir.join("profile.sh");
    let quoted = banner(root)
        .iter()
        .map(|line| format!("'{line}'"))
        .collect::<Vec<_>>()
        .join(" ");
    let text = format!(
        "[ -f \"$HOME/.bashrc\" ] && . \"$HOME/.bashrc\"\nprintf '%s\\n' {quoted}\nprintf '\\n'\n"
    );
    fs::write(&path, text).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(path)
}

/// Opens the chosen shell in the Autark root, with its banner above the first prompt.
pub fn open(root: &Path, requested: &str, shells: &tools::TerminalInfo) -> Result<()> {
    let shell = selected(requested, shells);
    let mut args: Vec<String> = Vec::new();
    let program = match shell {
        "bash" | "sh" => {
            let exe = if shell == "bash" {
                shells.bash.clone()
            } else {
                shells.sh.clone()
            }
            .context("that shell is not available")?;
            let file = profile(root)?;
            if shell == "bash" {
                args.push("--init-file".to_string());
                args.push(file.display().to_string());
            }
            args.push("-i".to_string());
            exe
        }
        "cmd" => {
            let script = banner(root)
                .iter()
                .map(|line| format!("echo {line}"))
                .collect::<Vec<_>>()
                .join(" & ");
            args.push("/K".to_string());
            args.push(format!("chcp 65001>nul & {script} & echo."));
            "cmd.exe".to_string()
        }
        _ => {
            let script = banner(root)
                .iter()
                .map(|line| format!("Write-Host '{line}' -ForegroundColor DarkGray; "))
                .collect::<String>()
                + "Write-Host ''";
            args.extend(["-NoLogo", "-NoExit", "-Command"].map(str::to_string));
            args.push(script);
            "powershell.exe".to_string()
        }
    };
    let mut command = match windows_terminal() {
        Some(wt) => {
            let mut command = Command::new(wt);
            command.arg("-d").arg(root).arg(&program).args(&args);
            command
        }
        None => {
            let mut command = Command::new(&program);
            command.args(&args);
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
    if shell == "sh" {
        // POSIX sh reads $ENV instead of an rc flag, so it gets the same banner file.
        if let Ok(file) = profile(root) {
            command.env("ENV", file);
        }
    }
    if matches!(shell, "bash" | "sh") {
        command.env("TERM", "xterm-256color");
    }
    command.spawn().context("cannot open the terminal")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{autark_path, selected};
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
        assert_eq!(selected("system", &without), "powershell");
        let with = TerminalInfo {
            bash: Some("C:\\git\\bin\\bash.exe".to_string()),
            sh: Some("C:\\git\\usr\\bin\\sh.exe".to_string()),
        };
        assert_eq!(selected("system", &with), "bash");
        assert_eq!(selected("sh", &with), "sh");
        assert_eq!(selected("cmd", &with), "cmd");
    }
}
