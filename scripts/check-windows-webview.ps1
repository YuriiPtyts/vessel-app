param([Parameter(Mandatory = $true)][string]$Executable)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$report = Join-Path $root '.cache/windows-webview-report.json'
if (Test-Path $report) { Remove-Item $report }
$env:VESSEL_WINDOWS_SMOKE_REPORT = $report
$env:VESSEL_RUNTIME_DIR = Join-Path $root 'src-tauri/runtime'
try {
  $process = Start-Process -FilePath (Resolve-Path $Executable) -PassThru
  if (-not $process.WaitForExit(45000)) { $process.Kill($true); throw 'Windows WebView2 smoke test timed out.' }
  if ($process.ExitCode -ne 0 -or -not (Test-Path $report)) { throw 'Windows WebView2 smoke test failed before reporting.' }
  $result = Get-Content $report -Raw | ConvertFrom-Json
  if (-not $result.passed -or -not $result.nativeIpc -or -not $result.webview2Mounted -or -not $result.runtimeAvailable) { throw 'Windows WebView2/native IPC contract failed.' }
  if (-not $result.transparentWebview -or -not $result.nativeAcrylicBackdrop -or -not $result.opaqueBackdropRemoved -or -not $result.cleanDarkText -or -not $result.signInHandoff.passed) { throw 'Windows transparency, text, or cookie-handoff contract failed.' }
  $result | ConvertTo-Json
} finally {
  Remove-Item Env:VESSEL_WINDOWS_SMOKE_REPORT -ErrorAction SilentlyContinue
  Remove-Item Env:VESSEL_RUNTIME_DIR -ErrorAction SilentlyContinue
}
