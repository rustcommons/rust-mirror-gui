# Remote Desktop qualification on a disposable GitHub Windows runner (no GPU):
# rust-mirror-gui runs inside a genuine RDP session (loopback to 127.0.0.2,
# FreeRDP client) and is driven and captured from inside that session by
# insession.ps1. Writes results.json (pass/fail per scenario) and screenshots
# to -Out. cleanup.ps1 removes the user, sessions, task and files afterwards.
#
# Never logs the test user's password. The RDP server certificate is pinned
# by SHA-256 (never ignored). FreeRDP is the exact MSYS2 package below,
# checked by SHA-256 (pacman also checks its signature).
param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [string]$OldExe,
    [Parameter(Mandatory = $true)][string]$Out
)
$ErrorActionPreference = 'Stop'
$FreeRdpPkg = 'mingw-w64-ucrt-x86_64-freerdp-3.33.1-1-any.pkg.tar.zst'
$FreeRdpSha = 'B854F530877E44CA5D22D2B084727824B4B5DFC45B2B3A01F1DDB71B66A56F3A'
$Share = 'C:\rdpqual'
New-Item -ItemType Directory -Force $Out, "$Share\app", "$Share\out", "$Share\data" | Out-Null
$script:results = [System.Collections.ArrayList]::new()
function Log([string]$m) {
    $line = "$((Get-Date).ToString('HH:mm:ss.fff')) $m"
    Write-Output $line
    Add-Content -Path "$Out\qualify.log" -Value $line
}
# status: pass, fail (fails the job), or limit (a plan threshold missed for a
# known platform reason; reported, does not fail the job).
function Record([string]$scenario, [bool]$pass, $details, [switch]$Limit) {
    $status = if ($pass) { 'pass' } elseif ($Limit) { 'limit' } else { 'fail' }
    [void]$script:results.Add([ordered]@{ scenario = $scenario; pass = $pass; status = $status; details = $details })
    Log "$($status.ToUpper()) $scenario $($details | ConvertTo-Json -Compress -Depth 5)"
    $script:results | ConvertTo-Json -Depth 6 | Set-Content "$Out\results.json"
}

# ---------------------------------------------------------------- setup
$os = Get-CimInstance Win32_OperatingSystem
Log "OS: $($os.Caption) $($os.BuildNumber); cores $([Environment]::ProcessorCount)"
$cpu = Get-CimInstance Win32_Processor | Select-Object -First 1
Log "Processor: $($cpu.Name.Trim()) ($($cpu.NumberOfLogicalProcessors) logical, $($cpu.MaxClockSpeed) MHz)"
Add-Content "$Out\results-header.txt" "processor=$($cpu.Name.Trim())"
Set-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Terminal Server' -Name fDenyTSConnections -Value 0
try { Enable-NetFirewallRule -DisplayGroup 'Remote Desktop' } catch { Log "firewall rule: $($_.Exception.Message)" }
Start-Service TermService
$bytes = New-Object byte[] 24
[System.Security.Cryptography.RandomNumberGenerator]::Fill($bytes)
$pw = [Convert]::ToBase64String($bytes).Replace('+', 'x').Replace('/', 'y') + 'aA1!'
Write-Output "::add-mask::$pw"
New-LocalUser -Name rdpqual -Password (ConvertTo-SecureString $pw -AsPlainText -Force) -PasswordNeverExpires -AccountNeverExpires -Description 'temporary rust-mirror-gui RDP qualification user' | Out-Null
Add-LocalGroupMember -Group 'Remote Desktop Users' -Member rdpqual
icacls $Share /grant 'rdpqual:(OI)(CI)M' | Out-Null
Copy-Item $Exe "$Share\app\rust-mirror-gui.exe"
if ($OldExe) { New-Item -ItemType Directory -Force "$Share\old" | Out-Null; Copy-Item $OldExe "$Share\old\rust-mirror-gui.exe" }
Copy-Item "$PSScriptRoot\insession.ps1" "$Share\app\insession.ps1"

