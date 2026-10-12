# Deploy the built GUI to a stable per-user location, then register logon startup.
# The installed APO and sound state are left in ProgramData.
$ErrorActionPreference = 'Stop'
$release = Join-Path $PSScriptRoot 'target/release'
if (Test-Path -LiteralPath (Join-Path $PSScriptRoot 'tronteq.exe')) {
    $release = $PSScriptRoot # extracted release ZIP
}
$install = Join-Path $env:LOCALAPPDATA 'Programs/TrontEQ'
foreach ($name in @('tronteq.exe', 'tronteq-cli.exe')) {
    if (!(Test-Path -LiteralPath (Join-Path $release $name))) {
        throw "Build the release first; missing $name"
    }
}
$installedGui = Join-Path $install 'tronteq.exe'
$running = Get-Process -Name tronteq -ErrorAction SilentlyContinue
if ($running) { throw 'Quit TrontEQ from its tray menu before updating the installed copy.' }
New-Item -ItemType Directory -Path $install -Force | Out-Null
foreach ($name in @('tronteq.exe', 'tronteq-cli.exe')) {
    Copy-Item -LiteralPath (Join-Path $release $name) -Destination (Join-Path $install $name) -Force
}
# Keep the already installed, signed APO available to the adjacent CLI for
# explicit future device installs. This does not register or load the DLL.
$installedApo = 'C:/ProgramData/TrontEq/TrontEqApo.dll'
if (Test-Path -LiteralPath $installedApo) {
    Copy-Item -LiteralPath $installedApo -Destination (Join-Path $install 'TrontEqApo.dll') -Force
}
& (Join-Path $install 'tronteq-cli.exe') register-autostart
if ($LASTEXITCODE -ne 0) {
    # An existing task created by an older elevated installer can require one
    # administrator consent. Only registration elevates; the GUI stays Limited.
    $registration = Start-Process -FilePath (Join-Path $install 'tronteq-cli.exe') `
        -ArgumentList 'register-autostart' -Verb RunAs -WindowStyle Hidden -PassThru
    $registration.WaitForExit()
    if ($registration.ExitCode -ne 0) { throw 'Startup registration failed' }
}
$shortcutPath = Join-Path ([Environment]::GetFolderPath('Programs')) 'TrontEQ.lnk'
$shortcut = (New-Object -ComObject WScript.Shell).CreateShortcut($shortcutPath)
$shortcut.TargetPath = $installedGui
$shortcut.WorkingDirectory = $install
$shortcut.IconLocation = "$installedGui,0"
$shortcut.Save()
Start-ScheduledTask -TaskName TrontEQ
Write-Output "Installed TrontEQ at $installedGui and started its logon task."
