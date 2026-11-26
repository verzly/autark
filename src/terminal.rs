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
fn banner(root: &Path) -> Vec<String> {
    let line = |code: u8, text: String| {
        format!("Write-Host \"$e[38;5;{code}m{text}$e[0m\"")
    };
    vec![
        "$e=[char]27".to_string(),
        line(110, format!("Autark v{}", env!("CARGO_PKG_VERSION"))),
        line(245, format!("Autark terminal · {}", root.display())),
        line(245, "AGPL-3.0 · github.com/verzly/autark".to_string()),
        line(
            245,
            "© 2026 rozsazoltan · apps, tools and settings stay in this folder".to_string(),
        ),
        line(240, "Autark bin and mise shims are on PATH".to_string()),
    ]
}

/// A script file keeps the shell command out of nested cmd quoting.
fn launcher(root: &Path, shell: &str, shells: &tools::TerminalInfo) -> Result<PathBuf> {
    let dir = root.join("runtime").join("terminal");
    fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let path = dir.join("start.ps1");
    let mut lines = banner(root);
    match shell {
        "bash" | "sh" => {
            let exe = if shell == "bash" {
                shells.bash.clone()
            } else {
                shells.sh.clone()
            }
            .context("that shell is not available")?;
            // -i keeps the shell interactive; without it it may exit on the spot.
            lines.push(format!("& '{exe}' -i"));
        }
        "cmd" => {
            lines.push("Write-Host ''".to_string());
            lines.push("cmd.exe".to_string());
        }
        // bash and sh already leave a blank line before their prompt.
        _ => lines.push("Write-Host ''".to_string()),
    }
    // Windows PowerShell reads a .ps1 without a BOM as ANSI, which mangles · and ©.
    fs::write(&path, format!("\u{feff}{}\r\n", lines.join("\r\n")))
        .with_context(|| format!("cannot write {}", path.display()))?;
    Ok(path)
}

/// Opens a console window that prints the banner and then runs the chosen shell.
pub fn open(root: &Path, requested: &str, shells: &tools::TerminalInfo) -> Result<()> {
    let shell = selected(requested, shells);
    let script = launcher(root, shell, shells)?;
    let interactive = shell == "powershell";
    // `start` opens the console window with clean handles; Autark itself never needs one.
    let mut command = Command::new("cmd.exe");
    command.args(["/C", "start", "", "powershell.exe", "-NoLogo"]);
    if interactive {
        command.arg("-NoExit");
    }
    command.arg("-File").arg(&script);
    tools::environment(&mut command, root);
    command
        .env("PATH", autark_path(root))
        .env("AUTARK_ROOT", root)
        .current_dir(root)
        // cmd itself stays invisible; `start` gives the shell its own window.
        .creation_flags(0x0800_0000);
    command.spawn().context("cannot open the terminal")?;
    Ok(())
}

/// Banner and a clean environment: both windows look like the Autark terminal.
fn head(root: &Path, command: &Command) -> Vec<String> {
    let quote = |text: &str| format!("'{}'", text.replace('\'', "''"));
    let envs = command.get_envs().collect::<Vec<_>>();
    let mut lines = vec!["$ErrorActionPreference = 'Continue'".to_string()];
    lines.extend(banner(root));
    lines.push(String::new());
    // apply_environment clears the environment and always sets PATH.
    // shortcut: a command that sets PATH without clearing loses nothing here.
    if envs
        .iter()
        .any(|(key, _)| key.to_string_lossy().eq_ignore_ascii_case("PATH"))
    {
        lines.push(
            "Get-ChildItem env: | ForEach-Object { Remove-Item \"env:$($_.Name)\" -ErrorAction SilentlyContinue }"
                .to_string(),
        );
    }
    for (key, value) in envs {
        let value = value.map(|value| value.to_string_lossy().into_owned()).unwrap_or_default();
        lines.push(format!("$env:{} = {}", key.to_string_lossy(), quote(&value)));
    }
    if let Some(cwd) = command.get_current_dir() {
        lines.push(format!("Set-Location -LiteralPath {}", quote(&cwd.to_string_lossy())));
    }
    lines
}

/// The program and its arguments as one PowerShell call.
fn call(command: &Command) -> String {
    let quote = |text: &str| format!("'{}'", text.replace('\'', "''"));
    let mut call = quote(&command.get_program().to_string_lossy());
    for arg in command.get_args() {
        call.push(' ');
        call.push_str(&quote(&arg.to_string_lossy()));
    }
    call
}

fn write_script(root: &Path, name: &str, lines: &[String]) -> Result<PathBuf> {
    let dir = root.join("runtime").join("terminal");
    fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let script = dir.join(format!("{name}.ps1"));
    fs::write(&script, format!("\u{feff}{}\r\n", lines.join("\r\n")))
        .with_context(|| format!("cannot write {}", script.display()))?;
    Ok(script)
}

