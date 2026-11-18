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

/// PowerShell prints the banner for every shell, so one code path colours all of them.
fn banner(root: &Path) -> String {
    let line = |code: u8, text: &str| format!("Write-Host \"$e[38;5;{code}m{text}$e[0m\"; ");
    "$e=[char]27; ".to_string()
        + &line(110, &format!("Autark v{}", env!("CARGO_PKG_VERSION")))
        + &line(245, &format!("Autark terminal · {}", root.display()))
        + &line(245, "AGPL-3.0 · github.com/verzly/autark")
        + &line(
            245,
            "© 2026 rozsazoltan · apps, tools and settings stay in this folder",
        )
        + &line(240, "Autark bin and mise shims are on PATH")
        + "Write-Host ''"
}

/// Opens PowerShell in the Autark root, prints the banner, then hands over to the chosen shell.
pub fn open(root: &Path, requested: &str, shells: &tools::TerminalInfo) -> Result<()> {
    let shell = selected(requested, shells);
    let mut script = banner(root);
    let mut interactive = false;
    match shell {
        "bash" | "sh" => {
            let exe = if shell == "bash" {
                shells.bash.clone()
            } else {
                shells.sh.clone()
            }
            .context("that shell is not available")?;
            script.push_str(&format!("; & '{exe}'"));
        }
        "cmd" => script.push_str("; cmd.exe"),
        // PowerShell is the target itself, so the session must survive the banner.
        _ => interactive = true,
    }
    let mut args = vec!["-NoLogo".to_string()];
    if interactive {
        args.push("-NoExit".to_string());
    }
    args.push("-Command".to_string());
    args.push(script);
    let mut command = match windows_terminal() {
        Some(wt) => {
            let mut command = Command::new(wt);
            command
                .arg("-d")
                .arg(root)
                .arg("powershell.exe")
                .args(&args);
            command
        }
        None => {
            let mut command = Command::new("powershell.exe");
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
    command.spawn().context("cannot open the terminal")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{autark_path, banner, selected};
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
    fn the_banner_is_colored_and_versioned() {
        let text = banner(Path::new("C:\\Autark"));
        assert!(text.contains("$e[38;5;110m"));
        assert!(text.contains(env!("CARGO_PKG_VERSION")));
        assert!(text.contains("C:\\Autark"));
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
