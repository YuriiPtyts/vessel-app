param([Parameter(Mandatory = $true)][string]$Installer)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$temporary = Join-Path ([IO.Path]::GetTempPath()) ("Vessel Installer Test " + [guid]::NewGuid())
$install = Join-Path $temporary 'Installed App'
New-Item -ItemType Directory -Force -Path $temporary | Out-Null
$process = $null
try {
  $setup = Start-Process -FilePath (Resolve-Path $Installer) -ArgumentList "/S /D=$install" -PassThru
  if (-not $setup.WaitForExit(180000)) { $setup.Kill(); throw 'Windows installer timed out.' }
  if ($setup.ExitCode -ne 0) { throw "Windows installer failed: $($setup.ExitCode)" }
  $app = Join-Path $install 'Vessel.exe'
  $runtime = Join-Path $install 'runtime'
  if (-not (Test-Path $app)) { throw 'Installed Vessel.exe is missing.' }
  & node (Join-Path $PSScriptRoot 'check-windows-runtime.mjs') $runtime
  if ($LASTEXITCODE -ne 0) { throw 'Installed runtime verification failed.' }
  & (Join-Path $runtime 'yt-dlp.exe') --ignore-config --no-plugin-dirs --version
  if ($LASTEXITCODE -ne 0) { throw 'Installed extractor could not launch.' }
  & (Join-Path $runtime 'deno.exe') eval --no-config 'if (1 + 1 !== 2) throw Error("runtime failure");'
  if ($LASTEXITCODE -ne 0) { throw 'Installed JavaScript runtime failed.' }
  $previousRuntime = $env:VESSEL_RUNTIME_DIR
  try {
    $env:VESSEL_RUNTIME_DIR = $runtime
    & cargo test --manifest-path (Join-Path $root 'src-tauri/Cargo.toml') --locked --lib native_windows_runtime_decodes_local_audio -- --ignored --nocapture --test-threads=1
    if ($LASTEXITCODE -ne 0) { throw 'Installed libmpv could not decode local audio.' }
    & cargo test --manifest-path (Join-Path $root 'src-tauri/Cargo.toml') --locked --lib native_volume_values_are_applied_immediately -- --ignored --nocapture --test-threads=1
    if ($LASTEXITCODE -ne 0) { throw 'Installed libmpv volume check failed.' }
  } finally { $env:VESSEL_RUNTIME_DIR = $previousRuntime }
  $process = Start-Process -FilePath $app -PassThru
  if ($process.WaitForExit(8000)) { throw "Installed application exited early: $($process.ExitCode)" }
  $process.Refresh()
  if ($process.MainWindowHandle -eq 0) { throw 'Installed application has no main window.' }
  if (-not $process.CloseMainWindow()) { throw 'Could not request normal application close.' }
  if (-not $process.WaitForExit(15000)) { throw 'Installed application did not quit normally.' }
  if ($process.ExitCode -ne 0) { throw "Installed application failed: $($process.ExitCode)" }
  $uninstaller = Get-ChildItem -Path $install -Filter '*uninstall*.exe' | Select-Object -First 1
  if (-not $uninstaller) { throw 'Installed uninstaller is missing.' }
  $remove = Start-Process -FilePath $uninstaller.FullName -ArgumentList '/S' -PassThru
  if (-not $remove.WaitForExit(30000) -or $remove.ExitCode -ne 0) { throw 'Silent uninstall failed.' }
  $uninstallDeadline = [DateTime]::UtcNow.AddSeconds(15)
  while (Test-Path $app) {
    if ([DateTime]::UtcNow -ge $uninstallDeadline) { throw 'Uninstaller did not remove the installed application.' }
    Start-Sleep -Milliseconds 100
  }
  @{ passed = $true; installedRuntime = $true; localPcmDecoded = $true; nativeWindow = $true; normalQuit = $true; uninstalled = $true; audibleOutputTested = $false; realAccountTested = $false } | ConvertTo-Json
} finally {
  if ($process -and -not $process.HasExited) { $process.Kill($true) }
  # NSIS can finish deleting runtime files after its launcher exits. Only retry
  # cleanup of this fixture's unique directory; do not race the uninstaller.
  $cleanupDeadline = [DateTime]::UtcNow.AddSeconds(15)
  while (Test-Path $temporary) {
    try { Remove-Item -LiteralPath $temporary -Recurse -Force -ErrorAction Stop }
    catch {
      if (-not (Test-Path $temporary)) { break }
      if ([DateTime]::UtcNow -ge $cleanupDeadline) { throw }
      Start-Sleep -Milliseconds 200
    }
  }
}
