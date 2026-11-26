# Measures the native shell's memory (docs/DECISIONS.md, D33), modelled on
# Pulse's scripts/measure.ps1. It takes no input and never takes the focus:
# no keys, no clicks, no clipboard. Every start gets an N8 Tray home of its
# own under -Out (N8TRAY_HOME), so an installed N8 Tray, its settings, its
# sign-in entry and its single-instance lock are never touched. The hosted
# app is the conformance kit's fake module. The window is opened with
# `--show` and closed with `--hide`, shown without activation
# (N8TRAY_NO_ACTIVATE), and is on screen only for the open samples.
#
# For each run it records the tray process's private working set (Task
# Manager's Memory column) and private bytes, its loaded modules and its
# top-level windows at four points: closed (after the start-up trim), window
# open, hidden 5 s (before the drop) and after the drop (or keep) and trim;
# then after each of -Cycles more opens and closes.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File native\scripts\measure.ps1 `
#     -Native target\release\n8tray-native.exe -Fake target\release\n8tray-fake-module.exe -Out C:\path\to\out
#   Add -Extra <exe> to measure a second build (for example one renderer only)
#   with its own default renderer.
param(
  [Parameter(Mandatory)] [string]$Native,
  [Parameter(Mandatory)] [string]$Fake,
  [Parameter(Mandatory)] [string]$Out,
  [string]$Extra = "",
  [int]$Cycles = 4,
  [int]$LimitSeconds = 1800
)
$ErrorActionPreference = "Stop"
$native = (Resolve-Path $Native).Path
$fakeExe = (Resolve-Path $Fake).Path
New-Item -ItemType Directory -Force $Out | Out-Null
$out = (Resolve-Path $Out).Path
$started = Get-Date
$report = [ordered]@{ native = $native; startedAt = $started.ToString("s"); machine = $null; baseline = $null; runs = @(); errors = @() }

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices; using System.Text; using System.Collections.Generic;
public static class W {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  public delegate bool EnumProc(IntPtr h, IntPtr p);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr p);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint f);
  [DllImport("user32.dll")] public static extern IntPtr GetWindowLongPtrW(IntPtr h, int i);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int a, out RECT r, int n);
  public static List<string[]> Windows(uint pid) {
    var list = new List<string[]>();
    EnumWindows((h, p) => {
      uint q; GetWindowThreadProcessId(h, out q);
      if (q == pid) {
        var s = new StringBuilder(256); GetWindowText(h, s, 256);
        var c = new StringBuilder(256); GetClassName(h, c, 256);
        list.Add(new string[] { h.ToInt64().ToString(), s.ToString(), IsWindowVisible(h) ? "1" : "0", c.ToString() });
      }
      return true;
    }, IntPtr.Zero);
    return list;
  }
}
"@
[void][W]::SetProcessDPIAware()

# Write-Host, not the output stream: a function's output is its return value.
function Log($text) { $line = "{0:HH:mm:ss} {1}" -f (Get-Date), $text; Write-Host $line; Add-Content (Join-Path $out "measure.log") $line }
function Check-Time { if (((Get-Date) - $started).TotalSeconds -gt $LimitSeconds) { throw "over the time limit of $LimitSeconds s" } }
function Wait-Until($what, [scriptblock]$test, [int]$seconds) {
  $end = (Get-Date).AddSeconds($seconds)
  while ((Get-Date) -lt $end) { Check-Time; $v = & $test; if ($v) { return $v }; Start-Sleep -Milliseconds 200 }
  throw "timed out waiting for $what"
}
function Wait-Since([datetime]$since, [double]$seconds) {
  $left = $seconds - ((Get-Date) - $since).TotalSeconds
  if ($left -gt 0) { Check-Time; Start-Sleep -Milliseconds ([int]($left * 1000)) }
}

# ----- the machine, before anything starts -----
function Machine {
  $cpu = (Get-Counter "\Processor(_Total)\% Processor Time" -SampleInterval 1 -MaxSamples 3).CounterSamples | Measure-Object CookedValue -Average
  $os = Get-CimInstance Win32_OperatingSystem
  return [ordered]@{ cpuPercent = [math]::Round($cpu.Average, 1); freeGb = [math]::Round($os.FreePhysicalMemory / 1MB, 1); totalGb = [math]::Round($os.TotalVisibleMemorySize / 1MB, 1); at = (Get-Date).ToString("s") }
}

