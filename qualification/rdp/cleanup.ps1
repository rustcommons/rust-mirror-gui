# Remove everything the RDP qualification created on the disposable runner:
# the test user's sessions, processes and scheduled tasks, the FreeRDP client
# processes, the user account and its profile, and C:\rdpqual. Safe to run
# more than once.
$ErrorActionPreference = 'Continue'
foreach ($line in (qwinsta rdpqual 2>$null | Select-Object -Skip 1)) {
    if ($line -match '\s(\d+)\s+(Active|Disc|Conn)') {
        logoff $Matches[1]
        Write-Output "logged off session $($Matches[1])"
    }
}
Get-Process wfreerdp, sdl-freerdp3, sdl-freerdp, wfreerdp3 -ErrorAction SilentlyContinue | Stop-Process -Force
Get-ScheduledTask -TaskName 'rdpqual-*' -ErrorAction SilentlyContinue | Unregister-ScheduledTask -Confirm:$false
Start-Sleep -Seconds 5
if (Get-LocalUser -Name rdpqual -ErrorAction SilentlyContinue) {
    Remove-LocalUser -Name rdpqual
    Write-Output "removed local user rdpqual"
}
Get-CimInstance Win32_UserProfile | Where-Object { $_.LocalPath -like '*\rdpqual*' } | ForEach-Object {
    Remove-CimInstance $_
    Write-Output "removed profile $($_.LocalPath)"
}
Remove-Item -Recurse -Force C:\rdpqual -ErrorAction SilentlyContinue
Write-Output "Sessions after cleanup:"
qwinsta
