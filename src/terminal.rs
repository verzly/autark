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

/// The banner: color code and text per line. 38;5;n keeps it soft on any terminal.
fn banner(root: &Path) -> Vec<(u8, String)> {
    vec![
        (110, format!("Autark v{}", env!("CARGO_PKG_VERSION"))),
        (245, format!("Autark terminal · {}", root.display())),
        (245, "AGPL-3.0 · github.com/verzly/autark".to_string()),
        (
            245,
            "© 2026 rozsazoltan · apps, tools and settings stay in this folder".to_string(),
        ),
        (240, "Autark bin and mise shims are on PATH".to_string()),
    ]
}

/// cmd prints text as-is only when the shell metacharacters are escaped.
fn cmd_text(text: &str) -> String {
    text.replace('·', "-")
        .replace('©', "(c)")
        .replace('^', "^^")
        .replace('&', "^&")
        .replace('|', "^|")
        .replace('<', "^<")
        .replace('>', "^>")
}

/// A single quote would end the printf argument early.
fn shell_quote(text: &str) -> String {
    text.replace('\'', "'\\''")
}

/// A startup file is the only way to get a banner into a real interactive shell.
fn profile(root: &Path) -> Result<PathBuf> {
    let dir = root.join("runtime").join("terminal");
    fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let path = dir.join("profile.sh");
    let mut text = String::from("[ -f \"$HOME/.bashrc\" ] && . \"$HOME/.bashrc\"\n");
    for (color, line) in banner(root) {
        text.push_str(&format!(
            "printf '\\033[38;5;{color}m%s\\033[0m\\n' '{}'\n",
            shell_quote(&line)
        ));
    }
    text.push_str("printf '\\n'\n");
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
            // `echo(` prints the spaces exactly, and the joins carry no separator spaces.
            let script = banner(root)
                .iter()
                .map(|(_, line)| format!("echo({}", cmd_text(line)))
                .collect::<Vec<_>>()
                .join("&");
            args.push("/K".to_string());
            args.push(format!("chcp 65001>nul&{script}&echo."));
            "cmd.exe".to_string()
        }
        _ => {
            let script = "$e=[char]27; "
                .to_string()
                + &banner(root)
                    .iter()
                    .map(|(color, line)| format!("Write-Host \"$e[38;5;{color}m{line}$e[0m\"; "))
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
    fn cmd_lines_keep_their_text() {
        assert_eq!(super::cmd_text("Autark · ©"), "Autark - (c)");
        assert_eq!(super::cmd_text("a & b | c"), "a ^& b ^| c");
        assert_eq!(super::shell_quote("it's"), "it'\\''s");
    }

    #[test]
    fn the_profile_carries_a_colored_banner() {
        let root = std::env::temp_dir().join("autark-terminal-profile-test");
        let path = super::profile(&root).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("[38;5;110m"));
        assert!(text.contains("Autark v"));
        assert!(text.contains("$HOME/.bashrc"));
        let _ = std::fs::remove_dir_all(&root);
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