fn window(root: &Path, script: &Path, keep_open: bool, label: &str) -> Result<std::process::Child> {
    let mut spawn = Command::new("powershell.exe");
    spawn.args(["-NoLogo", "-ExecutionPolicy", "Bypass", "-File"]);
    if keep_open {
        spawn.arg("-NoExit");
    }
    spawn.arg(script);
    tools::environment(&mut spawn, root);
    spawn
        .env("PATH", autark_path(root))
        .env("AUTARK_ROOT", root)
        .current_dir(root)
        // CREATE_NEW_CONSOLE: the window outlives Autark.
        .creation_flags(0x0000_0010);
    spawn
        .spawn()
        .with_context(|| format!("cannot open a terminal for `{label}`"))
}

/// A separate console window that runs the command and stays open afterwards.
///
/// Autark cannot read the child's status once its own window owns the process,
/// so the runner writes the exit code to `exit_file`.
pub fn run_detached(
    root: &Path,
    id: &str,
    label: &str,
    command: &Command,
    exit_file: &Path,
) -> Result<std::process::Child> {
    let quote = |text: &str| format!("'{}'", text.replace('\'', "''"));
    let mut lines = head(root, command);
    lines.push(format!(
        "Write-Host {} -ForegroundColor DarkGray",
        quote(&format!("$ {label}"))
    ));
    lines.extend([
        "$code = 1".to_string(),
        "try {".to_string(),
        format!("    & {}", call(command)),
        "    $code = if ($null -ne $LASTEXITCODE) { $LASTEXITCODE } else { 0 }".to_string(),
        "} catch {".to_string(),
        "    Write-Host $_ -ForegroundColor Red".to_string(),
        "} finally {".to_string(),
        format!(
            "    Set-Content -LiteralPath {} -Value $code -Encoding ascii",
            quote(&exit_file.to_string_lossy())
        ),
        "    Write-Host ''".to_string(),
        "    Write-Host \"exit code $code - this terminal stays open\" -ForegroundColor DarkGray"
            .to_string(),
        "}".to_string(),
    ]);
    let script = write_script(root, id, &lines)?;
    window(root, &script, true, label)
}

/// The app's own console: the same banner, then the app in the foreground.
/// The wrapper exits with the app, so the phase watcher still sees it end.
pub fn app_window(root: &Path, id: &str, label: &str, command: &Command) -> Result<std::process::Child> {
    let quote = |text: &str| format!("'{}'", text.replace('\'', "''"));
    let mut lines = head(root, command);
    lines.push(format!(
        "Write-Host {} -ForegroundColor DarkGray",
        quote(&format!("$ {label}"))
    ));
    lines.push(format!("& {}", call(command)));
    lines.push("exit $LASTEXITCODE".to_string());
    let script = write_script(root, &format!("{id}-app"), &lines)?;
    window(root, &script, false, label)
}

#[cfg(test)]
mod tests {
    use super::{autark_path, launcher, selected};
    use crate::tools::TerminalInfo;
    use std::path::Path;

    fn temp_root() -> std::path::PathBuf {
        std::env::temp_dir().join("autark-terminal-test")
    }

    #[test]
    fn autark_tools_come_before_the_machine_path() {
        let parts: Vec<String> = std::env::split_paths(&autark_path(Path::new("C:\\Autark")))
            .map(|part| part.display().to_string())
            .collect();
        assert_eq!(parts[0], "C:\\Autark\\bin");
        assert_eq!(parts[1], "C:\\Autark\\runtime\\mise-data\\shims");
    }

    #[test]
    fn the_launcher_prints_a_colored_banner_then_the_shell() {
        let root = temp_root();
        let shells = TerminalInfo {
            bash: Some("C:\\git\\bin\\bash.exe".to_string()),
            sh: Some("C:\\git\\bin\\sh.exe".to_string()),
        };
        let text = std::fs::read_to_string(launcher(&root, "bash", &shells).unwrap()).unwrap();
        assert!(text.starts_with("\u{feff}"));
        assert!(text.contains("[38;5;110m"));
        assert!(text.contains(env!("CARGO_PKG_VERSION")));
        assert!(text.trim_end().ends_with("& 'C:\\git\\bin\\bash.exe' -i"));
        assert!(!text.contains("Write-Host ''"));
        let plain = std::fs::read_to_string(launcher(&root, "powershell", &shells).unwrap()).unwrap();
        assert!(!plain.contains("cmd.exe"));
        assert!(plain.trim_end().ends_with("Write-Host ''"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_default_shell_follows_git() {
        let without = TerminalInfo::default();
        assert_eq!(selected("system", &without), "powershell");
        let with = TerminalInfo {
            bash: Some("C:\\git\\bin\\bash.exe".to_string()),
            sh: Some("C:\\git\\bin\\sh.exe".to_string()),
        };
        assert_eq!(selected("system", &with), "bash");
        assert_eq!(selected("sh", &with), "sh");
        assert_eq!(selected("cmd", &with), "cmd");
    }
}