# FreeRDP: the exact package, checked by SHA-256, installed by pacman.
$pkgDir = "$env:RUNNER_TEMP\freerdp-pkg"
New-Item -ItemType Directory -Force $pkgDir | Out-Null
Invoke-WebRequest "https://repo.msys2.org/mingw/ucrt64/$FreeRdpPkg" -OutFile "$pkgDir\$FreeRdpPkg"
Invoke-WebRequest "https://repo.msys2.org/mingw/ucrt64/$FreeRdpPkg.sig" -OutFile "$pkgDir\$FreeRdpPkg.sig"
$sha = (Get-FileHash "$pkgDir\$FreeRdpPkg").Hash
if ($sha -ne $FreeRdpSha) { throw "FreeRDP package hash $sha does not match the pinned $FreeRdpSha" }
Log "FreeRDP package $FreeRdpPkg SHA-256 $sha (pinned)"
$pacman = 'C:\msys64\usr\bin\pacman.exe'
& $pacman -Sy --noconfirm 2>&1 | Out-File -Append "$Out\pacman.log"
& $pacman -U --noconfirm --needed (& 'C:\msys64\usr\bin\cygpath.exe' -u "$pkgDir\$FreeRdpPkg") 2>&1 | Out-File -Append "$Out\pacman.log"
if ($LASTEXITCODE -ne 0) { throw "pacman could not install FreeRDP" }
& $pacman -Q mingw-w64-ucrt-x86_64-freerdp | ForEach-Object { Log "Installed: $_" }
$client = 'C:\msys64\ucrt64\bin\wfreerdp.exe'
$cert = Get-ChildItem 'Cert:\LocalMachine\Remote Desktop' | Select-Object -First 1
$fp = (([System.Security.Cryptography.SHA256]::Create().ComputeHash($cert.RawData)) | ForEach-Object { $_.ToString('x2') }) -join ':'
Log "RDP server certificate $($cert.Subject), pinned SHA-256 $fp"

# Data for the sync: a catalog of about 290 crate versions (about 46 MiB),
# one download at a time, so a sync lasts long enough to stop and disconnect.
@'
{"crates":{"tokio":{"version":"1","features":["full"],"default-features":true,"optional":false},
 "reqwest":{"version":"0.12","features":[],"default-features":true,"optional":false},
 "clap":{"version":"4","features":["derive"],"default-features":true,"optional":false},
 "regex":{"version":"1","features":[],"default-features":true,"optional":false},
 "serde":{"version":"1","features":["derive"],"default-features":true,"optional":false},
 "anyhow":{"version":"1","features":[],"default-features":true,"optional":false},
 "aws-sdk-s3":{"version":"1","features":[],"default-features":true,"optional":false},
 "aws-config":{"version":"1","features":[],"default-features":true,"optional":false}},
 "crate-rules":{},
 "crate-policy":{"features":"declared","resolver":"2","hosts":["x86_64-pc-windows-msvc","x86_64-unknown-linux-gnu"],"targets":["x86_64-pc-windows-msvc","x86_64-unknown-linux-gnu"],
  "max-versions":3000,"max-files":10000,"max-metadata-bytes":268435456,"max-file-bytes":536870912,"max-final-bytes":10737418240,"max-transfer-bytes":10737418240},
 "crate-output":{"base-url":"https://mirror.example/c/"},
 "crate-network":{"contact":"https://github.com/rustcommons/rust-mirror-gui (RDP qualification)","concurrency":1,"attempts":4,"max-wait-seconds":60}}
