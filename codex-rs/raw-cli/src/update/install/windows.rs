//! Detached Windows replacement with an explicit lock-ownership handoff.

mod handoff;
mod queued_lock;
mod restart_plan;

pub(super) use handoff::WindowsHandoff;
pub(super) use handoff::begin;

const WINDOWS_APPLY_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$target = $env:CODEX_RAW_UPDATE_TARGET
$staged = $env:CODEX_RAW_UPDATE_STAGED
$backup = $env:CODEX_RAW_UPDATE_BACKUP
$statusFile = $env:CODEX_RAW_UPDATE_STATUS
$lockPath = $env:CODEX_RAW_UPDATE_LOCK
$markerPath = $env:CODEX_RAW_UPDATE_MARKER
$waiterReady = $env:CODEX_RAW_UPDATE_WAITER_READY
$ownerReady = $env:CODEX_RAW_UPDATE_OWNER_READY
$token = $env:CODEX_RAW_UPDATE_TOKEN
$expected = $env:CODEX_RAW_UPDATE_VERSION
$parentPid = [int]$env:CODEX_RAW_UPDATE_PARENT_PID
$queuedLockSource = $env:CODEX_RAW_UPDATE_QUEUED_LOCK_SOURCE
$restartPlanSource = $env:CODEX_RAW_UPDATE_RESTART_PLAN_SOURCE
$lock = $null
$restartPlan = $null
$restartComplete = $false
$replaced = $false
$versionVerified = $false

function Test-MarkerOwnership {
  (Test-Path -LiteralPath $markerPath) -and
    ([IO.File]::ReadAllText($markerPath) -ceq $token)
}

function Write-OwnershipSignal([string]$path) {
  $temporary = "$path.$token.tmp"
  $bytes = [Text.Encoding]::UTF8.GetBytes($token)
  try {
    $stream = [IO.File]::Open($temporary, 'CreateNew', 'Write', 'Read')
    try {
      $stream.Write($bytes, 0, $bytes.Length)
      $stream.Flush($true)
    } finally {
      $stream.Dispose()
    }
    [IO.File]::Move($temporary, $path)
  } finally {
    try { [IO.File]::Delete($temporary) } catch {}
  }
}

function Get-BoundedInstalledVersion {
  $start = [Diagnostics.ProcessStartInfo]::new()
  $start.FileName = $target
  $start.Arguments = '--version'
  $start.UseShellExecute = $false
  $start.CreateNoWindow = $true
  $start.RedirectStandardOutput = $true
  $start.RedirectStandardError = $true
  $start.EnvironmentVariables.Clear()
  foreach ($name in @('SystemRoot', 'TEMP', 'TMP', 'WINDIR')) {
    $value = [Environment]::GetEnvironmentVariable($name)
    if ($null -ne $value) {
      $start.EnvironmentVariables[$name] = $value
    }
  }

  $process = [Diagnostics.Process]::new()
  $process.StartInfo = $start
  try {
    if (-not $process.Start()) {
      throw 'Failed to start the installed binary version probe.'
    }
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    if (-not $process.WaitForExit(15000)) {
      try { $process.Kill() } catch {}
      [void]$process.WaitForExit(5000)
      throw 'Installed binary version probe exceeded its time limit.'
    }
    $reads = [Threading.Tasks.Task[]]@($stdout, $stderr)
    if (-not [Threading.Tasks.Task]::WaitAll($reads, 5000)) {
      throw 'Installed binary version probe output did not close in time.'
    }
    if ($process.ExitCode -ne 0) {
      throw "Installed binary version probe failed: $($stderr.Result.Trim())"
    }
    return $stdout.Result.Trim()
  } finally {
    $process.Dispose()
  }
}

$restartManagerSource = @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using ComFileTime = System.Runtime.InteropServices.ComTypes.FILETIME;

public static class CodexRawRestartManager
{
    private const int ErrorSuccess = 0;
    private const int ErrorMoreData = 234;
    private const uint ProcessQueryLimitedInformation = 0x1000;

    [StructLayout(LayoutKind.Sequential)]
    public struct RmUniqueProcess
    {
        public int ProcessId;
        public ComFileTime ProcessStartTime;
    }

