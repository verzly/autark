$ErrorActionPreference = 'Stop'
# A running autark.exe locks the debug executable, so cargo cannot replace it.
# Only the debug build is stopped: a release Autark can keep running next to it.
$targetDir = (cargo metadata --no-deps --format-version 1 | ConvertFrom-Json).target_directory
$debugExe = Join-Path $targetDir 'debug\autark.exe'
Get-Process autark -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -and $_.Path -ieq $debugExe } |
    Stop-Process -Force
# A WebView2 process left over from an earlier run keeps the profile busy,
# which fails the next start with HRESULT 0x800700AA (resource in use).
Get-CimInstance Win32_Process -Filter "Name = 'msedgewebview2.exe'" |
    Where-Object { $_.CommandLine -like '*com.datarose.autark-dev*' } |
    ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
cargo tauri dev --config tauri.dev.conf.json
