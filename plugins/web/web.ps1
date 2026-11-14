# Web plugin (Windows): draws a web page into its panel with Edge or Chrome, without a
# window, only when asked (open, reload, the panel's size changed). Nothing keeps running.
param([string]$Action)
$ErrorActionPreference = 'SilentlyContinue'
$state = $env:LIGHTSOUT_PLUGIN_STATE
$urlFile = Join-Path $state 'url.txt'
$sizeFile = Join-Path $state 'size.txt'
$part = 'web.page'

function Say([string]$text) { lightsout part set $part $text | Out-Null }

function Browser {
    $paths = @(
        "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe",
        "$env:ProgramFiles\Microsoft\Edge\Application\msedge.exe",
        "$env:ProgramFiles\Google\Chrome\Application\chrome.exe",
        "$env:LOCALAPPDATA\Google\Chrome\Application\chrome.exe"
    )
    $paths | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
}

# draws the page at the panel's size (known once the panel shows: part.resized)
function Draw {
    $url = Get-Content $urlFile
    if (-not $url) { return }
    $size = Get-Content $sizeFile
    if (-not $size) { Say "Loading $url ..."; return }
    $w, $h = $size -split ' '
    if ($w -eq 'cells') {
        Say "$url`nThe page shows in the Lights Out window (lightsout --window)."
        return
    }
    $exe = Browser
    if (-not $exe) { Say "No Edge or Chrome to draw the page with.`nlightsout do web.browser opens it in your browser."; return }
    $png = Join-Path $state 'page.png'
    Remove-Item $png
    $dataDir = Join-Path $state 'profile'
    $p = Start-Process -FilePath $exe -PassThru -WindowStyle Hidden -ArgumentList @(
        '--headless=new', '--disable-gpu', '--hide-scrollbars', '--no-first-run',
        "--user-data-dir=`"$dataDir`"", "--window-size=$w,$($h * 4)", '--virtual-time-budget=4000',
        "--screenshot=`"$png`"", $url)
    $p | Wait-Process -Timeout 30
    if (Test-Path $png) { lightsout part image $part $png | Out-Null }
    else { Say "Couldn't draw $url" }
}

switch ($Action) {
    'open' {
        $url = "$env:LIGHTSOUT_ARG".Trim()
        if (-not $url) { $url = Get-Content $urlFile }
        if (-not $url) { Say 'lightsout do web.open https://...'; return }
        if ($url -notmatch '^https?://') { $url = "https://$url" }
        Set-Content -Path $urlFile -Value $url
        if (Test-Path $sizeFile) { Draw } else { Say "Loading $url ..." }
    }
    'reload' { Draw }
    'close' {
        lightsout part clear $part | Out-Null
        Remove-Item $sizeFile
    }
    { $_ -in 'browser', 'clicked' } {
        if ($Action -eq 'clicked' -and $env:LIGHTSOUT_EVENT_JSON -notmatch '"part":"web\.page"') { return }
        $url = Get-Content $urlFile
        if ($url) { Start-Process $url }
    }
    'resized' {
        $e = $env:LIGHTSOUT_EVENT_JSON | ConvertFrom-Json
        if ($e.part -ne $part) { return }
        # in a terminal the size is in cells: no picture shows there
        if ($e.width -eq $e.cols) { Set-Content $sizeFile 'cells 0' }
        else { Set-Content $sizeFile "$($e.width) $($e.height)" }
        Draw
    }
}
