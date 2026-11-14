# Launch rust-mirror-gui, capture its window with PrintWindow
# (PW_RENDERFULLCONTENT, which includes Direct3D content), and report whether
# it drew anything. Exits 1 if the window never appears, the process dies, or
# the capture is blank (almost a single color).
#
# Usage: capture-window.ps1 -Exe PATH -Out PNG [-WaitSeconds 15]
param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [Parameter(Mandatory = $true)][string]$Out,
    [int]$WaitSeconds = 15
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class Win {
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hwnd, IntPtr hdc, uint flags);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
}
"@

$adapters = Get-CimInstance Win32_VideoController | ForEach-Object { "$($_.Name) (driver $($_.DriverVersion))" }
Write-Output "Display adapters: $($adapters -join '; ')"
Write-Output "WGPU_BACKEND: '$env:WGPU_BACKEND' (empty means the app's Windows default, Direct3D 12)"

$proc = Start-Process -FilePath $Exe -PassThru
$deadline = (Get-Date).AddSeconds(60)
# Wait for the app's own window (title "rust-mirror GUI"), not a helper window.
while ($proc.MainWindowHandle -eq [IntPtr]::Zero -or $proc.MainWindowTitle -notlike 'rust-mirror*') {
    if ($proc.HasExited) { throw "the app exited with code $($proc.ExitCode) before showing a window" }
    if ((Get-Date) -gt $deadline) { throw "no window after 60 s" }
    Start-Sleep -Milliseconds 500
    $proc.Refresh()
}
$hwnd = $proc.MainWindowHandle
Write-Output "Window handle $hwnd, title '$($proc.MainWindowTitle)', visible $([Win]::IsWindowVisible($hwnd))"
Start-Sleep -Seconds $WaitSeconds
$proc.Refresh()
if ($proc.HasExited) { throw "the app exited with code $($proc.ExitCode) after showing its window" }

# Which renderer the process actually loaded (no code changes needed):
# d3d10warp.dll is Windows' software rasterizer (WARP) behind Direct3D.
$gfx = $proc.Modules | Where-Object { $_.ModuleName -match '^(d3d12|d3d12core|d3d11|dxgi|d3d10warp|vulkan-1|opengl32|nvwgf2umx|nvoglv64|amdxc64|aticfx64|igd12umd64|igd10iumd64)\.dll$' } | ForEach-Object { $_.ModuleName }
Write-Output "Graphics modules loaded: $(($gfx | Sort-Object) -join ', ')"
if ($gfx -contains 'd3d10warp.dll') { Write-Output "Renderer: Direct3D 12 on WARP (software)" }

$rect = New-Object Win+RECT
[void][Win]::GetWindowRect($hwnd, [ref]$rect)
$screen = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
Write-Output "Primary screen $($screen.Width)x$($screen.Height)"
[void][Win]::GetWindowRect($hwnd, [ref]$rect)
$w = $rect.Right - $rect.Left
$h = $rect.Bottom - $rect.Top
Write-Output "Window size ${w}x${h}"
$bmp = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($bmp)
$hdc = $g.GetHdc()
$PW_RENDERFULLCONTENT = 2
$ok = [Win]::PrintWindow($hwnd, $hdc, $PW_RENDERFULLCONTENT)
$g.ReleaseHdc($hdc)
$g.Dispose()
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
Write-Output "PrintWindow returned $ok; saved $Out"

# Blank check: distinct colors and the share of the most common color on a grid.
$counts = @{}
$samples = 0
for ($y = 0; $y -lt $h; $y += 4) {
    for ($x = 0; $x -lt $w; $x += 4) {
        $c = $bmp.GetPixel($x, $y).ToArgb()
        $counts[$c] = 1 + [int]$counts[$c]
        $samples++
    }
}
$bmp.Dispose()
$top = ($counts.Values | Measure-Object -Maximum).Maximum
$share = [math]::Round(100.0 * $top / $samples, 1)
Write-Output "Sampled $samples pixels: $($counts.Count) distinct colors; most common color covers $share%"
Stop-Process -Id $proc.Id -Force
if (-not $ok -or $counts.Count -lt 20 -or $share -gt 98) {
    throw "the captured window looks blank"
}
Write-Output "The window drew content."
