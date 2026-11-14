# Feasibility probe: can a disposable GitHub Windows runner host a genuine
# loopback RDP session, and can rust-mirror-gui run and be captured inside it?
#
# Enables Remote Desktop, creates a temporary local user (random password,
# never logged), installs the FreeRDP client from MSYS2 (signature-verified
# packages; versions and hashes recorded), connects to 127.0.0.2 pinning the
# server's RDP certificate by SHA-256, then runs the app and a PrintWindow
# capture inside that session through a session-scoped scheduled task.
# cleanup.ps1 removes the user, session, task and files.
param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [Parameter(Mandatory = $true)][string]$Out
)
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force $Out | Out-Null
$Share = 'C:\rdpqual'
New-Item -ItemType Directory -Force "$Share\app", "$Share\out" | Out-Null
function Log([string]$m) {
    $line = "$((Get-Date).ToString('HH:mm:ss.fff')) $m"
    Write-Output $line
    Add-Content -Path "$Out\probe.log" -Value $line
}

Log "OS: $((Get-CimInstance Win32_OperatingSystem).Caption) $((Get-CimInstance Win32_OperatingSystem).BuildNumber)"
Log "Runner identity: $(whoami); this process is in session $([System.Diagnostics.Process]::GetCurrentProcess().SessionId)"
Log "Sessions before:`n$(qwinsta | Out-String)"
Get-Process Runner.Listener, Runner.Worker -ErrorAction SilentlyContinue | ForEach-Object { Log "$($_.Name) in session $($_.SessionId)" }

# 1. Remote Desktop on (disposable runner only).
Set-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Terminal Server' -Name fDenyTSConnections -Value 0
try { Enable-NetFirewallRule -DisplayGroup 'Remote Desktop' } catch { Log "firewall rule: $($_.Exception.Message)" }
Start-Service TermService
Log "TermService: $((Get-Service TermService).Status)"
Start-Sleep -Seconds 3
Log "Listening on 3389: $((Get-NetTCPConnection -LocalPort 3389 -State Listen -ErrorAction SilentlyContinue | Measure-Object).Count -gt 0)"

# 2. Temporary test user, random password (masked, never written anywhere).
$bytes = New-Object byte[] 24
[System.Security.Cryptography.RandomNumberGenerator]::Fill($bytes)
$pw = [Convert]::ToBase64String($bytes).Replace('+', 'x').Replace('/', 'y') + 'aA1!'
Write-Output "::add-mask::$pw"
New-LocalUser -Name rdpqual -Password (ConvertTo-SecureString $pw -AsPlainText -Force) -PasswordNeverExpires -AccountNeverExpires -Description 'temporary rust-mirror-gui RDP qualification user' | Out-Null
Add-LocalGroupMember -Group 'Remote Desktop Users' -Member rdpqual
Log "Created local user rdpqual (Remote Desktop Users)"
icacls $Share /grant 'rdpqual:(OI)(CI)M' | Out-Null
Copy-Item $Exe "$Share\app\rust-mirror-gui.exe"
Copy-Item "$PSScriptRoot\..\capture-window.ps1" "$Share\app\capture-window.ps1"

# 3. FreeRDP client from MSYS2 (pacman verifies package signatures).
$pacman = 'C:\msys64\usr\bin\pacman.exe'
if (-not (Test-Path $pacman)) { throw "MSYS2 is not installed on this image" }
& $pacman -Sy --noconfirm --needed mingw-w64-ucrt-x86_64-freerdp 2>&1 | ForEach-Object { Add-Content "$Out\pacman.log" $_ }
Log "pacman exit $LASTEXITCODE"
& $pacman -Q mingw-w64-ucrt-x86_64-freerdp | ForEach-Object { Log "Installed: $_" }
$exes = & $pacman -Ql mingw-w64-ucrt-x86_64-freerdp | Where-Object { $_ -match '\.exe$' } | ForEach-Object { ($_ -split ' ', 2)[1] }
Log "FreeRDP executables: $($exes -join ', ')"
Get-ChildItem 'C:\msys64\var\cache\pacman\pkg\*freerdp*' -ErrorAction SilentlyContinue | ForEach-Object { Log "package $($_.Name) sha256 $((Get-FileHash $_.FullName).Hash)" }
$client = $null
foreach ($name in 'wfreerdp.exe', 'sdl-freerdp3.exe', 'sdl-freerdp.exe', 'wfreerdp3.exe') {
    $p = "C:\msys64\ucrt64\bin\$name"
    if (Test-Path $p) { $client = $p; break }
}
if (-not $client) { throw "no FreeRDP client executable found" }
Log "Client: $client"