# ----- one process's memory: Task Manager's Memory column and private bytes -----
function Private-WorkingSet([int]$id, [string]$name) {
  for ($try = 0; $try -lt 5; $try++) {
    try { return (Get-Counter -Counter "\Process V2($name`:$id)\Working Set - Private" -ErrorAction Stop).CounterSamples[0].CookedValue } catch { Start-Sleep -Milliseconds 300 }
  }
  return $null
}
function Sample([int]$id, [int]$count = 3) {
  $p = Get-Process -Id $id
  $name = $p.ProcessName
  $ws = @(); $pb = @()
  for ($i = 0; $i -lt $count; $i++) {
    if ($i -gt 0) { Start-Sleep -Milliseconds 700 }
    $v = Private-WorkingSet $id $name; if ($null -ne $v) { $ws += $v }
    $p.Refresh(); $pb += $p.PrivateMemorySize64
  }
  $mods = @($p.Modules | ForEach-Object { $_.ModuleName })
  $wins = [W]::Windows([uint32]$id)
  $median = { param($xs) $s = @($xs | Sort-Object); if ($s.Count -eq 0) { $null } else { $s[[int][math]::Floor(($s.Count - 1) / 2)] } }
  return [ordered]@{
    privateWorkingSetMb = [math]::Round((& $median $ws) / 1MB, 2)
    privateBytesMb = [math]::Round((& $median $pb) / 1MB, 2)
    workingSetMb = [math]::Round($p.WorkingSet64 / 1MB, 2)
    modules = $mods.Count
    gpuModules = @($mods | Where-Object { $_ -match '^(d3d|dxgi|dxcore|opengl|amd|atig|nvoglv|nvd3d|ig[0-9a-z]+icd|vulkan)' })
    topLevelWindows = $wins.Count
    visibleWindows = @($wins | Where-Object { $_[2] -eq "1" }).Count
    windows = @($wins | ForEach-Object { "$($_[3]) '$($_[1])' visible=$($_[2])" })
    moduleList = $mods
  }
}

# ----- the hosted app: one fake module, file status -----
$appDir = Join-Path $out "apps\fake"
New-Item -ItemType Directory -Force $appDir | Out-Null
$appExe = Join-Path $appDir "app.exe"
Copy-Item -Force $fakeExe $appExe
function New-Home([string]$tag) {
  $h = Join-Path $out "home-$tag"
  if (Test-Path $h) { Remove-Item -Recurse -Force $h }
  New-Item -ItemType Directory -Force (Join-Path $h "modules") | Out-Null
  $m = [ordered]@{
    contract = 2; id = "fake"; name = "Fake App"; version = "1.0.0"; installDir = $appDir
    status = @{ type = "file"; intervalSeconds = 1 }
    run = @{ program = "app.exe"; args = @("--id", "fake", "--status", "file", "--interval-seconds", "1", "--exit-after-seconds", "$LimitSeconds"); stop = @{ type = "terminate"; graceSeconds = 5 } }
    autoStartDefault = $true
  }
  [IO.File]::WriteAllText((Join-Path $h "modules\fake.json"), ($m | ConvertTo-Json -Depth 6), (New-Object Text.UTF8Encoding $false))
  return $h
}
function Stop-Fakes {
  Get-Process -ErrorAction SilentlyContinue | Where-Object { try { $_.Path -eq $appExe } catch { $false } } | ForEach-Object { Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue }
}

