use std::ffi::OsString;
use std::os::windows::process::CommandExt;
use std::path::Path;
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

/// The gray banner every shell prints before its first prompt.
fn banner(root: &Path) -> Vec<String> {
    vec![
        format!("Autark v{}", env!("CARGO_PKG_VERSION")),
        format!("Autark terminal · {}", root.display()),
        "AGPL-3.0 · github.com/verzly/autark".to_string(),
        "© 2026 rozsazoltan · apps, tools and settings stay in this folder".to_string(),
        "Autark bin and mise shims are on PATH".to_string(),
    ]
}

/// `system` is Autark's own default: bash through git when it is there, PowerShell otherwise.
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

fn shell_command(root: &Path, shell: &str, shells: &tools::TerminalInfo) -> Result<Command> {
    let lines = banner(root);
    // The shell is told where to stand, so nothing inherited from Autark's own start folder can win.
    let title = format!("Autark · {}", root.display());
    let posix = root.display().to_string().replace('\\', "/");
    match shell {
        "cmd" => {
            let script = lines
                .iter()
                .map(|line| format!("echo {line}"))
                .collect::<Vec<_>>()
                .join(" & ");
            let mut command = Command::new("cmd.exe");
            command.arg("/K").arg(format!(
                "cd /d \"{}\" & title {} & chcp 65001>nul & {script} & echo.",
                root.display(),
                title
            ));
            Ok(command)
        }
        "bash" | "sh" => {
            let exe = if shell == "bash" {
                shells.bash.clone()
            } else {
                shells.sh.clone()
            }
            .context("that shell is not available")?;
            let quoted = lines
                .iter()
                .map(|line| format!("'{line}'"))
                .collect::<Vec<_>>()
                .join(" ");
            let mut command = Command::new(&exe);
            command.args([
                "-c",
                &format!(
                    "cd '{posix}'; printf '\\033]0;{title}\\007'; printf '%s\\n' {quoted}; echo; exec '{exe}' -i"
                ),
            ]);
            Ok(command)
        }
        _ => {
            let script = lines
                .iter()
                .map(|line| format!("Write-Host '{line}' -ForegroundColor DarkGray; "))
                .collect::<String>()
                + "Write-Host ''";
            let mut command = Command::new("powershell.exe");
            command.args([
                "-NoLogo",
                "-NoExit",
                "-Command",
                &format!("Set-Location -LiteralPath '{}'; $Host.UI.RawUI.WindowTitle = '{title}'; {script}", root.display()),
            ]);
            Ok(command)
        }
    }
}

/// Opens the chosen shell in the Autark root with Autark's PATH and a short banner.
pub fn open(root: &Path, requested: &str, shells: &tools::TerminalInfo) -> Result<()> {
    let mut command = shell_command(root, selected(requested, shells), shells)?;
    tools::environment(&mut command, root);
    command
        .env("PATH", autark_path(root))
        .env("AUTARK_ROOT", root)
        .current_dir(root)
        // The shell needs its own console window, even though Autark itself has none.
        .creation_flags(0x0000_0010);
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
            sh: None,
        };
        assert_eq!(selected("system", &with), "bash");
        assert_eq!(selected("sh", &with), "powershell");
        assert_eq!(selected("bash", &with), "bash");
        assert_eq!(selected("cmd", &without), "cmd");
    }
}
