$ErrorActionPreference = 'Stop'
# A running autark.exe locks target\debug\autark.exe, so cargo cannot replace it.
Get-Process autark -ErrorAction SilentlyContinue | Stop-Process -Force
cargo tauri dev
