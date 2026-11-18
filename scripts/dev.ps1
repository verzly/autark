$ErrorActionPreference = 'Stop'
# A running autark.exe locks target\debug\autark.exe, so cargo cannot replace it.
Get-Process autark -ErrorAction SilentlyContinue | Stop-Process -Force
# A WebView2 process left over from an earlier run keeps the profile busy,
# which fails the next start with HRESULT 0x800700AA (resource in use).
Get-CimInstance Win32_Process -Filter "Name = 'msedgewebview2.exe'" |
    Where-Object { $_.CommandLine -like '*dev.verzly.autark*' } |
    ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
cargo tauri dev