    private enum RmAppType
    {
        Unknown = 0,
        MainWindow = 1,
        OtherWindow = 2,
        Service = 3,
        Explorer = 4,
        Console = 5,
        Critical = 1000
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct RmProcessInfo
    {
        public RmUniqueProcess Process;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 256)]
        public string AppName;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 64)]
        public string ServiceShortName;
        public RmAppType ApplicationType;
        public uint AppStatus;
        public uint TerminalSessionId;
        [MarshalAs(UnmanagedType.Bool)]
        public bool Restartable;
    }

    [DllImport("rstrtmgr.dll", CharSet = CharSet.Unicode)]
    private static extern int RmStartSession(
        out uint sessionHandle,
        int sessionFlags,
        StringBuilder sessionKey);

    [DllImport("rstrtmgr.dll")]
    private static extern int RmEndSession(uint sessionHandle);

    [DllImport("rstrtmgr.dll", CharSet = CharSet.Unicode)]
    private static extern int RmRegisterResources(
        uint sessionHandle,
        uint fileCount,
        string[] fileNames,
        uint applicationCount,
        [In] RmUniqueProcess[] applications,
        uint serviceCount,
        string[] serviceNames);

    [DllImport("rstrtmgr.dll")]
    private static extern int RmGetList(
        uint sessionHandle,
        out uint processInfoNeeded,
        ref uint processInfoCount,
        [In, Out] RmProcessInfo[] affectedApps,
        ref uint rebootReasons);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern IntPtr OpenProcess(
        uint desiredAccess,
        bool inheritHandle,
        int processId);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool QueryFullProcessImageName(
        IntPtr process,
        int flags,
        StringBuilder executableName,
        ref int size);

    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetProcessTimes(
        IntPtr process,
        out ComFileTime creation,
        out ComFileTime exit,
        out ComFileTime kernel,
        out ComFileTime user);

    [DllImport("kernel32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool CloseHandle(IntPtr handle);

    public static long[] DiscoverExact(string executable)
    {
        RmUniqueProcess[] exact =
            DiscoverAndValidate(executable, NormalizePath(executable));
        var encoded = new long[checked(exact.Length * 3)];
        for (int index = 0; index < exact.Length; index++)
        {
            encoded[index * 3] = (uint)exact[index].ProcessId;
            encoded[index * 3 + 1] =
                (uint)exact[index].ProcessStartTime.dwLowDateTime;
            encoded[index * 3 + 2] =
                (uint)exact[index].ProcessStartTime.dwHighDateTime;
        }
        return encoded;
    }

    private static RmUniqueProcess[] DiscoverAndValidate(
        string executable,
        string expected)
    {
        uint session = StartSession();
        try
        {
            Check(RmRegisterResources(
                session,
                1,
                new[] { executable },
                0,
                null,
                0,
                null));
            RmProcessInfo[] affected = GetAffectedProcesses(session);
            var exact = new List<RmUniqueProcess>(affected.Length);
            foreach (RmProcessInfo info in affected)
            {
                string actual = NormalizePath(ProcessImage(info.Process));
                if (!String.Equals(actual, expected, StringComparison.OrdinalIgnoreCase))
                {
                    throw new InvalidOperationException(
                        "Restart Manager reported a resource user whose image is not the exact codex-raw.exe target.");
                }
                exact.Add(info.Process);
            }
            return exact.ToArray();
        }
        finally
        {
            RmEndSession(session);
        }
    }

    private static RmProcessInfo[] GetAffectedProcesses(uint session)
    {
        uint needed;
        uint count = 0;
        uint reasons = 0;
        int result = RmGetList(session, out needed, ref count, null, ref reasons);
        if (result == ErrorSuccess)
        {
            return new RmProcessInfo[0];
        }
        if (result != ErrorMoreData)
        {
            Check(result);
        }

        for (int attempt = 0; attempt < 3; attempt++)
        {
            if (needed > Int32.MaxValue)
            {
                throw new InvalidOperationException(
                    "Restart Manager returned an oversized process list.");
            }
            var processes = new RmProcessInfo[(int)needed];
            count = needed;
            result = RmGetList(
                session,
                out needed,
                ref count,
                processes,
                ref reasons);
            if (result == ErrorSuccess)
            {
                if (count == (uint)processes.Length)
                {
                    return processes;
                }
                if (count > Int32.MaxValue)
                {
                    throw new InvalidOperationException(
                        "Restart Manager returned an oversized process count.");
                }
                var exactCount = new RmProcessInfo[(int)count];
                Array.Copy(processes, exactCount, (int)count);
                return exactCount;
            }
            if (result != ErrorMoreData)
            {
                Check(result);
            }
        }
        throw new InvalidOperationException(
            "Restart Manager process list changed repeatedly.");
    }

    private static string ProcessImage(RmUniqueProcess process)
    {
        IntPtr handle = OpenProcess(
            ProcessQueryLimitedInformation,
            false,
            process.ProcessId);
        if (handle == IntPtr.Zero)
        {
            throw new Win32Exception(Marshal.GetLastWin32Error());
        }
        try
        {
            ComFileTime creation;
            ComFileTime exit;
            ComFileTime kernel;
            ComFileTime user;
            if (!GetProcessTimes(
                handle,
                out creation,
                out exit,
                out kernel,
                out user))
            {
                throw new Win32Exception(Marshal.GetLastWin32Error());
            }
            if (FileTimeValue(creation) != FileTimeValue(process.ProcessStartTime))
            {
                throw new InvalidOperationException(
                    "Restart Manager process identity changed before validation.");
            }

            var path = new StringBuilder(32768);
            int size = path.Capacity;
            if (!QueryFullProcessImageName(handle, 0, path, ref size))
            {
                throw new Win32Exception(Marshal.GetLastWin32Error());
            }
            return path.ToString();
        }
        finally
        {
            CloseHandle(handle);
        }
    }

    private static long FileTimeValue(ComFileTime value)
    {
        return ((long)value.dwHighDateTime << 32)
            | (uint)value.dwLowDateTime;
    }

    private static string NormalizePath(string path)
    {
        string normalized = path;
        if (normalized.StartsWith(
            @"\\?\UNC\",
            StringComparison.OrdinalIgnoreCase))
        {
            normalized = @"\\" + normalized.Substring(8);
        }
        else if (normalized.StartsWith(
            @"\\?\",
            StringComparison.OrdinalIgnoreCase))
        {
            normalized = normalized.Substring(4);
        }
        return Path.GetFullPath(normalized)
            .TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);
    }

    private static uint StartSession()
    {
        uint session;
        var key = new StringBuilder(33);
        Check(RmStartSession(out session, 0, key));
        return session;
    }

    private static void Check(int result)
    {
        if (result != ErrorSuccess)
        {
            throw new Win32Exception(result);
        }
    }
}
'@