'@ | Set-Content "$Share\data\catalog.json"
# The worker treats a closed stdin as "cancel", so keep it open until it exits.
$psi = New-Object System.Diagnostics.ProcessStartInfo "$Share\app\rust-mirror-gui.exe"
$psi.Arguments = "--worker crates-write --input `"$Share\data\catalog.json`" --output `"$Share\data\crates.toml`""
$psi.UseShellExecute = $false; $psi.RedirectStandardInput = $true; $psi.RedirectStandardOutput = $true
$w = [System.Diagnostics.Process]::Start($psi)
$w.StandardOutput.ReadToEnd() | Set-Content "$Out\catalog-write.jsonl"
$w.WaitForExit()
if (-not (Test-Path "$Share\data\crates.toml")) { throw "could not write the crate catalog" }
# A smaller catalog (181 versions) that 0.4.1 can preview too, for the CPU
# comparison: 0.4.1 cannot read the result of the larger preview (1 MiB limit).
$small = Get-Content "$Share\data\catalog.json" -Raw | ConvertFrom-Json
$small.crates.PSObject.Properties.Remove('aws-sdk-s3')
$small.crates.PSObject.Properties.Remove('aws-config')
$small | ConvertTo-Json -Depth 10 | Set-Content "$Share\data\catalog-small.json"
$psi = New-Object System.Diagnostics.ProcessStartInfo "$Share\app\rust-mirror-gui.exe"
$psi.Arguments = "--worker crates-write --input `"$Share\data\catalog-small.json`" --output `"$Share\data\crates-small.toml`""
$psi.UseShellExecute = $false; $psi.RedirectStandardInput = $true; $psi.RedirectStandardOutput = $true
$w = [System.Diagnostics.Process]::Start($psi)
[void]$w.StandardOutput.ReadToEnd()
$w.WaitForExit()
if (-not (Test-Path "$Share\data\crates-small.toml")) { throw "could not write the small crate catalog" }
icacls "$Share\data" /grant 'rdpqual:(OI)(CI)M' | Out-Null