$tracked = New-Object System.Collections.Generic.List[int]
function Invoke-Tray([string]$exe, [string]$home8, [hashtable]$envVars, [string[]]$arguments) {
  $env:N8TRAY_HOME = $home8; $env:N8TRAY_NO_ACTIVATE = "1"
  foreach ($k in $envVars.Keys) { Set-Item "env:$k" $envVars[$k] }
  try {
    if ($arguments.Count) { $p = Start-Process $exe -ArgumentList $arguments -PassThru } else { $p = Start-Process $exe -PassThru }
  } finally {
    foreach ($k in @("N8TRAY_HOME", "N8TRAY_NO_ACTIVATE") + @($envVars.Keys)) { Remove-Item "env:$k" -ErrorAction SilentlyContinue }
  }
  $tracked.Add($p.Id)
  return $p
}
function Status([string]$home8) { try { Get-Content -Raw (Join-Path $home8 "status.json") | ConvertFrom-Json } catch { $null } }
function Hub-Window([int]$id) { [W]::Windows([uint32]$id) | Where-Object { $_[1] -eq "N8 Tray" } | Select-Object -First 1 }
function Picture([string]$handle, [string]$file) {
  $h = [IntPtr]::new([int64]$handle)
  $r = New-Object W+RECT; [void][W]::GetWindowRect($h, [ref]$r)
  $w = $r.R - $r.L; $hh = $r.B - $r.T
  if ($w -le 0 -or $hh -le 0) { return $null }
  $bmp = New-Object Drawing.Bitmap $w, $hh
  $g = [Drawing.Graphics]::FromImage($bmp); $dc = $g.GetHdc()
  [void][W]::PrintWindow($h, $dc, 2); $g.ReleaseHdc($dc); $g.Dispose()
  $bmp.Save($file, [Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
  $frame = New-Object W+RECT; [void][W]::DwmGetWindowAttribute($h, 9, [ref]$frame, 16)
  return [ordered]@{ rect = @($r.L, $r.T, $w, $hh); visibleFrame = @($frame.L, $frame.T, ($frame.R - $frame.L), ($frame.B - $frame.T)); exStyle = ('0x{0:X8}' -f [W]::GetWindowLongPtrW($h, -20).ToInt64()) }
}

function Measure-Run([string]$exe, [string]$tag, [hashtable]$envVars) {
  Log "run $tag ($($envVars | ConvertTo-Json -Compress))"
  $home8 = New-Home $tag
  $run = [ordered]@{ tag = $tag; env = $envVars; exe = $exe; points = [ordered]@{}; foregroundTaken = $false }
  $fg0 = [W]::GetForegroundWindow()
  $p = Invoke-Tray $exe $home8 $envVars @()
  $t0 = Get-Date
  Wait-Until "the status file" { $s = Status $home8; $s -and $s.tray.pid -eq $p.Id } 60 | Out-Null
  Wait-Until "the fake app running" { $s = Status $home8; $s -and @($s.apps | Where-Object { $_.id -eq "fake" -and $_.running }).Count -eq 1 } 60 | Out-Null
  # The start-up trim runs 30 s after start.
  Wait-Since $t0 36
  $run.points.closed = Sample $p.Id
  Log "  closed: $($run.points.closed.privateWorkingSetMb) MB private WS, $($run.points.closed.privateBytesMb) MB private bytes, $($run.points.closed.modules) modules"

  Invoke-Tray $exe $home8 $envVars @("--show") | Out-Null
  $tShow = Get-Date
  $win = Wait-Until "the window" { $w = Hub-Window $p.Id; if ($w -and $w[2] -eq "1") { $w } } 15
  $run.openMs = [int]((Get-Date) - $tShow).TotalMilliseconds
  Start-Sleep -Milliseconds 800
  $run.window = Picture $win[0] (Join-Path $out "window-$tag.png")
  $run.points.open = Sample $p.Id
  $fg = [W]::GetForegroundWindow()
  if ($fg -eq [IntPtr]::new([int64]$win[0])) { $run.foregroundTaken = $true }
  Log "  open: $($run.points.open.privateWorkingSetMb) MB private WS, $($run.points.open.privateBytesMb) MB private bytes, $($run.points.open.modules) modules, window $($run.window.rect -join ',') style $($run.window.exStyle)"

  Invoke-Tray $exe $home8 $envVars @("--hide") | Out-Null
  $tHide = Get-Date
  Wait-Until "the window hidden" { $w = Hub-Window $p.Id; -not $w -or $w[2] -eq "0" } 15 | Out-Null
  Wait-Since $tHide 5
  $run.points.hidden = Sample $p.Id 2
  Log "  hidden: $($run.points.hidden.privateWorkingSetMb) MB private WS, $($run.points.hidden.privateBytesMb) MB private bytes, windows $($run.points.hidden.topLevelWindows)"
  # Dropped (or kept) at 10 s, trimmed 2 s after a drop or at once when kept.
  Wait-Since $tHide 16
  $run.points.retired = Sample $p.Id
  Log "  after drop or keep and trim: $($run.points.retired.privateWorkingSetMb) MB private WS, $($run.points.retired.privateBytesMb) MB private bytes, windows $($run.points.retired.topLevelWindows)"

  # More opens and closes: what each one leaves behind (a leak shows as
  # private bytes that keep growing). The window is on screen about 1 s each.
  $run.cycles = @()
  for ($c = 1; $c -le $Cycles; $c++) {
    Invoke-Tray $exe $home8 $envVars @("--show") | Out-Null
    Wait-Until "the window" { $w = Hub-Window $p.Id; if ($w -and $w[2] -eq "1") { $w } } 15 | Out-Null
    Start-Sleep -Milliseconds 800
    if ([W]::GetForegroundWindow() -eq [IntPtr]::new([int64](Hub-Window $p.Id)[0])) { $run.foregroundTaken = $true }
    Invoke-Tray $exe $home8 $envVars @("--hide") | Out-Null
    $tHide = Get-Date
    Wait-Until "the window hidden" { $w = Hub-Window $p.Id; -not $w -or $w[2] -eq "0" } 15 | Out-Null
    Wait-Since $tHide 16
    $s = Sample $p.Id 2
    $run.cycles += [ordered]@{ cycle = $c; privateWorkingSetMb = $s.privateWorkingSetMb; privateBytesMb = $s.privateBytesMb; topLevelWindows = $s.topLevelWindows }
    Log "  cycle ${c}: $($s.privateWorkingSetMb) MB private WS, $($s.privateBytesMb) MB private bytes"
  }

  Invoke-Tray $exe $home8 $envVars @("--exit") | Out-Null
  try { Wait-Process -Id $p.Id -Timeout 20 } catch { Log "  did not exit; stopped"; Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
  Stop-Fakes
  $log = Get-ChildItem (Join-Path $home8 "logs") -Filter "n8tray*.log" -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($log) { $run.logLines = @(Get-Content $log.FullName | Where-Object { $_ -match "frame insets|opens at|created in|dropped|kept|started|exited|WARN|ERROR" } | ForEach-Object { [string]$_ }) }
  return $run
}

try {
  $report.machine = Machine
  Log "machine: CPU $($report.machine.cpuPercent)%, $($report.machine.freeGb) of $($report.machine.totalGb) GB free"
  # Today's tray, read only: the installed N8 Tray's own process, if it runs.
  $installed = Get-Process -Name n8tray -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($installed) {
    $report.baseline = [ordered]@{ pid = $installed.Id; path = $installed.Path; privateWorkingSetMb = [math]::Round((Private-WorkingSet $installed.Id "n8tray") / 1MB, 2); privateBytesMb = [math]::Round($installed.PrivateMemorySize64 / 1MB, 2); modules = @($installed.Modules).Count }
    Log "installed n8tray.exe now: $($report.baseline.privateWorkingSetMb) MB private WS, $($report.baseline.privateBytesMb) MB private bytes, $($report.baseline.modules) modules"
  }
  $runs = @(
    @{ tag = "drop-software"; env = @{ N8TRAY_RENDERER = "software"; N8TRAY_WINDOW = "drop" } },
    @{ tag = "drop-skia"; env = @{ N8TRAY_RENDERER = "skia-software"; N8TRAY_WINDOW = "drop" } },
    @{ tag = "keep-software"; env = @{ N8TRAY_RENDERER = "software"; N8TRAY_WINDOW = "keep" } },
    @{ tag = "keep-skia"; env = @{ N8TRAY_RENDERER = "skia-software"; N8TRAY_WINDOW = "keep" } }
  )
  foreach ($r in $runs) {
    try { $report.runs += Measure-Run $native $r.tag $r.env } catch { $report.errors += "$($r.tag): $($_.Exception.Message)"; Log "  FAILED: $($_.Exception.Message)" }
  }
  if ($Extra) {
    $extraExe = (Resolve-Path $Extra).Path
    try { $report.runs += Measure-Run $extraExe "extra-drop" @{ N8TRAY_WINDOW = "drop" } } catch { $report.errors += "extra: $($_.Exception.Message)"; Log "  FAILED: $($_.Exception.Message)" }
  }
  $report.machineAfter = Machine
} catch {
  $report.errors += $_.Exception.Message
  Log "FAILED: $($_.Exception.Message)"
} finally {
  # Only what this script started.
  foreach ($id in $tracked) { Stop-Process -Id $id -Force -ErrorAction SilentlyContinue }
  Stop-Fakes
  $report.finishedAt = (Get-Date).ToString("s")
  $report | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 (Join-Path $out "report.json")
  Log "report: $(Join-Path $out 'report.json')"
}
