# Runs as SYSTEM at every Windows start (task \LightsOut\Cleanup):
#   1. removes a leftover drive key or "came from Windows" pass from the boot partition
#      (a switch that was interrupted)
#   2. an update whose Lights Out never came up: puts the previous version back
#   3. an update asked for inside Lights Out (`lightsout update`): runs the installer,
#      then goes straight back to Lights Out with your tabs
#   -LO, -Esp  other folders instead of C:\LightsOut and the EFI partition (for the test)
param([string] $LO = 'C:\LightsOut', [string] $Esp)
$u = "$LO\update"
function Note($m) { Add-Content "$LO\logs\update.log" "$(Get-Date -Format s) $m" }

$letter = $null
if (-not $Esp) {
  $letter = (68..90 | ForEach-Object { [char]$_ } | Where-Object { -not (Test-Path "$($_):\") } | Select-Object -Last 1)
  mountvol "$($letter):" /S
  $Esp = "$($letter):"
}
try {
  Remove-Item "$Esp\EFI\lightsout\handoff.key", "$Esp\EFI\lightsout\handoff.nonce" -Force -ErrorAction SilentlyContinue
  $p = "$u\previous"
  if ((Test-Path "$u\trial") -and (Test-Path "$u\tried")) {
    # the updated Lights Out was started but never came up (else it removes trial itself)
    if (Test-Path "$p\system.img") { Move-Item "$p\system.img" "$LO\system.img" -Force }
    if (Test-Path "$p\system-sha256") { Copy-Item "$p\system-sha256" "$LO\config\system-sha256" -Force }
    if (Test-Path "$p\starter") {
      Copy-Item "$p\starter\*" "$Esp\EFI\lightsout\" -Force
      if (Test-Path "$Esp\EFI\lightsout\created-fedora-dir") { Copy-Item "$p\starter\grub.cfg" "$Esp\EFI\fedora\grub.cfg" -Force }
    }
    Remove-Item "$u\trial", "$u\tried", $p -Recurse -Force -ErrorAction SilentlyContinue
    Note 'the updated Lights Out did not start: the previous version is back'
  } elseif ((Test-Path "$u\trial") -and (Get-Item "$u\trial").LastWriteTime -lt (Get-Date).AddDays(-14)) {
    # updated two weeks ago and Lights Out not used since: the backup's room goes back to Windows
    Remove-Item "$u\trial", $p -Recurse -Force -ErrorAction SilentlyContinue
  }
} finally { if ($letter) { mountvol "$($letter):" /D } }

if (Test-Path "$u\requested") {
  Remove-Item "$u\requested" -Force
  $url = (Get-Content "$LO\bin\installer-url" -ErrorAction SilentlyContinue | Select-Object -First 1)
  # Windows connects to Wi-Fi at start, before anyone signs in
  for ($i = 0; $i -lt 60 -and -not (Test-NetConnection lightsout.sh -Port 443 -InformationLevel Quiet -WarningAction SilentlyContinue); $i++) { Start-Sleep 2 }
  powershell -NoProfile -ExecutionPolicy Bypass -Command "irm '$url' | iex" *> "$LO\logs\update-install.log"
  Note "installed from $url (exit $LASTEXITCODE)"
  # back to Lights Out, with the tabs it handed back
  if (Test-Path "$LO\handoff\back.json") { Move-Item "$LO\handoff\back.json" "$LO\handoff\tabs.json" -Force }
  $b = [byte[]]::new(32); [Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($b)
  [IO.File]::WriteAllText("$LO\handoff\nonce", -join ($b | ForEach-Object { $_.ToString('x2') }))
  & "$LO\bin\enter.ps1"
}
