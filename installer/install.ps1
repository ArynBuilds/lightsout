# Lights Out installer for Windows:  irm https://lightsout.sh | iex
# (asks for admin rights; downloads everything from the latest release). Run again, it
# updates (`lightsout update` does that): settings, logins and installed programs stay.
#   -Source  optional local folder instead of downloading: starter\EFI\lightsout\*,
#            system\system.img.gz(+.sha256), term\lightsout.exe + lightsout-app.exe + cleanup.ps1 + uninstall.ps1
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
# stable = the latest release; rc = the newest test build (lightsout.sh/rc sends this installer with 'rc' here)
$Channel = 'stable'
$Site = if ($Channel -eq 'rc') { 'https://lightsout.sh/rc' } else { 'https://lightsout.sh' }
# admin rights: reopen this installer in an admin window
if (-not ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
  Write-Host 'Lights Out needs admin rights to install. Confirm the Windows prompt...'
  if ($PSCommandPath) {
    $a = "-NoProfile -ExecutionPolicy Bypass -NoExit -File `"$PSCommandPath`""
    if ($Source) { $a += " -Source `"$Source`"" }
  } else {
    $a = "-NoProfile -ExecutionPolicy Bypass -NoExit -Command `"irm $Site | iex`""
  }
  Start-Process powershell -Verb RunAs -ArgumentList $a
  return
}
# (lightsout.sh/rc sends this installer already pointed at the newest test build)
if ($Channel -eq 'rc') { Write-Host "Test build: $(Split-Path $Base -Leaf)" -ForegroundColor Yellow }
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
function Download($f, $to) { Write-Host "  $f"; curl.exe -fL --progress-bar -o $to "$Base/$f"; if ($LASTEXITCODE -ne 0) { Fail "could not download $f" } }
if (-not $Source) {
  Step 'Downloading Lights Out'
  $Source = Join-Path $env:TEMP 'lightsout-setup'
  Remove-Item $Source -Recurse -Force -ErrorAction SilentlyContinue
  New-Item -ItemType Directory -Force "$Source\system", "$Source\term", "$Source\starter" | Out-Null
  Download 'lightsout-windows-x64.zip' "$Source\lightsout-windows-x64.zip"
  Download 'lightsout-starter.zip' "$Source\lightsout-starter.zip"
  Download 'system.img.gz.sha256' "$Source\system\system.img.gz.sha256"
  Expand-Archive "$Source\lightsout-windows-x64.zip" "$Source\term" -Force
  Expand-Archive "$Source\lightsout-starter.zip" "$Source\starter" -Force
}
# An update: your own space (user.img: programs, logins, keys) always stays. The system
# file is replaced only when it changed; the previous one is kept until the new one has
# started in Lights Out (else Windows puts it back, see cleanup.ps1).
$updating = Test-Path "$LO\system.img"
$gz = Join-Path $Source 'system\system.img.gz'
$want = (Get-Content "$gz.sha256" -ErrorAction SilentlyContinue | Select-Object -First 1) -split '\s+' | Select-Object -First 1
$newSystem = -not $updating -or -not $want -or $want -ne (Get-Content "$LO\config\system-sha256" -ErrorAction SilentlyContinue)
if ($newSystem -and -not (Test-Path $gz)) { Download 'system.img.gz' $gz }
$need = 'starter\EFI\lightsout\shimx64.efi','starter\EFI\lightsout\starter.img','term\lightsout.exe','term\lightsout-app.exe','term\cleanup.ps1'
if ($newSystem) { $need += 'system\system.img.gz' }
foreach ($f in $need) {
  if (-not (Test-Path (Join-Path $Source $f))) { Fail "missing $f in $Source" }
}
# run by Windows itself (an update asked for inside Lights Out): your settings stay as they are
$asSystem = [Security.Principal.WindowsIdentity]::GetCurrent().IsSystem
$user = if ($asSystem) { (Get-Content "$LO\config\windows-user").Trim() } else { $env:USERNAME }
$prev = "$LO\update\previous"
$keepPrev = Test-Path "$LO\update\trial"   # an update not started yet: its backup is the last good version
if ($updating -and -not $keepPrev) {
  Remove-Item $prev -Recurse -Force -ErrorAction SilentlyContinue
  New-Item -ItemType Directory -Force "$prev\starter" | Out-Null
  Copy-Item "$LO\config\system-sha256" $prev -ErrorAction SilentlyContinue
}
$freeGB = [math]::Round((Get-PSDrive C).Free / 1GB, 1)
if ($freeGB -lt 6) { Fail "C: has $freeGB GB free; Lights Out needs about 6 GB" }
$bl = Get-BitLockerVolume -MountPoint 'C:' -ErrorAction SilentlyContinue
$encrypted = $bl -and $bl.ProtectionStatus -eq 'On'
Write-Host "C: free: $freeGB GB, BitLocker: $(if ($encrypted) {'on'} else {'off'})"
# the TPM chip is only needed to unlock an encrypted drive without typing a key
if ($encrypted -and -not $tpmOk) { Fail 'C: is encrypted (BitLocker) but this PC has no ready TPM security chip' }

# ---------------------------------------------------------------- 2. system file
Step 'Unpacking the Lights Out system (a few GB on C:)'
if (-not $newSystem) { Write-Host 'the system file is unchanged' }
else {
  if ($want) {
    if ($want -ne (Get-FileHash $gz -Algorithm SHA256).Hash.ToLower()) { Fail 'the system download is damaged (checksum mismatch)' }
    Write-Host 'checksum ok'
  }
  $in = [IO.File]::OpenRead($gz)
  $z = New-Object IO.Compression.GZipStream($in, [IO.Compression.CompressionMode]::Decompress)
  $out = [IO.File]::Create("$LO\system.img.part")
  try { $z.CopyTo($out, 4MB) } finally { $out.Close(); $z.Close(); $in.Close() }
  if ($updating -and -not $keepPrev) { Move-Item "$LO\system.img" "$prev\system.img" -Force }
  Move-Item "$LO\system.img.part" "$LO\system.img" -Force
  [IO.File]::WriteAllText("$LO\config\system-sha256", "$want")
}
[IO.File]::WriteAllText("$LO\bin\installer-url", $Site)   # where `lightsout update` gets the next version

# ---------------------------------------------------------------- 3. Wi-Fi, user, PIN
Step 'Saving your Wi-Fi networks and settings for Lights Out'
Remove-Item "$LO\wifi\*" -Force -ErrorAction SilentlyContinue
netsh wlan export profile key=clear folder="$LO\wifi" | Out-Null
Write-Host "Wi-Fi networks: $((Get-ChildItem "$LO\wifi" -Filter *.xml).Count)"
[IO.File]::WriteAllText("$LO\config\windows-user", $user)
# keyboard layout(s) for Lights Out, e.g. "tr" or "us,tr" (yours: not read when Windows runs the update)
if (-not $asSystem) {
  $xkb = @{ 'en-US'='us'; 'en-GB'='gb'; 'tr-TR'='tr'; 'de-DE'='de'; 'fr-FR'='fr'; 'es-ES'='es'; 'it-IT'='it'; 'pt-BR'='br'; 'pt-PT'='pt'; 'nl-NL'='nl'; 'ru-RU'='ru'; 'pl-PL'='pl'; 'sv-SE'='se'; 'da-DK'='dk'; 'nb-NO'='no'; 'fi-FI'='fi'; 'ja-JP'='jp'; 'ko-KR'='kr' }
  $layouts = (Get-WinUserLanguageList | ForEach-Object { $xkb[$_.LanguageTag] } | Where-Object { $_ } | Select-Object -Unique) -join ','
  if (-not $layouts) { $layouts = 'us' }
  [IO.File]::WriteAllText("$LO\config\keyboard", $layouts)
  Write-Host "Keyboard: $layouts"
}
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
icacls $LO /inheritance:r /grant:r "$($user):(OI)(CI)F" "Administrators:(OI)(CI)F" "SYSTEM:(OI)(CI)F" | Out-Null
# Windows runs the scripts and the update address in bin as SYSTEM: only admins may change them
icacls "$LO\bin" /inheritance:r /grant:r "$($user):(OI)(CI)RX" "Administrators:(OI)(CI)F" "SYSTEM:(OI)(CI)F" | Out-Null

# ---------------------------------------------------------------- 4. BitLocker key for Lights Out
if ($encrypted) {
  Step 'Adding a dedicated Lights Out key to BitLocker'
  $oldId = if (Test-Path "$LO\config\protector-id") { (Get-Content "$LO\config\protector-id").Trim() } else { $null }
  if ($oldId) { try { Remove-BitLockerKeyProtector -MountPoint 'C:' -KeyProtectorId $oldId | Out-Null } catch {} }
  $before = (Get-BitLockerVolume -MountPoint 'C:').KeyProtector | ForEach-Object KeyProtectorId
  Add-BitLockerKeyProtector -MountPoint 'C:' -RecoveryPasswordProtector -WarningAction SilentlyContinue | Out-Null
  $new = (Get-BitLockerVolume -MountPoint 'C:').KeyProtector | Where-Object { $before -notcontains $_.KeyProtectorId -and $_.KeyProtectorType -eq 'RecoveryPassword' } | Select-Object -First 1
  if (-not $new) { Fail 'could not add the Lights Out key' }
  [IO.File]::WriteAllText("$LO\config\protector-id", $new.KeyProtectorId)
  Write-Host 'done (sealed into the TPM at the first Lights Out start)'
}

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
  if ($updating -and -not $keepPrev) {   # the previous starter, never the one-time key files
    Get-ChildItem "$esp\EFI\lightsout" -File -Exclude 'handoff.*' | Copy-Item -Destination "$prev\starter"
  }
  Copy-Item "$src\*" "$esp\EFI\lightsout\" -Force
  if ($updating) { New-Item -ItemType File -Force "$LO\update\trial" | Out-Null }
  # the starter's boot loader is Fedora's, which reads \EFI\fedora\grub.cfg (left alone if Fedora is installed)
  if (-not (Test-Path "$esp\EFI\fedora") -or (Test-Path "$esp\EFI\lightsout\created-fedora-dir")) {
    New-Item -ItemType Directory -Force "$esp\EFI\fedora" | Out-Null
    Copy-Item "$src\grub.cfg" "$esp\EFI\fedora\grub.cfg" -Force
    Set-Content "$esp\EFI\lightsout\created-fedora-dir" 'yes'
  }
  # a new key was made above: the old sealed copy is useless now
  if ($encrypted) { Remove-Item "$esp\EFI\lightsout\seal", "$esp\EFI\lightsout\handoff.key", "$esp\EFI\lightsout\handoff.nonce" -Recurse -Force -ErrorAction SilentlyContinue }
} finally { mountvol "$($letter):" /D }

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
$wasOpen = Get-Process lightsout-app -ErrorAction SilentlyContinue   # opened again at the end, tabs and agents back
$wasOpen | Stop-Process -Force
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
Copy-Item (Join-Path $Source 'term\cleanup.ps1') "$LO\bin\cleanup.ps1" -Force
$un = Join-Path $Source 'term\uninstall.ps1'
if (-not (Test-Path $un) -and $PSScriptRoot) { $un = Join-Path $PSScriptRoot 'uninstall.ps1' }
if (Test-Path $un) { Copy-Item $un "$LO\bin\uninstall.ps1" -Force }
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
# after an update: this start decides whether it stays (see cleanup.ps1)
if (Test-Path 'C:\LightsOut\update\trial') { New-Item -ItemType File -Force 'C:\LightsOut\update\tried' | Out-Null }
bcdedit /set '{fwbootmgr}' bootsequence '$id' | Out-Null
shutdown /r /t 0 /c 'Switching to Lights Out'
"@
[IO.File]::WriteAllText("$LO\bin\enter.ps1", $enter)
# Tasks, PATH, sign-in and Start menu entries: set up by you; Windows running an update keeps them
if (-not $asSystem) {
  # Windows' default only starts a task when plugged in; Lights Out must also work on battery
  $onBattery = { param($limit) New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit $limit }
  schtasks /create /tn '\LightsOut\Enter' /tr "powershell.exe -NoProfile -ExecutionPolicy Bypass -File $LO\bin\enter.ps1" /sc once /st 00:00 /sd 01/01/2000 /ru SYSTEM /rl HIGHEST /f | Out-Null
  Set-ScheduledTask -TaskPath '\LightsOut\' -TaskName Enter -Settings (& $onBattery (New-TimeSpan -Minutes 10)) | Out-Null
  # allow the signed-in user to start (only) this task
  $svc = New-Object -ComObject Schedule.Service; $svc.Connect()
  $svc.GetFolder('\LightsOut').GetTask('Enter').SetSecurityDescriptor('D:(A;;FA;;;BA)(A;;FA;;;SY)(A;;GRGX;;;AU)', 0)
  # at every Windows start (cleanup.ps1): leftovers of a switch, updates (an hour: a download may run)
  schtasks /create /tn '\LightsOut\Cleanup' /tr "powershell.exe -NoProfile -ExecutionPolicy Bypass -File $LO\bin\cleanup.ps1" /sc onstart /ru SYSTEM /rl HIGHEST /f | Out-Null
  Set-ScheduledTask -TaskPath '\LightsOut\' -TaskName Cleanup -Settings (& $onBattery (New-TimeSpan -Hours 1)) | Out-Null
  # `lightsout` opens the app from any terminal (added to your PATH)
  $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
  if (($userPath -split ';') -notcontains "$LO\bin") {
    [Environment]::SetEnvironmentVariable('Path', ($userPath.TrimEnd(';') + ";$LO\bin"), 'User')
  }
  # back from Lights Out: at sign-in the app opens and takes your tabs back (otherwise it stays closed)
  Set-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name 'Lights Out' -Value "`"$LO\bin\lightsout-app.exe`" --if-returning"
  $lnk = (New-Object -ComObject WScript.Shell).CreateShortcut("$env:ProgramData\Microsoft\Windows\Start Menu\Programs\Lights Out.lnk")
  $lnk.TargetPath = "$LO\bin\lightsout-app.exe"; $lnk.WorkingDirectory = $env:USERPROFILE; $lnk.Save()
  # it was open: open it again as you (not as admin); it takes its tabs back and continues the agents
  if ($wasOpen) { Start-Process explorer.exe "$LO\bin\lightsout-app.exe" }
}

Write-Host "`nLights Out is $(if ($updating) {'updated'} else {'installed'})." -ForegroundColor Green
Write-Host 'Open it with the command  lightsout  (in a new terminal) or from the Start menu.'
Write-Host 'The Lights Out button (bottom left, or Ctrl+Shift+L) switches over.'
Write-Host 'Remove it with:  lightsout uninstall'
Stop-Transcript | Out-Null
if ($Source -eq (Join-Path $env:TEMP 'lightsout-setup')) { Remove-Item $Source -Recurse -Force -ErrorAction SilentlyContinue }
