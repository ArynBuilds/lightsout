# Removes Lights Out (`lightsout uninstall` runs this as administrator).
#   -KeepSystem  keep C:\LightsOut\system.img and user.img (your Lights Out installs and logins)
param([switch] $KeepSystem)
$ErrorActionPreference = 'Continue'
$LO = 'C:\LightsOut'
Get-Process lightsout-app, lightsout -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep 1

# boot entry
$fw = bcdedit /enum firmware; $cur = $null
for ($i = 0; $i -lt $fw.Count; $i++) {
  if ($fw[$i] -match '^identifier\s+(\{[0-9a-f-]+\})') { $cur = $Matches[1] }
  if ($fw[$i] -match '^description\s+Lights Out$') { bcdedit /delete $cur | Out-Null; Write-Host "removed boot entry $cur" }
}
# EFI partition files
$letter = (68..90 | ForEach-Object { [char]$_ } | Where-Object { -not (Test-Path "$($_):\") } | Select-Object -Last 1)
mountvol "$($letter):" /S
try {
  if (Test-Path "$($letter):\EFI\lightsout\created-fedora-dir") { Remove-Item "$($letter):\EFI\fedora" -Recurse -Force }
  Remove-Item "$($letter):\EFI\lightsout" -Recurse -Force -ErrorAction SilentlyContinue
  Write-Host 'removed the starter from the EFI partition'
} finally { mountvol "$($letter):" /D }
# BitLocker key we added
if (Test-Path "$LO\config\protector-id") {
  $id = (Get-Content "$LO\config\protector-id").Trim()
  try { Remove-BitLockerKeyProtector -MountPoint 'C:' -KeyProtectorId $id | Out-Null; Write-Host 'removed the Lights Out BitLocker key' } catch { Write-Host "could not remove key $id" }
}
# task, shortcut, files
schtasks /delete /tn '\LightsOut\Enter' /f 2>$null | Out-Null
schtasks /delete /tn '\LightsOut\Cleanup' /f 2>$null | Out-Null
try { $svc = New-Object -ComObject Schedule.Service; $svc.Connect(); $svc.GetFolder('\').DeleteFolder('LightsOut', 0) } catch {}
# the app's own settings and saved tabs
$state = Join-Path $env:LOCALAPPDATA 'lightsout'
foreach ($p in (Get-CimInstance Win32_UserProfile | Where-Object { -not $_.Special }).LocalPath) {
  Remove-Item (Join-Path $p 'AppData\Local\lightsout') -Recurse -Force -ErrorAction SilentlyContinue
}
Remove-Item $state -Recurse -Force -ErrorAction SilentlyContinue
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
[Environment]::SetEnvironmentVariable('Path', ((($userPath -split ';') | Where-Object { $_ -and $_ -ne "$LO\bin" }) -join ';'), 'User')
Remove-Item "$env:ProgramData\Microsoft\Windows\Start Menu\Programs\Lights Out.lnk" -ErrorAction SilentlyContinue
Remove-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name 'Lights Out' -ErrorAction SilentlyContinue
if ($KeepSystem) {
  Get-ChildItem $LO -Exclude system.img, user.img | Remove-Item -Recurse -Force
  Write-Host "kept $LO\system.img and user.img"
} else {
  Remove-Item $LO -Recurse -Force
}
bcdedit /deletevalue '{fwbootmgr}' bootsequence 2>$null | Out-Null
Write-Host 'Lights Out removed.' -ForegroundColor Green
