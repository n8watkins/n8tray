# The real N8 Tray window on Windows, pictured and measured: in its own
# folders with five stand-in apps (the conformance kit's fake module under the
# example apps' names and icons), shown without taking the focus
# (N8TRAY_NO_ACTIVATE), light and dark (N8TRAY_THEME), by the real taskbar and
# by simulated ones on the other edges, auto-hidden and from the overflow
# flyout (N8TRAY_ANCHOR). No keys and no clipboard; every wait is bounded.
#
# Writes to -Out: hub-<theme>-bottom.png and hub-edge-<edge>.png (the window),
# and report.json: where each one is and its gaps to the icon and the work
# area, first-frame checks, memory open and closed
# (the host's figures and the process list's), window style and the names the
# accessibility tree gives screen readers.
#
#   powershell -File scripts\windows-hub-shots.ps1 -Tray target\release\n8tray.exe `
#     -Fake target\release\n8tray-fake-module.exe -Out shots
param(
  [Parameter(Mandatory)] [string]$Tray,
  [Parameter(Mandatory)] [string]$Fake,
  [string]$Icons = "",
  [string]$Out = "hub-shots",
  [string]$Work = (Join-Path ([IO.Path]::GetTempPath()) "n8tray-hub-shots"),
  [int]$LimitSeconds = 900
)
$ErrorActionPreference = "Stop"
$tray = (Resolve-Path $Tray).Path
$fakeExe = (Resolve-Path $Fake).Path
if (-not $Icons) { $Icons = Join-Path (Split-Path -Parent $MyInvocation.MyCommand.Path) "..\spec\examples\icons" }
$icons = (Resolve-Path $Icons).Path
$root = $Work
$out = $Out
New-Item -ItemType Directory -Force $root, $out | Out-Null
$out = (Resolve-Path $out).Path
$started = Get-Date
$report = [ordered]@{}

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type @"
using System; using System.Runtime.InteropServices; using System.Text;
public static class W {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  public delegate bool EnumProc(IntPtr h, IntPtr p);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr p);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint f);
  [DllImport("user32.dll")] public static extern IntPtr GetWindowLongPtrW(IntPtr h, int i);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int a, out RECT r, int n);
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int a, out int v, int n);
  public static IntPtr Find(uint pid, string title) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, p) => { uint q; GetWindowThreadProcessId(h, out q); if (q == pid) { var s = new StringBuilder(256); GetWindowText(h, s, 256); if (s.ToString() == title) { found = h; return false; } } return true; }, IntPtr.Zero);
    return found;
  }
}
"@

function Log($text) { $line = "{0:HH:mm:ss} {1}" -f (Get-Date), $text; Write-Output $line; Add-Content (Join-Path $out "handson.log") $line }
function Check-Time { if (((Get-Date) - $started).TotalSeconds -gt $LimitSeconds) { throw "over the time limit" } }
function Wait-Until($what, [scriptblock]$test, [int]$seconds) {
  $end = (Get-Date).AddSeconds($seconds)
  while ((Get-Date) -lt $end) { Check-Time; $v = & $test; if ($v) { return $v }; Start-Sleep -Milliseconds 250 }
  throw "timed out waiting for $what"
}

$home8 = Join-Path $root "home"
$apps = Join-Path $root "apps"
function Status { try { Get-Content -Raw (Join-Path $home8 "status.json") | ConvertFrom-Json } catch { $null } }

function Install-StandIns {
  if (Test-Path $home8) { Remove-Item -Recurse -Force $home8 }
  New-Item -ItemType Directory -Force (Join-Path $home8 "modules") | Out-Null
  $list = @(
    @{ id = "voicedock"; name = "VoiceDock"; group = "Audio"; http = $true; auto = $true; extra = @("--eat-mb", "120", "--children", "1") },
    @{ id = "n8hotkeys"; name = "N8 Hotkeys"; group = "Hotkeys"; http = $false; auto = $true; extra = @() },
    @{ id = "n8commentary"; name = "N8 Commentary"; group = "Streaming"; http = $false; auto = $false; extra = @() },
    @{ id = "wslmemory"; name = "WSL Memory"; group = "System"; http = $false; auto = $true; extra = @() },
    @{ id = "leadscraper"; name = "Lead Scraper"; group = "Work"; http = $false; auto = $true; extra = @("--eat-mb", "40") }
  )
  $port = 47700
  foreach ($a in $list) {
    $dir = Join-Path $apps $a.id
    New-Item -ItemType Directory -Force $dir | Out-Null
    Copy-Item -Force $fakeExe (Join-Path $dir "app.exe")
    Copy-Item -Force (Join-Path $icons "$($a.id).svg") (Join-Path $dir "icon.svg")
    $args = @("--id", $a.id, "--interval-seconds", "1", "--exit-after-seconds", "$LimitSeconds") + $a.extra
    if ($a.http) {
      $l = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0); $l.Start(); $port = $l.LocalEndpoint.Port; $l.Stop()
      $status = @{ type = "http"; url = "http://127.0.0.1:$port/n8/v2/status"; intervalSeconds = 1; timeoutMs = 1000 }
      $args = @("--status", "http", "--port", "$port") + $args
      $stop = @{ type = "http"; graceSeconds = 5 }
    } else {
      $status = @{ type = "file"; intervalSeconds = 1 }
      $args = @("--status", "file") + $args
      $stop = @{ type = "terminate"; graceSeconds = 5 }
    }
    $m = [ordered]@{ contract = 2; id = $a.id; name = $a.name; version = "1.0.0"; group = $a.group; icon = "icon.svg"; installDir = $dir; status = $status; run = @{ program = "app.exe"; args = $args; stop = $stop }; autoStartDefault = $a.auto }
    $json = $m | ConvertTo-Json -Depth 6
    [IO.File]::WriteAllText((Join-Path $home8 "modules\$($a.id).json"), $json, (New-Object Text.UTF8Encoding $false))
  }
}

