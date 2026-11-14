# Runs INSIDE the test user's Remote Desktop session (started by a
# session-scoped scheduled task). Reads one command from C:\rdpqual\cmd.json,
# performs it against the running rust-mirror-gui window and writes the result
# to C:\rdpqual\out\<id>.json. Commands:
#   launch   {exe, settings}  write the app settings, start the app, time the
#                              window and the first full draw, record renderer
#   capture  {name}           PrintWindow (PW_RENDERFULLCONTENT) capture
#   invoke   {name}           press a button or sidebar entry by its accessible
#                              name (UI Automation; egui exposes AccessKit)
#   texts    {}               all accessible names (labels, status, progress)
#   cpu      {seconds}        CPU% (of one core) and memory of the app and its
#                              worker processes over an interval
#   stop     {}               end the app
$ErrorActionPreference = 'Stop'
$cmd = Get-Content C:\rdpqual\cmd.json -Raw | ConvertFrom-Json
$result = [ordered]@{ id = $cmd.id; action = $cmd.action; ok = $false }
Add-Type -AssemblyName System.Drawing, System.Windows.Forms, UIAutomationClient, UIAutomationTypes
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class Win {
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hwnd, IntPtr hdc, uint flags);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hwnd, out RECT rect);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
}
"@

function App-Process {
    Get-Process rust-mirror-gui -ErrorAction SilentlyContinue |
        Where-Object { $_.MainWindowTitle -like 'rust-mirror*' } | Select-Object -First 1
}

function Capture([IntPtr]$hwnd, [string]$path) {
    $rect = New-Object Win+RECT; [void][Win]::GetWindowRect($hwnd, [ref]$rect)
    $client = New-Object Win+RECT; [void][Win]::GetClientRect($hwnd, [ref]$client)
    $w = $rect.Right - $rect.Left; $h = $rect.Bottom - $rect.Top
    $bmp = New-Object System.Drawing.Bitmap $w, $h
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $hdc = $g.GetHdc()
    $ok = [Win]::PrintWindow($hwnd, $hdc, 2)
    $g.ReleaseHdc($hdc); $g.Dispose()
    if ($path) { $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png) }
    $counts = @{}; $n = 0
    for ($y = 0; $y -lt $h; $y += 4) { for ($x = 0; $x -lt $w; $x += 4) { $c = $bmp.GetPixel($x, $y).ToArgb(); $counts[$c] = 1 + [int]$counts[$c]; $n++ } }
    $bmp.Dispose()
    $top = ($counts.Values | Measure-Object -Maximum).Maximum
    $screen = [System.Windows.Forms.Screen]::PrimaryScreen
    [ordered]@{
        printwindow = $ok; window = "${w}x${h}"; client = "$($client.Right)x$($client.Bottom)"
        dpi = [Win]::GetDpiForWindow($hwnd); minimized = [Win]::IsIconic($hwnd)
        screen = "$($screen.Bounds.Width)x$($screen.Bounds.Height)"
        colors = $counts.Count; top_color_pct = [math]::Round(100.0 * $top / [math]::Max($n, 1), 1)
        drawn = ($ok -and $counts.Count -ge 20 -and (100.0 * $top / [math]::Max($n, 1)) -le 98)
    }
}

function Root {
    $p = App-Process
    if (-not $p) { throw "the app is not running" }
    [System.Windows.Automation.AutomationElement]::FromHandle($p.MainWindowHandle)
}

function All-Names {
    $all = (Root).FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
    $names = foreach ($e in $all) { $n = $e.Current.Name; if ($n) { $n } }
    @($names)
}

