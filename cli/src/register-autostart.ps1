$ErrorActionPreference = 'Stop'
$exe = $env:TRONTEQ_GUI_EXE
if (!$exe -or !(Test-Path -LiteralPath $exe -PathType Leaf)) {
    throw 'TrontEQ GUI executable is missing; build/install it before registering startup.'
}
$exe = (Resolve-Path -LiteralPath $exe).ProviderPath
$user = [Security.Principal.WindowsIdentity]::GetCurrent().Name
$action = New-ScheduledTaskAction -Execute $exe -Argument '--startup' -WorkingDirectory (Split-Path -Parent $exe)
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $user
$trigger.Delay = 'PT10S'
$principal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Limited
$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
    -StartWhenAvailable -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew `
    -RestartCount 10 -RestartInterval (New-TimeSpan -Minutes 1)
Register-ScheduledTask -TaskName 'TrontEQ' -Action $action -Trigger $trigger -Principal $principal `
    -Settings $settings -Description 'TrontEQ system tray. Starts at sign-in and retries after failures.' -Force | Out-Null
