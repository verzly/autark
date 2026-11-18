use std::ffi::OsString;
use std::io::{Read, Write};
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Mutex;
use std::thread;

use anyhow::{anyhow, Context, Result};
use tauri::{AppHandle, Emitter};

use crate::tools;

/// Tauri event carrying raw terminal output.
pub const EVENT: &str = "terminal";
/// No console window for the shell behind the terminal tab.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

struct Session {
    child: Child,
    input: ChildStdin,
}

/// One long-lived shell behind the terminal tab.
pub struct Terminal(Mutex<Option<Session>>);

impl Default for Terminal {
    fn default() -> Self {
        Self(Mutex::new(None))
    }
}

/// Autark's own tools come first, so the terminal sees mise, git and every installed tool by name.
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

impl Terminal {
    /// Starts the shell once; a second start keeps the running session.
    pub fn start(&self, app: &AppHandle, root: &Path) -> Result<()> {
        let mut slot = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if slot.is_some() {
            return Ok(());
        }
        // `-Command -` reads the commands from stdin, so the working directory and variables persist.
        let mut command = Command::new("powershell.exe");
        command
            .args(["-NoLogo", "-NoProfile", "-Command", "-"])
            .current_dir(root)
            .env("PATH", autark_path(root))
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        tools::environment(&mut command, root);
        let mut child = command.spawn().context("cannot start powershell")?;
        let stdin = child.stdin.take().context("powershell has no stdin")?;
        let mut streams: Vec<Box<dyn Read + Send>> = Vec::new();
        if let Some(stdout) = child.stdout.take() {
            streams.push(Box::new(stdout));
        }
        if let Some(stderr) = child.stderr.take() {
            streams.push(Box::new(stderr));
        }
        for stream in streams {
            let handle = app.clone();
            thread::spawn(move || pump(stream, handle));
        }
        let mut input = stdin;
        // UTF-8 both ways, so non-ASCII paths and file names survive the pipe.
        input.write_all(
            b"[Console]::InputEncoding=[Text.UTF8Encoding]::new($false);[Console]::OutputEncoding=[Console]::InputEncoding\r\n",
        )?;
        input.flush()?;
        *slot = Some(Session { child, input });
        Ok(())
    }

    pub fn write(&self, line: &str) -> Result<()> {
        let mut slot = self.0.lock().unwrap_or_else(|error| error.into_inner());
        let session = slot.as_mut().ok_or_else(|| anyhow!("terminal is not running"))?;
        let written = session
            .input
            .write_all(line.as_bytes())
            .and_then(|_| session.input.write_all(b"\r\n"))
            .and_then(|_| session.input.flush());
        if written.is_err() {
            if let Some(mut closed) = slot.take() {
                let _ = closed.child.kill();
            }
        }
        written.context("cannot write to the terminal")
    }

    pub fn stop(&self) {
        let mut slot = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(mut session) = slot.take() {
            let _ = session.child.kill();
            let _ = session.child.wait();
        }
    }
}

/// A chunk can end in the middle of a character, so only whole characters are emitted.
fn pump(mut stream: Box<dyn Read + Send>, app: AppHandle) {
    let mut pending: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let read = match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(read) => read,
        };
        pending.extend_from_slice(&chunk[..read]);
        let valid = match std::str::from_utf8(&pending) {
            Ok(_) => pending.len(),
            Err(error) => error.valid_up_to(),
        };
        if valid == pending.len() {
            let _ = app.emit(EVENT, String::from_utf8_lossy(&pending).to_string());
            pending.clear();
            continue;
        }
        if valid > 0 {
            let text = String::from_utf8_lossy(&pending[..valid]).to_string();
            let _ = app.emit(EVENT, text);
            pending.drain(..valid);
        }
        if pending.len() >= 4 {
            let text = String::from_utf8_lossy(&pending).to_string();
            let _ = app.emit(EVENT, text);
            pending.clear();
        }
    }
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