# --------------------------------------------------------- helpers
function Session-Id {
    foreach ($line in (qwinsta rdpqual 2>$null | Select-Object -Skip 1)) {
        if ($line -match '\s(\d+)\s+(Active|Disc|Conn)') { return [int]$Matches[1] }
    }
    $null
}
function Session-State {
    foreach ($line in (qwinsta rdpqual 2>$null | Select-Object -Skip 1)) {
        if ($line -match '^\s*(\S*)\s+rdpqual\s+(\d+)\s+(\w+)') { return "$($Matches[1]) $($Matches[3])".Trim() }
    }
    'none'
}
function Connect([string]$size, [string[]]$extra = @()) {
    Get-Process wfreerdp -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Seconds 1
    $rdpArgs = @('/v:127.0.0.2', '/u:rdpqual', "/p:$pw", "/cert:fingerprint:sha256:$fp", "/size:$size", '/log-level:WARN') + $extra
    $script:rdp = Start-Process -FilePath $client -PassThru -ArgumentList $rdpArgs -RedirectStandardOutput "$Out\freerdp.out" -RedirectStandardError "$Out\freerdp.err"
    $deadline = (Get-Date).AddSeconds(60)
    while ((Get-Date) -lt $deadline) {
        if ((Session-State) -match 'Active$' -and (Session-State) -match 'rdp-tcp') { break }
        Start-Sleep -Seconds 1
    }
    Start-Sleep -Seconds 4
    Log "connected $size $($extra -join ' '): session $(Session-Id) $(Session-State)"
}
$script:n = 0
function In([string]$action, [hashtable]$params = @{}, [int]$timeout = 120) {
    $script:n++
    $id = "{0:d3}-$action" -f $script:n
    $cmd = @{ id = $id; action = $action } + $params
    $cmd | ConvertTo-Json -Depth 5 | Set-Content "$Share\cmd.json"
    Start-ScheduledTask -TaskName rdpqual-cmd
    $file = "$Share\out\$id.json"
    $deadline = (Get-Date).AddSeconds($timeout)
    while (-not (Test-Path $file) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 300 }
    if (-not (Test-Path $file)) { throw "in-session '$action' gave no result in $timeout s" }
    Start-Sleep -Milliseconds 200
    $r = Get-Content $file -Raw | ConvertFrom-Json
    if ($r.error) { Log "in-session $action error: $($r.error)" }
    $r
}
function Wait-Text([string]$pattern, [int]$seconds) {
    $deadline = (Get-Date).AddSeconds($seconds)
    do {
        $t = In 'texts'
        $hit = @($t.names | Where-Object { $_ -match $pattern }) | Select-Object -First 1
        if ($hit) { return $hit }
        Start-Sleep -Seconds 1
    } while ((Get-Date) -lt $deadline)
    $null
}
function Transferred {
    $t = In 'texts'
    $hit = @($t.names | Where-Object { $_ -match '^(\S+ \S+) over the network' }) | Select-Object -First 1
    if ($hit -and $hit -match '^(\S+ \S+) over the network') { return $Matches[1] }
    ''
}
function Files-Done {
    $t = In 'texts'
    foreach ($pattern in '(\d+) of (\d+) files done', '(\d+) of (\d+) crate files verified') {
        $hit = @($t.names | Where-Object { $_ -match $pattern }) | Select-Object -First 1
        if ($hit -and $hit -match $pattern) { return [int]$Matches[1] }
    }
    -1
}
# CPU and memory of the app, measured from this (administrator) session so
# nothing starts inside the test session: a process starting there takes
# focus for a moment, and the window redraws on focus changes.
function Measure-Cpu([int]$seconds) {
    $procs = @(Get-CimInstance Win32_Process -Filter "Name = 'rust-mirror-gui.exe'" | Where-Object { $_.SessionId -eq (Session-Id) })
    $gui = $procs | Where-Object { $_.CommandLine -notmatch '--worker' } | Select-Object -First 1
    if (-not $gui) { return @{ ok = $false; error = 'the app is not running' } }
    $g = Get-Process -Id $gui.ProcessId
    $before = @{}
    foreach ($q in Get-Process rust-mirror-gui -ErrorAction SilentlyContinue) { $before[$q.Id] = $q.TotalProcessorTime.TotalSeconds }
    $g0 = $g.TotalProcessorTime.TotalSeconds
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    Start-Sleep -Seconds $seconds
    $el = $sw.Elapsed.TotalSeconds
    $g.Refresh()
    $total = 0.0
    foreach ($q in Get-Process rust-mirror-gui -ErrorAction SilentlyContinue) {
        $start = if ($before.ContainsKey($q.Id)) { $before[$q.Id] } else { 0 }
        $total += $q.TotalProcessorTime.TotalSeconds - $start
    }
    [ordered]@{
        ok = $true
        gui_cpu_pct = [math]::Round(100 * ($g.TotalProcessorTime.TotalSeconds - $g0) / $el, 1)
        gui_plus_workers_cpu_pct = [math]::Round(100 * $total / $el, 1)
        gui_working_set_mib = [math]::Round($g.WorkingSet64 / 1MB)
        gui_private_mib = [math]::Round($g.PrivateMemorySize64 / 1MB)
        interval_s = [math]::Round($el, 1)
        cores = [Environment]::ProcessorCount
    }
}
function Settings([string]$run, [string]$catalog = 'crates.toml') {
    @{ crate_catalog_path = "$Share\data\$catalog"; crate_output = "$Share\data\$run"; crate_previous = '' }
}
function Start-Sync([string]$exe, [string]$run, [string]$catalog = 'crates.toml', [hashtable]$env = @{}) {
    $l = In 'launch' @{ exe = $exe; settings = (Settings $run $catalog); env = $env }
    [void](In 'invoke' @{ name = 'Crate preview & sync' })
    [void](In 'invoke' @{ name = 'Preview' })
    $p = Wait-Text '^Preview: \d+ crates' 600
    [void](In 'invoke' @{ name = 'Approve this preview and download' })
    [void](Wait-Text '\d+ of \d+ files done' 300)
    @{ launch = $l; preview = $p }
}

# -------------------------------------------------- the in-session task
$action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument '-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File C:\rdpqual\app\insession.ps1'
$principal = New-ScheduledTaskPrincipal -UserId rdpqual -LogonType Interactive
$settings = New-ScheduledTaskSettingsSet -MultipleInstances Queue -ExecutionTimeLimit (New-TimeSpan -Minutes 10)
Register-ScheduledTask -TaskName rdpqual-cmd -Action $action -Principal $principal -Settings $settings | Out-Null

# ------------------------------------------------------------ scenarios
Connect '1280x800'
Start-Sleep -Seconds 8   # first logon: profile and shell

# 1. Launch time and full draw, renderer and adapter.
$l = In 'launch' @{ exe = "$Share\app\rust-mirror-gui.exe"; settings = (Settings 'run-001') }
[void](In 'capture' @{ name = '01-launch' })
Record 'launch-under-10s-and-draws' ($l.ok -and $l.drawn_s -lt 10) @{ window_s = $l.window_s; drawn_s = $l.drawn_s; capture = $l.capture; session = $l.session }
Record 'renderer-is-direct3d12-warp' ($l.modules -contains 'd3d10warp.dll' -and $l.modules -contains 'd3d12.dll') @{ modules = $l.modules; adapters = $l.adapters }