function Start-Tray([hashtable]$env) {
  $env:N8TRAY_HOME = $home8; $env:N8TRAY_NO_ACTIVATE = "1"
  foreach ($k in @("N8TRAY_THEME", "N8TRAY_ANCHOR")) { Remove-Item "env:$k" -ErrorAction SilentlyContinue }
  foreach ($k in $env.Keys) { Set-Item "env:$k" $env[$k] }
  $p = Start-Process $tray -PassThru
  Wait-Until "the status file" { $s = Status; $s -and $s.tray.pid -eq $p.Id } 30 | Out-Null
  return $p
}
function Stop-Tray($p) {
  Start-Process $tray -ArgumentList "--exit" -Wait
  try { Wait-Process -Id $p.Id -Timeout 15 } catch { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
}

function Show-Hub($p) {
  $t0 = Get-Date
  Start-Process $tray -ArgumentList "--show" -Wait
  $h = Wait-Until "the window" { $w = [W]::Find([uint32]$p.Id, "N8 Tray"); if ($w -ne [IntPtr]::Zero -and [W]::IsWindowVisible($w)) { $w } } 20
  $ms = [int]((Get-Date) - $t0).TotalMilliseconds
  # The first frame Windows has: it must be the whole window, already drawn.
  $first = Capture $h
  return @{ hwnd = $h; ms = $ms; first = $first }
}

function Bounds($h) { $r = New-Object W+RECT; [void][W]::DwmGetWindowAttribute($h, 9, [ref]$r, 16); return $r }

function Capture($h) {
  $r = Bounds $h; $w = $r.R - $r.L; $hh = $r.B - $r.T
  $bmp = New-Object Drawing.Bitmap $w, $hh
  $g = [Drawing.Graphics]::FromImage($bmp); $dc = $g.GetHdc()
  [void][W]::PrintWindow($h, $dc, 2); $g.ReleaseHdc($dc); $g.Dispose()
  if (Blank $bmp) {
    # WebView2 composites on the GPU, which PrintWindow often misses: copy
    # the window's own rectangle from the screen instead (it is topmost).
    $g = [Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($r.L, $r.T, 0, 0, (New-Object Drawing.Size $w, $hh))
    $g.Dispose()
  }
  return $bmp
}

function Blank($bmp) {
  # A frame that is one color (nothing drawn yet) has no distinct pixels.
  $seen = @{}
  for ($y = 10; $y -lt $bmp.Height; $y += 23) { for ($x = 10; $x -lt $bmp.Width; $x += 19) { $seen[$bmp.GetPixel($x, $y).ToArgb()] = 1 } }
  return $seen.Count -lt 4
}

function Placement-Line { (Get-ChildItem (Join-Path $home8 "logs") -Filter "n8tray*.log" | Sort-Object LastWriteTime | Select-Object -Last 1 | Get-Content | Select-String "The window opens at" | Select-Object -Last 1).ToString() }

# The window's picture, and where it is against the icon and the work area.
function Save-Window($name, $h, $anchor, $area) {
  $r = Bounds $h
  $bmp = Capture $h
  $bmp.Save((Join-Path $out "$name.png"), [Drawing.Imaging.ImageFormat]::Png)
  $bmp.Dispose()
  return [ordered]@{
    window = "$($r.L),$($r.T) to $($r.R),$($r.B)"; icon = ($anchor -join ","); workArea = ($area -join ",")
    # Gaps between the visible frame and the icon and work area (2 expected
    # on the sides that face them).
    gapLeftOfIcon = ($anchor[0] - $r.R); gapAboveIcon = ($anchor[1] - $r.B); gapRightOfIcon = ($r.L - ($anchor[0] + $anchor[2])); gapBelowIcon = ($r.T - ($anchor[1] + $anchor[3]))
    gapLeft = ($r.L - $area[0]); gapTop = ($r.T - $area[1]); gapRight = (($area[0] + $area[2]) - $r.R); gapBottom = (($area[1] + $area[3]) - $r.B)
  }
}

function Process-Memory($trayPid) {
  $all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, Name
  $tree = @($trayPid); $added = $true
  while ($added) { $added = $false; foreach ($p in $all) { if ($tree -contains $p.ParentProcessId -and -not ($tree -contains $p.ProcessId) -and $p.Name -like "msedgewebview2*") { $tree += $p.ProcessId; $added = $true } } }
  $procs = $tree | ForEach-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue }
  $own = $procs | Where-Object Id -eq $trayPid
  $web = $procs | Where-Object Id -ne $trayPid
  return [ordered]@{
    trayPrivateMb = [math]::Round($own.PrivateMemorySize64 / 1MB, 1); trayWorkingSetMb = [math]::Round($own.WorkingSet64 / 1MB, 1)
    windowProcesses = @($web).Count
    windowPrivateMb = [math]::Round((($web | Measure-Object PrivateMemorySize64 -Sum).Sum) / 1MB, 1)
    windowWorkingSetMb = [math]::Round((($web | Measure-Object WorkingSet64 -Sum).Sum) / 1MB, 1)
  }
}

function Uia-Names($h) {
  $root = [Windows.Automation.AutomationElement]::FromHandle($h)
  $types = @([Windows.Automation.ControlType]::Button, [Windows.Automation.ControlType]::Group, [Windows.Automation.ControlType]::CheckBox)
  $found = Wait-Until "the window's accessibility tree" { $f = $root.FindAll([Windows.Automation.TreeScope]::Descendants, [Windows.Automation.Condition]::TrueCondition); if ($f.Count -gt 20) { $f } } 15
  $names = @()
  foreach ($e in $found) { if ($types -contains $e.Current.ControlType -and $e.Current.Name) { $names += ("{0}: {1}" -f $e.Current.ControlType.ProgrammaticName.Replace("ControlType.", ""), $e.Current.Name) } }
  return $names
}

$trayProc = $null
$watchdog = Start-Process powershell -WindowStyle Hidden -PassThru -ArgumentList "-NoProfile", "-Command", "Start-Sleep $($LimitSeconds + 60); Get-Process | Where-Object { `$_.Path -like '$root*' -or `$_.Path -eq '$tray' } | Stop-Process -Force"
try {
  Log "START (limit $LimitSeconds s)"
  $fg0 = [W]::GetForegroundWindow()
  Install-StandIns
  Add-Type -AssemblyName System.Windows.Forms
  $screen = [System.Windows.Forms.Screen]::PrimaryScreen
  $mon = $screen.Bounds; $wa = $screen.WorkingArea
  $report.monitor = "$($mon.Width)x$($mon.Height), work area $($wa.X),$($wa.Y) $($wa.Width)x$($wa.Height)"

  # 1. The real taskbar and the real icon, in each theme.
  foreach ($theme in @("dark", "light")) {
    $trayProc = Start-Tray @{ N8TRAY_THEME = $theme }
    Wait-Until "the stand-ins to run" { $s = Status; $s -and (@($s.apps | Where-Object running).Count -ge 4) } 60 | Out-Null
    if ($theme -eq "dark") {
      # Idle, before any window: the trim runs 30 s after start.
      Start-Sleep 42
      $s = Status
      $report.idle = [ordered]@{ host = [ordered]@{ privateMb = $s.tray.privateMb; workingSetMb = $s.tray.rssMb; cpuPercent = $s.tray.cpuPercent }; measured = (Process-Memory $trayProc.Id) }
      Log ("idle: private {0} MB, working set {1} MB, CPU {2}%" -f $s.tray.privateMb, $s.tray.rssMb, $s.tray.cpuPercent)
    } else { Start-Sleep 20 }
    $shown = Show-Hub $trayProc
    $report["firstFrame_$theme"] = @{ ms = $shown.ms; blank = (Blank $shown.first) }
    Start-Sleep 3
    $line = Placement-Line
    Log $line
    if ($line -match "opens at (-?\d+),(-?\d+) \((\d+)x(\d+), scale ([\d.]+)\) by the (\w+) taskbar; icon at (-?\d+),(-?\d+) \((\d+)x(\d+)\), work area (-?\d+),(-?\d+) \((\d+)x(\d+)\)") {
      $anchor = @([int]$Matches[7], [int]$Matches[8], [int]$Matches[9], [int]$Matches[10]); $workArea = @([int]$Matches[11], [int]$Matches[12], [int]$Matches[13], [int]$Matches[14])
      $report["real_$theme"] = Save-Window "hub-$theme-bottom" $shown.hwnd $anchor $workArea
      $report["real_$theme"].edge = $Matches[6]; $report["real_$theme"].scale = $Matches[5]
    }
    if ($theme -eq "dark") {
      Start-Sleep 10
      $s = Status
      $report.open = [ordered]@{ host = [ordered]@{ trayPrivateMb = $s.tray.privateMb; windowPrivateMb = $s.tray.windowPrivateMb; trayWorkingSetMb = $s.tray.rssMb; windowWorkingSetMb = $s.tray.windowMb; cpuPercent = $s.tray.cpuPercent }; measured = (Process-Memory $trayProc.Id) }
      $h = $shown.hwnd
      $ex = [int64][W]::GetWindowLongPtrW($h, -20)
      $corner = 0; [void][W]::DwmGetWindowAttribute($h, 33, [ref]$corner, 4)
      $report.window = [ordered]@{ toolWindow = (($ex -band 0x80) -ne 0); appWindow = (($ex -band 0x40000) -ne 0); cornerPreference = $corner; dpi = [W]::GetDpiForWindow($h); widthPx = ((Bounds $h).R - (Bounds $h).L) }
      try { $report.uia = Uia-Names $h } catch { $report.uia = "unavailable: $($_.Exception.Message)" }
      $report.foregroundUnchanged = ([W]::GetForegroundWindow() -eq $fg0)
      Start-Process $tray -ArgumentList "--hide" -Wait
      $hid = Get-Date
      $s = Wait-Until "the window destroyed and memory back" { $s = Status; if ($s -and $s.tray.windowMb -eq 0 -and ((Get-Date) - $hid).TotalSeconds -gt 63) { $s } } 90
      $report.closed = [ordered]@{ seconds = [int]((Get-Date) - $hid).TotalSeconds; host = [ordered]@{ privateMb = $s.tray.privateMb; workingSetMb = $s.tray.rssMb }; measured = (Process-Memory $trayProc.Id) }
      Log ("closed after {0} s: private {1} MB" -f $report.closed.seconds, $s.tray.privateMb)
      # Reopening after it was destroyed creates the window again.
      $again = Show-Hub $trayProc
      $report.reopenAfterDestroy = @{ ms = $again.ms; blank = (Blank $again.first) }
      Start-Sleep 2
      Start-Process $tray -ArgumentList "--hide" -Wait
      $quick = Show-Hub $trayProc
      $report.reopenWhileHidden = @{ ms = $quick.ms; blank = (Blank $quick.first) }
    }
    Stop-Tray $trayProc; $trayProc = $null
  }

  # 2. The other taskbar edges, simulated: the icon and work area the
  #    taskbar there would give, on this monitor.
  $W = $mon.Width; $H = $mon.Height; $band = $mon.Height - $wa.Height; if ($band -le 0) { $band = 48 }
  $i = [int]($band / 2 - 12)
  $cases = [ordered]@{
    top = @(@(($W - 300), $i, 24, 24), @(0, $band, $W, ($H - $band)))
    left = @(@($i, ($H - 300), 24, 24), @($band, 0, ($W - $band), $H))
    right = @(@(($W - $band + $i), ($H - 300), 24, 24), @(0, 0, ($W - $band), $H))
    autohide = @(@(($W - 300), ($H - $band + $i), 24, 24), @(0, 0, $W, $H))
    overflow = @(@(($W - 340), ($H - $band - 60), 32, 32), @(0, 0, $W, ($H - $band)))
  }
  foreach ($k in $cases.Keys) {
    $a = $cases[$k][0]; $wk = $cases[$k][1]
    $trayProc = Start-Tray @{ N8TRAY_THEME = "dark"; N8TRAY_ANCHOR = (($a -join ",") + ";" + ($wk -join ",")) }
    Wait-Until "the stand-ins" { $s = Status; $s -and (@($s.apps | Where-Object running).Count -ge 4) } 60 | Out-Null
    $shown = Show-Hub $trayProc
    Start-Sleep 2
    $report["edge_$k"] = Save-Window "hub-edge-$k" $shown.hwnd $a $wk
    Stop-Tray $trayProc; $trayProc = $null
  }
  Log "END"
} catch {
  Log "STOPPED: $($_.Exception.Message)"
  $report.error = $_.Exception.Message
} finally {
  if ($trayProc) { Stop-Tray $trayProc }
  Get-Process | Where-Object { $_.Path -like "$apps*" } | Stop-Process -Force -ErrorAction SilentlyContinue
  Stop-Process -Id $watchdog.Id -Force -ErrorAction SilentlyContinue
  $report | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $out "report.json")
}