try {
  if (-not (Test-MarkerOwnership)) {
    throw 'Windows update handoff marker ownership check failed.'
  }
  Add-Type -TypeDefinition $queuedLockSource -Language CSharp
  Add-Type -TypeDefinition $restartManagerSource -Language CSharp
  Add-Type -TypeDefinition $restartPlanSource -Language CSharp

  $lock = [CodexRawQueuedFileLock]::Arm($lockPath)
  Write-OwnershipSignal $waiterReady
  $lock.WaitUntilOwned(120000)
  if (-not (Test-MarkerOwnership)) {
    throw 'Windows update handoff marker changed before lock ownership transferred.'
  }
  Write-OwnershipSignal $ownerReady

  try {
    $parent = [Diagnostics.Process]::GetProcessById($parentPid)
    if (-not $parent.WaitForExit(30000)) {
      throw 'Timed out waiting for the parent codex-raw updater to exit.'
    }
  } catch [ArgumentException] {
    # The parent already exited.
  }

  $exactProcesses = [CodexRawRestartManager]::DiscoverExact($target)
  $restartPlan = [CodexRawRestartPlan]::Create($exactProcesses)
  $restartPlan.Shutdown(30000)

  $acl = [IO.File]::GetAccessControl($target)
  [IO.File]::SetAccessControl($staged, $acl)
  $replaceDeadline = [DateTime]::UtcNow.AddSeconds(120)
  while (-not $replaced) {
    try {
      [IO.File]::Replace($staged, $target, $backup, $true)
      $replaced = $true
    } catch {
      if ([DateTime]::UtcNow -ge $replaceDeadline) {
        throw 'Timed out replacing the exact codex-raw.exe target. No process was force-killed.'
      }
      Start-Sleep -Milliseconds 250
    }
  }

  $actual = Get-BoundedInstalledVersion
  if ($actual -ne "codex-raw $expected") {
    throw "Installed binary version check failed: $actual"
  }
  $versionVerified = $true
  $restartPlan.Restart(30000)
  $restartComplete = $true
} catch {
  $failure = $_.Exception.Message
  if ($replaced -and -not $versionVerified -and
      (Test-Path -LiteralPath $backup)) {
    try {
      [IO.File]::Replace($backup, $target, $staged, $true)
      Remove-Item -LiteralPath $staged -Force -ErrorAction SilentlyContinue
    } catch {
      $failure = "$failure`nRollback also failed: $($_.Exception.Message)"
    }
  }
  if ($null -ne $restartPlan -and -not $restartComplete) {
    try {
      $restartPlan.Restart(30000)
      $restartComplete = $true
    } catch {
      $failure = "$failure`nRestart also failed: $($_.Exception.Message)"
    }
  }
  [IO.File]::WriteAllText(
    $statusFile,
    "codex-raw update failed:`n$failure",
    [Text.Encoding]::UTF8)
  exit 1
} finally {
  if ($null -ne $restartPlan) { $restartPlan.Dispose() }
  if ($null -ne $lock) { $lock.Dispose() }
  if (Test-MarkerOwnership) {
    Remove-Item -LiteralPath $markerPath -Force -ErrorAction SilentlyContinue
  }
  Remove-Item -LiteralPath $waiterReady -Force -ErrorAction SilentlyContinue
  Remove-Item -LiteralPath $ownerReady -Force -ErrorAction SilentlyContinue
}
"#;