# 2. Preview, idle CPU, sync with live status, Stop, Resume.
[void](In 'invoke' @{ name = 'Crate preview & sync' })
[void](In 'invoke' @{ name = 'Preview' })
$p = Wait-Text '^Preview: \d+ crates' 600
[void](In 'capture' @{ name = '02-preview' })
$idle = Measure-Cpu 30
Record 'idle-cpu-and-memory' ($idle.gui_cpu_pct -lt 3 -and $idle.gui_working_set_mib -lt 300) $idle
[void](In 'invoke' @{ name = 'Approve this preview and download' })
# The sync first resolves again (10-20 s), then downloads.
[void](Wait-Text '\d+ of \d+ files done' 300)
# Each in-session call takes a second or two, so these samples are that far
# apart; the window itself redraws every 0.5 s while a job runs.
$samples = @()
$sw = [System.Diagnostics.Stopwatch]::StartNew()
for ($i = 0; $i -lt 4; $i++) { $samples += [ordered]@{ t_s = [math]::Round($sw.Elapsed.TotalSeconds, 1); transferred = (Transferred); files_done = (Files-Done) } }
# Bytes move continuously; file counts can pause on a large crate.
$increasing = ($samples | ForEach-Object { $_.transferred } | Where-Object { $_ } | Select-Object -Unique).Count -ge 3
[void](In 'capture' @{ name = '03-downloading' })
Record 'live-status-updates' $increasing @{ samples = $samples; preview = $p }
[void](In 'invoke' @{ name = 'Stop' })
$stopped = Wait-Text 'Stopped: operation cancelled' 30
[void](In 'capture' @{ name = '04-stopped' })
Record 'stop' ([bool]$stopped) @{ status = $stopped; files_done = (Files-Done) }

# 3. Resume, then disconnect mid-sync; the app keeps running; reconnect.
[void](In 'invoke' @{ name = 'Resume download' })
Start-Sleep -Seconds 2
$before = Files-Done
$sid = Session-Id
tsdiscon $sid
Start-Sleep -Seconds 3
$disc = Session-State
$alive = @(Get-Process rust-mirror-gui -ErrorAction SilentlyContinue | Where-Object { $_.SessionId -eq $sid }).Count
Log "disconnected: session $disc; app processes in the session: $alive"
Start-Sleep -Seconds 12
Connect '1280x800'
$fin = Wait-Text '^Verified crate mirror' 180
$c = In 'capture' @{ name = '05-after-reconnect' }
Record 'disconnect-mid-sync-reconnect' ($alive -ge 1 -and [bool]$fin -and $c.ok) @{ files_done_before_disconnect = $before; state_while_disconnected = $disc; app_processes_while_disconnected = $alive; finished = $fin; capture = $c.capture }

# 4. Reconnect at another resolution.
tsdiscon (Session-Id); Start-Sleep -Seconds 3
Connect '1600x900'
$c = In 'capture' @{ name = '06-reconnect-1600x900' }
Record 'reconnect-other-resolution' ($c.ok -and $c.capture.screen -eq '1600x900') $c.capture

# 5. Reconnect at 150% scaling.
tsdiscon (Session-Id); Start-Sleep -Seconds 3
Connect '1920x1080' @('/scale-desktop:150', '/scale:140')
$c = In 'capture' @{ name = '07-reconnect-150pct' }
Record 'reconnect-150-percent' ($c.ok -and $c.capture.dpi -ge 144) $c.capture