# 4. Pin the RDP listener's certificate by SHA-256 (no certificate is ignored).
$cert = Get-ChildItem 'Cert:\LocalMachine\Remote Desktop' | Select-Object -First 1
$fp = (([System.Security.Cryptography.SHA256]::Create().ComputeHash($cert.RawData)) | ForEach-Object { $_.ToString('x2') }) -join ':'
Log "RDP certificate $($cert.Subject), SHA-256 $fp"

# 5. Connect.
$env:PATH = "C:\msys64\ucrt64\bin;$env:PATH"
$rdp = Start-Process -FilePath $client -PassThru -ArgumentList '/v:127.0.0.2', '/u:rdpqual', "/p:$pw", "/cert:fingerprint:sha256:$fp", '/size:1280x800', '/log-level:INFO' -RedirectStandardOutput "$Out\freerdp.out" -RedirectStandardError "$Out\freerdp.err"
$deadline = (Get-Date).AddSeconds(90)
$sessionId = $null
while ((Get-Date) -lt $deadline) {
    $line = qwinsta rdpqual 2>$null | Select-Object -Skip 1 | Select-Object -First 1
    if ($line -match '\s(\d+)\s+Active') { $sessionId = [int]$Matches[1]; break }
    if ($rdp.HasExited) { break }
    Start-Sleep -Seconds 1
}
Log "Sessions after connecting:`n$(qwinsta | Out-String)"
Log "FreeRDP running: $(-not $rdp.HasExited)"
if (-not $sessionId) {
    Get-Content "$Out\freerdp.err" -ErrorAction SilentlyContinue | Select-Object -Last 30 | ForEach-Object { Log "freerdp: $_" }
    throw "no active RDP session for rdpqual"
}
Log "rdpqual is active in RDP session $sessionId"
Start-Sleep -Seconds 10  # let the logon finish (profile, shell)

# 6. Run the app and a capture inside that session.
@"
`$ErrorActionPreference = 'Continue'
"session `$([System.Diagnostics.Process]::GetCurrentProcess().SessionId), user `$env:USERNAME, SESSIONNAME `$env:SESSIONNAME" | Out-File C:\rdpqual\out\insession.txt
Add-Type -AssemblyName System.Windows.Forms
[System.Windows.Forms.Screen]::AllScreens | ForEach-Object { "screen `$(`$_.DeviceName) `$(`$_.Bounds.Width)x`$(`$_.Bounds.Height)" } | Out-File -Append C:\rdpqual\out\insession.txt
`$t = Measure-Command { & C:\rdpqual\app\capture-window.ps1 -Exe C:\rdpqual\app\rust-mirror-gui.exe -Out C:\rdpqual\out\rdp-window.png -WaitSeconds 10 *>&1 | Out-File -Append C:\rdpqual\out\insession.txt }
"capture script took `$([int]`$t.TotalSeconds) s" | Out-File -Append C:\rdpqual\out\insession.txt
"done" | Out-File C:\rdpqual\out\done.txt
"@ | Set-Content "$Share\app\insession.ps1"
$action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument '-NoProfile -ExecutionPolicy Bypass -File C:\rdpqual\app\insession.ps1'
$principal = New-ScheduledTaskPrincipal -UserId rdpqual -LogonType Interactive
Register-ScheduledTask -TaskName rdpqual-probe -Action $action -Principal $principal | Out-Null
Start-ScheduledTask -TaskName rdpqual-probe
$deadline = (Get-Date).AddSeconds(150)
while (-not (Test-Path "$Share\out\done.txt") -and (Get-Date) -lt $deadline) { Start-Sleep -Seconds 2 }
Log "Scheduled task: $((Get-ScheduledTaskInfo rdpqual-probe | Format-List LastRunTime, LastTaskResult | Out-String).Trim())"
Get-Content "$Share\out\insession.txt" -ErrorAction SilentlyContinue | ForEach-Object { Log "in-session: $_" }
Copy-Item "$Share\out\*" $Out -ErrorAction SilentlyContinue
if (-not (Test-Path "$Share\out\done.txt")) { throw "the in-session script did not finish" }