try {
    switch ($cmd.action) {
        'launch' {
            $dir = Join-Path $env:APPDATA 'rust-mirror-gui'
            New-Item -ItemType Directory -Force $dir | Out-Null
            ($cmd.settings | ConvertTo-Json -Compress) | Set-Content (Join-Path $dir 'settings.json')
            $env:RUST_MIRROR_GUI_FRAME_LOG = "C:\rdpqual\out\frames-$($cmd.id).log"
            $sw = [System.Diagnostics.Stopwatch]::StartNew()
            $p = Start-Process -FilePath $cmd.exe -PassThru
            while ($p.MainWindowHandle -eq [IntPtr]::Zero -or $p.MainWindowTitle -notlike 'rust-mirror*') {
                if ($p.HasExited) { throw "the app exited with code $($p.ExitCode)" }
                if ($sw.Elapsed.TotalSeconds -gt 60) { throw "no window after 60 s" }
                Start-Sleep -Milliseconds 100; $p.Refresh()
            }
            $result.window_s = [math]::Round($sw.Elapsed.TotalSeconds, 2)
            do {
                $cap = Capture $p.MainWindowHandle $null
                if ($cap.drawn) { break }
                Start-Sleep -Milliseconds 200
            } while ($sw.Elapsed.TotalSeconds -lt 60)
            $result.drawn_s = [math]::Round($sw.Elapsed.TotalSeconds, 2)
            $result.capture = $cap
            Start-Sleep -Seconds 2
            $p.Refresh()
            $result.modules = @($p.Modules | Where-Object { $_.ModuleName -match '^(d3d12|d3d12core|d3d11|dxgi|d3d10warp|vulkan-1|opengl32)\.dll$' } | ForEach-Object { $_.ModuleName } | Sort-Object)
            $result.adapters = @(Get-CimInstance Win32_VideoController | ForEach-Object { $_.Name })
            $result.session = [System.Diagnostics.Process]::GetCurrentProcess().SessionId
            $result.ok = $cap.drawn
        }
        'capture' {
            $p = App-Process
            if (-not $p) { throw "the app is not running" }
            $result.capture = Capture $p.MainWindowHandle "C:\rdpqual\out\$($cmd.name).png"
            $result.adapters = @(Get-CimInstance Win32_VideoController | ForEach-Object { $_.Name })
            $result.ok = $result.capture.drawn
        }
        'invoke' {
            $cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, $cmd.name)
            $deadline = (Get-Date).AddSeconds(15)
            do {
                $el = (Root).FindFirst([System.Windows.Automation.TreeScope]::Descendants, $cond)
                if ($el) { break }
                Start-Sleep -Milliseconds 300
            } while ((Get-Date) -lt $deadline)
            if (-not $el) { throw "no element named '$($cmd.name)'" }
            $pattern = $null
            if ($el.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
                $pattern.Invoke(); $result.how = 'invoke'
            } elseif ($el.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern, [ref]$pattern)) {
                $pattern.Select(); $result.how = 'select'
            } elseif ($el.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern, [ref]$pattern)) {
                $pattern.Toggle(); $result.how = 'toggle'
            } else {
                # A real click at the element's centre, inside this session.
                $r = $el.Current.BoundingRectangle
                [void][Win]::SetCursorPos([int]($r.X + $r.Width / 2), [int]($r.Y + $r.Height / 2))
                [Win]::mouse_event(0x2, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 50
                [Win]::mouse_event(0x4, 0, 0, 0, [UIntPtr]::Zero); $result.how = 'click'
            }
            $result.ok = $true
        }
        'texts' {
            $result.names = All-Names
            $result.ok = $true
        }
        'cpu' {
            # GUI alone, and GUI plus worker processes (same executable).
            $gui = App-Process
            if (-not $gui) { throw "the app is not running" }
            $before = @{}
            foreach ($q in Get-Process rust-mirror-gui -ErrorAction SilentlyContinue) { $before[$q.Id] = $q.TotalProcessorTime.TotalSeconds }
            $g0 = $gui.TotalProcessorTime.TotalSeconds
            $sw = [System.Diagnostics.Stopwatch]::StartNew()
            Start-Sleep -Seconds $cmd.seconds
            $el = $sw.Elapsed.TotalSeconds
            $gui.Refresh()
            $total = 0.0
            foreach ($q in Get-Process rust-mirror-gui -ErrorAction SilentlyContinue) {
                $start = if ($before.ContainsKey($q.Id)) { $before[$q.Id] } else { 0 }
                $total += $q.TotalProcessorTime.TotalSeconds - $start
            }
            $result.gui_cpu_pct = [math]::Round(100 * ($gui.TotalProcessorTime.TotalSeconds - $g0) / $el, 1)
            $result.gui_plus_workers_cpu_pct = [math]::Round(100 * $total / $el, 1)
            $result.gui_working_set_mib = [math]::Round($gui.WorkingSet64 / 1MB)
            $result.gui_private_mib = [math]::Round($gui.PrivateMemorySize64 / 1MB)
            $result.interval_s = [math]::Round($el, 1)
            $result.cores = [Environment]::ProcessorCount
            $result.ok = $true
        }
        'stop' {
            Get-Process rust-mirror-gui -ErrorAction SilentlyContinue | Stop-Process -Force
            $result.ok = $true
        }
    }
} catch {
    $result.error = $_.Exception.Message
}
$result | ConvertTo-Json -Depth 5 | Set-Content "C:\rdpqual\out\$($cmd.id).json"