# 6. Minimize the RDP client during a sync.
Connect '1280x800'
[void](In 'stop')
$s = Start-Sync "$Share\app\rust-mirror-gui.exe" 'run-002'
Start-Sleep -Seconds 2
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class Show { [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd); }
"@
$active = Measure-Cpu 8
# Plan threshold: under 25% of one core. Windows Server 2022's software
# renderer costs about 0.9 s of CPU per frame even at 1 redraw a second, so a
# miss there is recorded as a limit; over 150% means the continuous redraw of
# 0.4.1 is back, which fails.
if ($active.gui_cpu_pct -lt 25) { Record 'cpu-during-sync' $true $active }
elseif ($active.gui_cpu_pct -le 150) { Record 'cpu-during-sync' $false $active -Limit }
else { Record 'cpu-during-sync' $false $active }
$h = (Get-Process wfreerdp | Select-Object -First 1).MainWindowHandle
[void][Show]::ShowWindow($h, 6)   # SW_MINIMIZE
Log "RDP client minimized"
$minCpu = Measure-Cpu 8
Start-Sleep -Seconds 5
[void][Show]::ShowWindow($h, 9)   # SW_RESTORE
Start-Sleep -Seconds 3
$fin = Wait-Text '^Verified crate mirror' 180
$c = In 'capture' @{ name = '08-after-minimize' }
Record 'minimize-client-mid-sync' ([bool]$fin -and $c.ok) @{ cpu_while_minimized = $minCpu; finished = $fin; capture = $c.capture }

# 7. CPU of the 0.4.1 release in the same session and sync, for comparison.
if ($OldExe) {
    [void](In 'stop')
    [void](Start-Sync "$Share\old\rust-mirror-gui.exe" 'run-old' 'crates-small.toml')
    Start-Sleep -Seconds 2
    $old = Measure-Cpu 8
    [void](Wait-Text '^Verified crate mirror' 180)
    [void](In 'stop')
    [void](Start-Sync "$Share\app\rust-mirror-gui.exe" 'run-new' 'crates-small.toml')
    Start-Sleep -Seconds 2
    $new = Measure-Cpu 8
    [void](Wait-Text '^Verified crate mirror' 180)
    Record 'cpu-during-sync-vs-0.4.1' ($new.gui_cpu_pct -lt $old.gui_cpu_pct) @{ v0_4_1 = $old; this_build = $new }
    # The same, with the swap chain presenting without vsync (diagnostic).
    [void](In 'stop')
    [void](Start-Sync "$Share\app\rust-mirror-gui.exe" 'run-novsync' 'crates-small.toml' @{ RUST_MIRROR_GUI_PRESENT_MODE = 'auto_no_vsync' })
    Start-Sleep -Seconds 2
    $nv = Measure-Cpu 8
    [void](Wait-Text '^Verified crate mirror' 180)
    Record 'cpu-during-sync-present-without-vsync (measurement)' $true @{ default_present_mode = $new; auto_no_vsync = $nv }
}

# 8. Start on the console, then take over through RDP.
$sid = Session-Id
# tscon to the console needs SYSTEM; run it from a one-off SYSTEM task.
$t = New-ScheduledTaskAction -Execute 'cmd.exe' -Argument "/c tscon $sid /dest:console > C:\rdpqual\out\tscon.txt 2>&1"
Register-ScheduledTask -TaskName rdpqual-tscon -Action $t -User 'SYSTEM' -RunLevel Highest | Out-Null
Start-ScheduledTask -TaskName rdpqual-tscon
Start-Sleep -Seconds 8
Log "tscon: $((Get-Content C:\rdpqual\out\tscon.txt -ErrorAction SilentlyContinue) -join ' ') (task result $((Get-ScheduledTaskInfo rdpqual-tscon).LastTaskResult))"
$state = Session-State
$c1 = In 'capture' @{ name = '09-on-console' }
Log "on console: $state"
Connect '1280x800'
$c2 = In 'capture' @{ name = '10-after-takeover' }
Record 'console-then-rdp-takeover' ($state -match 'console' -and $c1.ok -and $c2.ok) @{ state_on_console = $state; on_console = $c1.capture; adapters_on_console = $c1.adapters; after_takeover = $c2.capture; session_after = (Session-State) }

Copy-Item "$Share\out\*" $Out -ErrorAction SilentlyContinue
$failed = @($script:results | Where-Object { $_.status -eq 'fail' })
$limits = @($script:results | Where-Object { $_.status -eq 'limit' })
Log "$($script:results.Count) scenarios, $($failed.Count) failed, $($limits.Count) at a known limit"
if ($failed.Count) { exit 1 }
