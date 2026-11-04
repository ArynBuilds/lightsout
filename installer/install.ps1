# Lights Out installer for Windows:  irm https://lightsout.sh | iex
# (asks for admin rights; downloads everything from the latest release)
#   -Source  optional local folder instead of downloading: starter\EFI\lightsout\*,
#            system\system.img.gz(+.sha256), term\lightsout.exe + lightsout-app.exe + uninstall.ps1
# What it does (each step is undone by uninstall.ps1):
#   1. checks this PC can run Lights Out safely
#   2. unpacks the Lights Out system to C:\LightsOut\system.img
#   3. saves your Wi-Fi networks, Windows user name and lock-screen PIN in C:\LightsOut
#      (on your encrypted drive, never on the unencrypted boot area)
#   4. adds a dedicated "Lights Out" key to BitLocker; each switch hands it to the
#      starter for that one start, which seals it into the TPM and deletes it
#      (never typed, never shown, never left on the boot partition)
#   5. installs the small starter on the EFI partition + a boot entry (Windows stays default)
#   6. installs the Lights Out terminal + a task that switches to Lights Out on request
param(
  [string] $Source,
  [int] $MinEspFreeMB = 40,
  [switch] $NoPin,
  [string] $Base = $(if ($env:LIGHTSOUT_BASE) { $env:LIGHTSOUT_BASE } else { 'https://github.com/ArynBuilds/lightsout/releases/latest/download' })
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
# admin rights: reopen this installer in an admin window
if (-not ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
  Write-Host 'Lights Out needs admin rights to install. Confirm the Windows prompt...'
  if ($PSCommandPath) {
    $a = "-NoProfile -ExecutionPolicy Bypass -NoExit -File `"$PSCommandPath`""
    if ($Source) { $a += " -Source `"$Source`"" }
  } else {
    $a = '-NoProfile -ExecutionPolicy Bypass -NoExit -Command "irm https://lightsout.sh | iex"'
  }
  Start-Process powershell -Verb RunAs -ArgumentList $a
  return
}
$LO = 'C:\LightsOut'
New-Item -ItemType Directory -Force "$LO\config", "$LO\bin", "$LO\handoff", "$LO\wifi", "$LO\logs" | Out-Null
Start-Transcript -Path "$LO\logs\install.log" -Force | Out-Null
function Step($m) { Write-Host "`n== $m" -ForegroundColor Cyan }
function Fail($m) { Write-Host "STOP: $m" -ForegroundColor Red; Stop-Transcript | Out-Null; exit 1 }

# ---------------------------------------------------------------- 1. checks
Step 'Checking this PC'
if (-not ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { Fail 'needs administrator' }
if ($env:firmware_type -ne 'UEFI') { Fail 'this PC does not start with UEFI' }
if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64') { Fail 'Lights Out needs an Intel or AMD 64-bit PC (ARM is not supported yet)' }
$tpm = Get-Tpm -ErrorAction SilentlyContinue
$tpmOk = $tpm -and $tpm.TpmReady
$sb = $false; try { $sb = Confirm-SecureBootUEFI } catch {}
Write-Host "Secure Boot: $sb, TPM: $(if ($tpmOk) {'ready'} else {'none'})"
if (-not $Source) {
  Step 'Downloading Lights Out'
  $Source = Join-Path $env:TEMP 'lightsout-setup'
  Remove-Item $Source -Recurse -Force -ErrorAction SilentlyContinue
  New-Item -ItemType Directory -Force "$Source\system", "$Source\term", "$Source\starter" | Out-Null
  foreach ($f in 'lightsout-windows-x64.zip', 'lightsout-starter.zip', 'system.img.gz.sha256', 'system.img.gz') {
    Write-Host "  $f"
    $to = if ($f -like 'system*') { "$Source\system\$f" } else { "$Source\$f" }
    curl.exe -fL --progress-bar -o $to "$Base/$f"
    if ($LASTEXITCODE -ne 0) { Fail "could not download $f" }
  }
  Expand-Archive "$Source\lightsout-windows-x64.zip" "$Source\term" -Force
  Expand-Archive "$Source\lightsout-starter.zip" "$Source\starter" -Force
}
foreach ($f in 'starter\EFI\lightsout\shimx64.efi','starter\EFI\lightsout\starter.img','system\system.img.gz','term\lightsout.exe','term\lightsout-app.exe') {
  if (-not (Test-Path (Join-Path $Source $f))) { Fail "missing $f in $Source" }
}
$gz = Join-Path $Source 'system\system.img.gz'
$freeGB = [math]::Round((Get-PSDrive C).Free / 1GB, 1)
if ($freeGB -lt 6) { Fail "C: has $freeGB GB free; Lights Out needs about 6 GB" }
$bl = Get-BitLockerVolume -MountPoint 'C:' -ErrorAction SilentlyContinue
$encrypted = $bl -and $bl.ProtectionStatus -eq 'On'
Write-Host "C: free: $freeGB GB, BitLocker: $(if ($encrypted) {'on'} else {'off'})"
# the TPM chip is only needed to unlock an encrypted drive without typing a key
if ($encrypted -and -not $tpmOk) { Fail 'C: is encrypted (BitLocker) but this PC has no ready TPM security chip' }

# ---------------------------------------------------------------- 2. system file
Step 'Unpacking the Lights Out system (a few GB on C:)'
$sumFile = "$gz.sha256"
if (Test-Path $sumFile) {
  $want = ((Get-Content $sumFile) -split '\s+')[0].ToLower()
  $have = (Get-FileHash $gz -Algorithm SHA256).Hash.ToLower()
  if ($want -ne $have) { Fail 'the system download is damaged (checksum mismatch)' }
  Write-Host 'checksum ok'
}
if (Test-Path "$LO\system.img") { Write-Host 'keeping the existing system file (your installs and logins stay)' }
else {
  $in = [IO.File]::OpenRead($gz)
  $z = New-Object IO.Compression.GZipStream($in, [IO.Compression.CompressionMode]::Decompress)
  $out = [IO.File]::Create("$LO\system.img.part")
  try { $z.CopyTo($out, 4MB) } finally { $out.Close(); $z.Close(); $in.Close() }
  Move-Item "$LO\system.img.part" "$LO\system.img"
}

# ---------------------------------------------------------------- 3. Wi-Fi, user, PIN
Step 'Saving your Wi-Fi networks and settings for Lights Out'
Remove-Item "$LO\wifi\*" -Force -ErrorAction SilentlyContinue
netsh wlan export profile key=clear folder="$LO\wifi" | Out-Null
Write-Host "Wi-Fi networks: $((Get-ChildItem "$LO\wifi" -Filter *.xml).Count)"
[IO.File]::WriteAllText("$LO\config\windows-user", $env:USERNAME)
# keyboard layout(s) for Lights Out, e.g. "tr" or "us,tr"
$xkb = @{ 'en-US'='us'; 'en-GB'='gb'; 'tr-TR'='tr'; 'de-DE'='de'; 'fr-FR'='fr'; 'es-ES'='es'; 'it-IT'='it'; 'pt-BR'='br'; 'pt-PT'='pt'; 'nl-NL'='nl'; 'ru-RU'='ru'; 'pl-PL'='pl'; 'sv-SE'='se'; 'da-DK'='dk'; 'nb-NO'='no'; 'fi-FI'='fi'; 'ja-JP'='jp'; 'ko-KR'='kr' }
$layouts = (Get-WinUserLanguageList | ForEach-Object { $xkb[$_.LanguageTag] } | Where-Object { $_ } | Select-Object -Unique) -join ','
if (-not $layouts) { $layouts = 'us' }
[IO.File]::WriteAllText("$LO\config\keyboard", $layouts)
Write-Host "Keyboard: $layouts"
$tzmap = @{ 'Turkey Standard Time'='Europe/Istanbul'; 'GMT Standard Time'='Europe/London'; 'W. Europe Standard Time'='Europe/Berlin';
  'Romance Standard Time'='Europe/Paris'; 'Central Europe Standard Time'='Europe/Budapest'; 'E. Europe Standard Time'='Europe/Bucharest';
  'Russian Standard Time'='Europe/Moscow'; 'Eastern Standard Time'='America/New_York'; 'Central Standard Time'='America/Chicago';
  'Mountain Standard Time'='America/Denver'; 'Pacific Standard Time'='America/Los_Angeles'; 'Tokyo Standard Time'='Asia/Tokyo';
  'China Standard Time'='Asia/Shanghai'; 'India Standard Time'='Asia/Kolkata'; 'Arabian Standard Time'='Asia/Dubai';
  'AUS Eastern Standard Time'='Australia/Sydney'; 'UTC'='Etc/UTC' }
$tz = $tzmap[(Get-TimeZone).Id]; if (-not $tz) { $tz = 'Etc/UTC' }
[IO.File]::WriteAllText("$LO\config\timezone", $tz)
Write-Host "Time zone: $tz"
if (-not $NoPin -and -not (Test-Path "$LO\config\pin")) {
  do {
    $p1 = Read-Host 'Choose a Lights Out PIN (4+ digits, used when Lights Out starts without Windows)' -AsSecureString
    $p2 = Read-Host 'Repeat the PIN' -AsSecureString
    $a = [Runtime.InteropServices.Marshal]::PtrToStringBSTR([Runtime.InteropServices.Marshal]::SecureStringToBSTR($p1))
    $b = [Runtime.InteropServices.Marshal]::PtrToStringBSTR([Runtime.InteropServices.Marshal]::SecureStringToBSTR($p2))
  } while ($a.Length -lt 4 -or $a -ne $b)
  $salt = New-Object byte[] 16; [Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($salt)
  $sha = [Security.Cryptography.SHA256]::Create()
  $hash = $sha.ComputeHash([byte[]]($salt + [Text.Encoding]::UTF8.GetBytes($a)))
  $hex = { param($x) -join ($x | ForEach-Object { $_.ToString('x2') }) }
  [IO.File]::WriteAllText("$LO\config\pin", "$(& $hex $salt):$(& $hex $hash)")
  $a = $null; $b = $null
}
# Only you (and admins) may read the handover folder
icacls $LO /inheritance:r /grant:r "$($env:USERNAME):(OI)(CI)F" "Administrators:(OI)(CI)F" "SYSTEM:(OI)(CI)F" | Out-Null

# ---------------------------------------------------------------- 4. BitLocker key for Lights Out
$handoff = $null
if ($encrypted) {
  Step 'Adding a dedicated Lights Out key to BitLocker'
  $oldId = if (Test-Path "$LO\config\protector-id") { (Get-Content "$LO\config\protector-id").Trim() } else { $null }
  if ($oldId) { try { Remove-BitLockerKeyProtector -MountPoint 'C:' -KeyProtectorId $oldId | Out-Null } catch {} }
  $before = (Get-BitLockerVolume -MountPoint 'C:').KeyProtector | ForEach-Object KeyProtectorId
  Add-BitLockerKeyProtector -MountPoint 'C:' -RecoveryPasswordProtector -WarningAction SilentlyContinue | Out-Null
  $new = (Get-BitLockerVolume -MountPoint 'C:').KeyProtector | Where-Object { $before -notcontains $_.KeyProtectorId -and $_.KeyProtectorType -eq 'RecoveryPassword' } | Select-Object -First 1
  if (-not $new) { Fail 'could not add the Lights Out key' }
  [IO.File]::WriteAllText("$LO\config\protector-id", $new.KeyProtectorId)
  $handoff = $new.RecoveryPassword
  Write-Host 'done (sealed into the TPM at the first Lights Out start)'
}
$handoff = $null   # handed over per switch by enter.ps1, never stored at install

# ---------------------------------------------------------------- 5. EFI partition + boot entry
Step 'Installing the starter on the EFI partition'
$src = Join-Path $Source 'starter\EFI\lightsout'
$needMB = [math]::Ceiling(((Get-ChildItem $src -File | Measure-Object Length -Sum).Sum) / 1MB)
$letter = (68..90 | ForEach-Object { [char]$_ } | Where-Object { -not (Test-Path "$($_):\") } | Select-Object -Last 1)
mountvol "$($letter):" /S
$esp = "$($letter):"
try {
  $freeMB = [math]::Floor((New-Object IO.DriveInfo("$letter")).AvailableFreeSpace / 1MB)
  $existing = 0; if (Test-Path "$esp\EFI\lightsout") { $existing = [math]::Ceiling(((Get-ChildItem "$esp\EFI\lightsout" -Recurse -File | Measure-Object Length -Sum).Sum) / 1MB) }
  if ($freeMB + $existing - $needMB -lt $MinEspFreeMB) { Fail "not enough room on the EFI partition ($freeMB MB free)" }
  New-Item -ItemType Directory -Force "$esp\EFI\lightsout" | Out-Null
  # old test files from earlier experiments
  Remove-Item "$esp\EFI\lightsout\initrd.img", "$esp\EFI\lightsout\probe.conf", "$esp\EFI\lightsout\mode.txt", "$esp\EFI\lightsout\claude-credentials.json", "$esp\EFI\lightsout\gh-token" -Force -ErrorAction SilentlyContinue
  Copy-Item "$src\*" "$esp\EFI\lightsout\" -Force
  if (-not (Test-Path "$esp\EFI\debian") -or (Test-Path "$esp\EFI\lightsout\created-debian-dir")) {
    New-Item -ItemType Directory -Force "$esp\EFI\debian" | Out-Null
    Copy-Item "$src\grub.cfg" "$esp\EFI\debian\grub.cfg" -Force
    Set-Content "$esp\EFI\lightsout\created-debian-dir" 'yes'
  }
  # a new key was made above: the old sealed copy is useless now
  if ($encrypted) { Remove-Item "$esp\EFI\lightsout\seal", "$esp\EFI\lightsout\handoff.key", "$esp\EFI\lightsout\handoff.nonce" -Recurse -Force -ErrorAction SilentlyContinue }
} finally { mountvol "$($letter):" /D }
$handoff = $null

$id = $null; $cur = $null
$fw = bcdedit /enum firmware
for ($i = 0; $i -lt $fw.Count; $i++) {
  if ($fw[$i] -match '^identifier\s+(\{[0-9a-f-]+\})') { $cur = $Matches[1] }
  if ($fw[$i] -match '^description\s+Lights Out$') { $id = $cur }
}
if (-not $id) {
  $created = bcdedit /copy '{bootmgr}' /d 'Lights Out'
  if (($created -join ' ') -match '(\{[0-9a-f-]{36}\})') { $id = $Matches[1] } else { Fail "could not create the boot entry: $created" }
}
bcdedit /set $id path '\EFI\lightsout\shimx64.efi' | Out-Null
bcdedit /set '{fwbootmgr}' displayorder $id /addlast | Out-Null   # Windows stays first
[IO.File]::WriteAllText("$LO\config\boot-entry", $id)

# ---------------------------------------------------------------- 6. terminal + switch task
Step 'Installing the Lights Out terminal'
Get-Process lightsout-app -ErrorAction SilentlyContinue | Stop-Process -Force
# a running lightsout.exe (Lights Out kept running in the background for a terminal) can't be
# overwritten, but it can be moved aside: it keeps running, its agents too; the new one is used next time
Get-ChildItem "$LO\bin\*.old" -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
foreach ($exe in 'lightsout.exe', 'lightsout-app.exe') {
  $cur = Join-Path "$LO\bin" $exe
  if (Test-Path $cur) {
    try { Remove-Item $cur -Force -ErrorAction Stop } catch { Move-Item $cur "$cur.$([DateTime]::Now.Ticks).old" -Force }
  }
}
Copy-Item (Join-Path $Source 'term\lightsout.exe'), (Join-Path $Source 'term\lightsout-app.exe') "$LO\bin\" -Force
$un = Join-Path $Source 'term\uninstall.ps1'
if (-not (Test-Path $un) -and $PSScriptRoot) { $un = Join-Path $PSScriptRoot 'uninstall.ps1' }
if (Test-Path $un) { Copy-Item $un "$LO\bin\uninstall.ps1" -Force }
# `lightsout` opens the app from any terminal (added to your PATH)
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if (($userPath -split ';') -notcontains "$LO\bin") {
  [Environment]::SetEnvironmentVariable('Path', ($userPath.TrimEnd(';') + ";$LO\bin"), 'User')
}
$enter = @"
# Runs as SYSTEM when the terminal asks to switch: pass the one-time
# "came from Windows" ticket to the EFI partition, boot Lights Out once, restart.
`$letter = (68..90 | ForEach-Object { [char]`$_ } | Where-Object { -not (Test-Path "`$(`$_):\") } | Select-Object -Last 1)
mountvol "`$(`$letter):" /S
try {
  Copy-Item 'C:\LightsOut\handoff\nonce' "`$(`$letter):\EFI\lightsout\handoff.nonce" -Force
  # Hand the Lights Out drive key over for this one start (the starter deletes it
  # right away and re-seals it), so switching keeps working after Windows or
  # firmware updates change the TPM measurements.
  `$protId = Get-Content 'C:\LightsOut\config\protector-id' -ErrorAction SilentlyContinue
  if (`$protId) {
    `$k = (Get-BitLockerVolume -MountPoint 'C:').KeyProtector | Where-Object { `$_.KeyProtectorId -eq `$protId.Trim() }
    if (`$k) { [IO.File]::WriteAllText("`$(`$letter):\EFI\lightsout\handoff.key", `$k.RecoveryPassword) }
  }
} finally { mountvol "`$(`$letter):" /D }
bcdedit /set '{fwbootmgr}' bootsequence '$id' | Out-Null
shutdown /r /t 0 /c 'Switching to Lights Out'
"@
[IO.File]::WriteAllText("$LO\bin\enter.ps1", $enter)
schtasks /create /tn '\LightsOut\Enter' /tr "powershell.exe -NoProfile -ExecutionPolicy Bypass -File $LO\bin\enter.ps1" /sc once /st 00:00 /sd 01/01/2000 /ru SYSTEM /rl HIGHEST /f | Out-Null
# allow the signed-in user to start (only) this task
$svc = New-Object -ComObject Schedule.Service; $svc.Connect()
$svc.GetFolder('\LightsOut').GetTask('Enter').SetSecurityDescriptor('D:(A;;FA;;;BA)(A;;FA;;;SY)(A;;GRGX;;;AU)', 0)
# Safety: at every Windows start, remove a leftover key or "came from Windows" pass
# from the boot partition (e.g. if a switch was interrupted).
$cleanup = @"
`$letter = (68..90 | ForEach-Object { [char]`$_ } | Where-Object { -not (Test-Path "`$(`$_):\") } | Select-Object -Last 1)
mountvol "`$(`$letter):" /S
try { Remove-Item "`$(`$letter):\EFI\lightsout\handoff.key", "`$(`$letter):\EFI\lightsout\handoff.nonce" -Force -ErrorAction SilentlyContinue }
finally { mountvol "`$(`$letter):" /D }
"@
[IO.File]::WriteAllText("$LO\bin\cleanup.ps1", $cleanup)
schtasks /create /tn '\LightsOut\Cleanup' /tr "powershell.exe -NoProfile -ExecutionPolicy Bypass -File $LO\bin\cleanup.ps1" /sc onstart /ru SYSTEM /rl HIGHEST /f | Out-Null
$lnk = (New-Object -ComObject WScript.Shell).CreateShortcut("$env:ProgramData\Microsoft\Windows\Start Menu\Programs\Lights Out.lnk")
$lnk.TargetPath = "$LO\bin\lightsout-app.exe"; $lnk.WorkingDirectory = $env:USERPROFILE; $lnk.Save()

Write-Host "`nLights Out is installed." -ForegroundColor Green
Write-Host 'Open it with the command  lightsout  (in a new terminal) or from the Start menu.'
Write-Host 'The Lights Out button (bottom left, or Ctrl+Shift+L) switches over.'
Write-Host 'Remove it with:  lightsout uninstall'
Stop-Transcript | Out-Null
if ($Source -eq (Join-Path $env:TEMP 'lightsout-setup')) { Remove-Item $Source -Recurse -Force -ErrorAction SilentlyContinue }
