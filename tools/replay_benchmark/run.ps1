[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Manifest,
    [string]$AppBinary,
    [string]$AnalyzerPath,
    [string]$PythonPath,
    [ValidateRange(0, 86400)]
    [int]$TimeoutSeconds = 0,
    [switch]$PreflightOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$script:SentinelName = ".chronobreak-replay-benchmark"
$script:ResultRootCreated = $null
$script:AppProcess = $null
$script:AppOutput = $null
$script:KnownProcesses = @{}
$script:WebViewVersions = @{}
$script:BenchmarkJob = [IntPtr]::Zero
$script:BenchmarkCompletionPort = [IntPtr]::Zero
$script:ForcedTerminationReason = $null
$script:PreviousJobMembers = @{}
$script:JobNotifications = New-Object 'System.Collections.Generic.List[object]'
$script:JobNewProcessNotificationCount = [uint64]0
$script:WebViewVersionPaths = @{}
$script:GpuCollectionEnabled = $false
$script:GpuCollectionLimitation = "GPU collection has not been initialized."
$script:AllowedTopLevel = @(
    "schema_version", "run_id", "sentinel_root", "library_root", "config_path",
    "app_data_root", "result_root", "scratch_root", "observer_profile", "ddragon",
    "fixtures", "scenarios", "app_binary", "analyzer_path", "python_path", "timeout_seconds",
    "prepared_utc", "preparation_receipt", "media_tools"
)
$script:RequiredTopLevel = @(
    "schema_version", "run_id", "sentinel_root", "library_root", "config_path",
    "app_data_root", "result_root", "scratch_root", "observer_profile", "ddragon",
    "fixtures", "scenarios"
)
$script:AllowedScenarioFields = @(
    "id", "kind", "fixture_ids", "trial_id", "seed", "warmup_seconds",
    "duration_seconds", "idle_seconds", "target_times_ms", "rates", "request_rate_hz",
    "iterations", "distance_classes", "export_presets", "music_modes", "gain_modes",
    "endpoint_alignment", "observer_control", "seek_reason", "seek_playback_mode", "clip_start_ms", "clip_end_ms",
    "expected_duration_ms", "duration_tolerance_ms", "expected_video_codec",
    "expected_audio_codec", "music_mode", "gain_mode", "built_in_music_filename"
)
$script:ScenarioKinds = @(
    "app_idle", "cold_open", "warm_open", "play_pause", "rate", "seek", "scrub",
    "layout", "lifecycle", "export"
)

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

public sealed class QueueBackReplayJobSnapshot
{
    public ulong CpuTime100ns { get; set; }
    public uint TotalProcesses { get; set; }
    public uint ActiveProcesses { get; set; }
    public uint TotalTerminatedProcesses { get; set; }
    public ulong ReadOperations { get; set; }
    public ulong WriteOperations { get; set; }
    public ulong OtherOperations { get; set; }
    public ulong ReadBytes { get; set; }
    public ulong WriteBytes { get; set; }
    public ulong OtherBytes { get; set; }
    public long[] ProcessIds { get; set; }
}

public sealed class QueueBackReplaySystemTimes
{
    public ulong IdleTime100ns { get; set; }
    public ulong KernelTime100ns { get; set; }
    public ulong UserTime100ns { get; set; }
}

public sealed class QueueBackReplayJobNotification
{
    public uint MessageId { get; set; }
    public string Message { get; set; }
    public long ProcessId { get; set; }
    public DateTime ObservedUtc { get; set; }
}

public sealed class QueueBackReplayProcessSnapshot
{
    public int ProcessId { get; set; }
    public int ParentProcessId { get; set; }
    public DateTime CreationDate { get; set; }
    public string Name { get; set; }
    public string ExecutablePath { get; set; }
    public ulong KernelModeTime { get; set; }
    public ulong UserModeTime { get; set; }
    public ulong PrivatePageCount { get; set; }
    public ulong WorkingSetSize { get; set; }
    public ulong ReadOperationCount { get; set; }
    public ulong WriteOperationCount { get; set; }
    public ulong OtherOperationCount { get; set; }
    public ulong ReadTransferCount { get; set; }
    public ulong WriteTransferCount { get; set; }
    public ulong OtherTransferCount { get; set; }
    public uint HandleCount { get; set; }
    public uint ThreadCount { get; set; }
}

public static class QueueBackReplayJob
{
    private const uint CREATE_NEW_PROCESS_GROUP = 0x00000200;
    private const uint CREATE_NO_WINDOW = 0x08000000;
    private const uint CREATE_SUSPENDED = 0x00000004;
    private const uint FILE_ATTRIBUTE_NORMAL = 0x00000080;
    private const uint FILE_SHARE_READ = 0x00000001;
    private const uint GENERIC_READ = 0x80000000;
    private const uint GENERIC_WRITE = 0x40000000;
    private const uint JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = 0x00002000;
    private const uint MOVEFILE_REPLACE_EXISTING = 0x00000001;
    private const uint MOVEFILE_WRITE_THROUGH = 0x00000008;
    private const uint PROCESS_QUERY_INFORMATION = 0x00000400;
    private const uint PROCESS_QUERY_LIMITED_INFORMATION = 0x00001000;
    private const uint PROCESS_VM_READ = 0x00000010;
    private const uint STARTF_USESHOWWINDOW = 0x00000001;
    private const uint STARTF_USESTDHANDLES = 0x00000100;
    private const uint TH32CS_SNAPPROCESS = 0x00000002;
    private const int CREATE_NEW = 1;
    private const int JobObjectBasicProcessIdList = 3;
    private const int JobObjectAssociateCompletionPortInformation = 7;
    private const int JobObjectBasicAndIoAccountingInformation = 8;
    private const int JobObjectExtendedLimitInformation = 9;
    private const int OPEN_EXISTING = 3;
    private const int ERROR_MORE_DATA = 234;
    private const int WAIT_TIMEOUT = 258;
    private const short SW_HIDE = 0;
    private static readonly IntPtr InvalidHandleValue = new IntPtr(-1);

    [StructLayout(LayoutKind.Sequential)]
    private struct SecurityAttributes
    {
        public int Length;
        public IntPtr SecurityDescriptor;
        [MarshalAs(UnmanagedType.Bool)]
        public bool InheritHandle;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct StartupInfo
    {
        public int Size;
        public IntPtr Reserved;
        public IntPtr Desktop;
        public IntPtr Title;
        public uint X;
        public uint Y;
        public uint XSize;
        public uint YSize;
        public uint XCountChars;
        public uint YCountChars;
        public uint FillAttribute;
        public uint Flags;
        public short ShowWindow;
        public short Reserved2Size;
        public IntPtr Reserved2;
        public IntPtr StandardInput;
        public IntPtr StandardOutput;
        public IntPtr StandardError;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct ProcessInformation
    {
        public IntPtr Process;
        public IntPtr Thread;
        public uint ProcessId;
        public uint ThreadId;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct ProcessEntry32
    {
        public uint Size;
        public uint Usage;
        public uint ProcessId;
        public IntPtr DefaultHeapId;
        public uint ModuleId;
        public uint Threads;
        public uint ParentProcessId;
        public int PriorityClassBase;
        public uint Flags;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 260)]
        public string ExeFile;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct FileTime
    {
        public uint Low;
        public uint High;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct IoCounters
    {
        public ulong ReadOperationCount;
        public ulong WriteOperationCount;
        public ulong OtherOperationCount;
        public ulong ReadTransferCount;
        public ulong WriteTransferCount;
        public ulong OtherTransferCount;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct ProcessMemoryCountersEx
    {
        public uint Size;
        public uint PageFaultCount;
        public UIntPtr PeakWorkingSetSize;
        public UIntPtr WorkingSetSize;
        public UIntPtr QuotaPeakPagedPoolUsage;
        public UIntPtr QuotaPagedPoolUsage;
        public UIntPtr QuotaPeakNonPagedPoolUsage;
        public UIntPtr QuotaNonPagedPoolUsage;
        public UIntPtr PagefileUsage;
        public UIntPtr PeakPagefileUsage;
        public UIntPtr PrivateUsage;
    }

    private sealed class ProcessEntrySnapshot
    {
        public int ParentProcessId;
        public uint ThreadCount;
        public string Name;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct BasicAccounting
    {
        public long TotalUserTime;
        public long TotalKernelTime;
        public long ThisPeriodTotalUserTime;
        public long ThisPeriodTotalKernelTime;
        public uint TotalPageFaultCount;
        public uint TotalProcesses;
        public uint ActiveProcesses;
        public uint TotalTerminatedProcesses;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct BasicAndIoAccounting
    {
        public BasicAccounting BasicInfo;
        public IoCounters IoInfo;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct BasicLimitInformation
    {
        public long PerProcessUserTimeLimit;
        public long PerJobUserTimeLimit;
        public uint LimitFlags;
        public UIntPtr MinimumWorkingSetSize;
        public UIntPtr MaximumWorkingSetSize;
        public uint ActiveProcessLimit;
        public UIntPtr Affinity;
        public uint PriorityClass;
        public uint SchedulingClass;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct ExtendedLimitInformation
    {
        public BasicLimitInformation BasicLimitInformation;
        public IoCounters IoInfo;
        public UIntPtr ProcessMemoryLimit;
        public UIntPtr JobMemoryLimit;
        public UIntPtr PeakProcessMemoryUsed;
        public UIntPtr PeakJobMemoryUsed;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct JobObjectAssociateCompletionPort
    {
        public IntPtr CompletionKey;
        public IntPtr CompletionPort;
    }

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr CreateJobObject(IntPtr securityAttributes, string name);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr CreateFile(
        string fileName,
        uint desiredAccess,
        uint shareMode,
        ref SecurityAttributes securityAttributes,
        int creationDisposition,
        uint flagsAndAttributes,
        IntPtr templateFile);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool CreateProcess(
        string applicationName,
        StringBuilder commandLine,
        IntPtr processAttributes,
        IntPtr threadAttributes,
        [MarshalAs(UnmanagedType.Bool)] bool inheritHandles,
        uint creationFlags,
        IntPtr environment,
        string currentDirectory,
        ref StartupInfo startupInfo,
        out ProcessInformation processInformation);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool SetInformationJobObject(
        IntPtr job,
        int informationClass,
        IntPtr information,
        uint informationLength);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool QueryInformationJobObject(
        IntPtr job,
        int informationClass,
        IntPtr information,
        uint informationLength,
        out uint returnLength);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool TerminateJobObject(IntPtr job, uint exitCode);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool TerminateProcess(IntPtr process, uint exitCode);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern IntPtr CreateIoCompletionPort(
        IntPtr fileHandle,
        IntPtr existingCompletionPort,
        UIntPtr completionKey,
        uint numberOfConcurrentThreads);

    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetQueuedCompletionStatus(
        IntPtr completionPort,
        out uint numberOfBytesTransferred,
        out UIntPtr completionKey,
        out IntPtr overlapped,
        uint milliseconds);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern uint ResumeThread(IntPtr thread);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern IntPtr CreateToolhelp32Snapshot(uint flags, uint processId);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool Process32First(IntPtr snapshot, ref ProcessEntry32 entry);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool Process32Next(IntPtr snapshot, ref ProcessEntry32 entry);

    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetProcessIoCounters(IntPtr process, out IoCounters counters);

    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetProcessTimes(
        IntPtr process,
        out FileTime creation,
        out FileTime exit,
        out FileTime kernel,
        out FileTime user);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern IntPtr OpenProcess(
        uint desiredAccess,
        [MarshalAs(UnmanagedType.Bool)] bool inheritHandle,
        int processId);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool QueryFullProcessImageName(
        IntPtr process,
        uint flags,
        StringBuilder executablePath,
        ref uint size);

    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetProcessHandleCount(IntPtr process, out uint handleCount);

    [DllImport("psapi.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetProcessMemoryInfo(
        IntPtr process,
        out ProcessMemoryCountersEx counters,
        uint size);

    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetSystemTimes(
        out FileTime idle,
        out FileTime kernel,
        out FileTime user);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool CloseHandle(IntPtr handle);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool MoveFileEx(string existingPath, string newPath, uint flags);

    public static IntPtr CreateKillOnClose()
    {
        IntPtr job = CreateJobObject(IntPtr.Zero, null);
        if (job == IntPtr.Zero)
            throw new Win32Exception(Marshal.GetLastWin32Error(), "CreateJobObject failed");
        ExtendedLimitInformation limits = new ExtendedLimitInformation();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        int size = Marshal.SizeOf(typeof(ExtendedLimitInformation));
        IntPtr buffer = Marshal.AllocHGlobal(size);
        try
        {
            Marshal.StructureToPtr(limits, buffer, false);
            if (!SetInformationJobObject(job, JobObjectExtendedLimitInformation, buffer, (uint)size))
                throw new Win32Exception(Marshal.GetLastWin32Error(), "SetInformationJobObject failed");
            return job;
        }
        catch
        {
            CloseHandle(job);
            throw;
        }
        finally
        {
            Marshal.FreeHGlobal(buffer);
        }
    }

    public static void Assign(IntPtr job, IntPtr process)
    {
        if (!AssignProcessToJobObject(job, process))
            throw new Win32Exception(Marshal.GetLastWin32Error(), "AssignProcessToJobObject failed");
    }

    public static IntPtr CreateCompletionPort()
    {
        IntPtr port = CreateIoCompletionPort(InvalidHandleValue, IntPtr.Zero, UIntPtr.Zero, 1);
        if (port == IntPtr.Zero)
            throw new Win32Exception(Marshal.GetLastWin32Error(), "CreateIoCompletionPort failed");
        return port;
    }

    public static void AssociateCompletionPort(IntPtr job, IntPtr completionPort)
    {
        JobObjectAssociateCompletionPort association = new JobObjectAssociateCompletionPort
        {
            CompletionKey = job,
            CompletionPort = completionPort
        };
        int size = Marshal.SizeOf(typeof(JobObjectAssociateCompletionPort));
        IntPtr buffer = Marshal.AllocHGlobal(size);
        try
        {
            Marshal.StructureToPtr(association, buffer, false);
            if (!SetInformationJobObject(
                job,
                JobObjectAssociateCompletionPortInformation,
                buffer,
                (uint)size))
                throw new Win32Exception(Marshal.GetLastWin32Error(), "Job completion-port association failed");
        }
        finally
        {
            Marshal.FreeHGlobal(buffer);
        }
    }

    public static QueueBackReplayJobNotification[] DrainNotifications(
        IntPtr completionPort,
        uint initialWaitMilliseconds)
    {
        List<QueueBackReplayJobNotification> result = new List<QueueBackReplayJobNotification>();
        uint wait = initialWaitMilliseconds;
        while (true)
        {
            uint message;
            UIntPtr completionKey;
            IntPtr value;
            if (!GetQueuedCompletionStatus(
                completionPort,
                out message,
                out completionKey,
                out value,
                wait))
            {
                int error = Marshal.GetLastWin32Error();
                if (error == WAIT_TIMEOUT)
                    break;
                throw new Win32Exception(error, "GetQueuedCompletionStatus failed");
            }
            result.Add(new QueueBackReplayJobNotification
            {
                MessageId = message,
                Message = JobMessageName(message),
                ProcessId = value.ToInt64(),
                ObservedUtc = DateTime.UtcNow
            });
            wait = 0;
        }
        return result.ToArray();
    }

    private static string JobMessageName(uint message)
    {
        switch (message)
        {
            case 1: return "end_of_job_time";
            case 2: return "end_of_process_time";
            case 3: return "active_process_limit";
            case 4: return "active_process_zero";
            case 6: return "new_process";
            case 7: return "exit_process";
            case 8: return "abnormal_exit_process";
            case 9: return "process_memory_limit";
            case 10: return "job_memory_limit";
            case 11: return "notification_limit";
            case 12: return "job_cycle_time_limit";
            default: return "message_" + message.ToString();
        }
    }

    public static Process StartSuspended(
        IntPtr job,
        string applicationPath,
        string argumentLine,
        string stdoutPath,
        string stderrPath)
    {
        SecurityAttributes inheritable = new SecurityAttributes
        {
            Length = Marshal.SizeOf(typeof(SecurityAttributes)),
            SecurityDescriptor = IntPtr.Zero,
            InheritHandle = true
        };
        IntPtr stdout = InvalidHandleValue;
        IntPtr stderr = InvalidHandleValue;
        IntPtr stdin = InvalidHandleValue;
        ProcessInformation native = new ProcessInformation();
        Process managed = null;
        bool resumed = false;
        try
        {
            stdout = CreateFile(
                stdoutPath,
                GENERIC_WRITE,
                FILE_SHARE_READ,
                ref inheritable,
                CREATE_NEW,
                FILE_ATTRIBUTE_NORMAL,
                IntPtr.Zero);
            ThrowIfInvalid(stdout, "CreateFile(stdout) failed");
            stderr = CreateFile(
                stderrPath,
                GENERIC_WRITE,
                FILE_SHARE_READ,
                ref inheritable,
                CREATE_NEW,
                FILE_ATTRIBUTE_NORMAL,
                IntPtr.Zero);
            ThrowIfInvalid(stderr, "CreateFile(stderr) failed");
            stdin = CreateFile(
                "NUL",
                GENERIC_READ,
                FILE_SHARE_READ,
                ref inheritable,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                IntPtr.Zero);
            ThrowIfInvalid(stdin, "CreateFile(NUL) failed");

            StartupInfo startup = new StartupInfo
            {
                Size = Marshal.SizeOf(typeof(StartupInfo)),
                Flags = STARTF_USESHOWWINDOW | STARTF_USESTDHANDLES,
                ShowWindow = SW_HIDE,
                StandardInput = stdin,
                StandardOutput = stdout,
                StandardError = stderr
            };
            StringBuilder commandLine = new StringBuilder();
            commandLine.Append('"').Append(applicationPath).Append('"');
            if (!String.IsNullOrWhiteSpace(argumentLine))
                commandLine.Append(' ').Append(argumentLine);

            if (!CreateProcess(
                applicationPath,
                commandLine,
                IntPtr.Zero,
                IntPtr.Zero,
                true,
                CREATE_SUSPENDED | CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP,
                IntPtr.Zero,
                Path.GetDirectoryName(applicationPath),
                ref startup,
                out native))
                throw new Win32Exception(Marshal.GetLastWin32Error(), "CreateProcess(suspended) failed");

            Assign(job, native.Process);
            managed = Process.GetProcessById(checked((int)native.ProcessId));
            IntPtr managedHandle = managed.Handle;
            if (ResumeThread(native.Thread) == UInt32.MaxValue)
                throw new Win32Exception(Marshal.GetLastWin32Error(), "ResumeThread failed");
            resumed = true;
            return managed;
        }
        catch
        {
            if (native.Process != IntPtr.Zero && !resumed)
                TerminateProcess(native.Process, 2);
            if (managed != null)
                managed.Dispose();
            throw;
        }
        finally
        {
            CloseIgnoring(native.Thread);
            CloseIgnoring(native.Process);
            CloseIgnoring(stdin);
            CloseIgnoring(stderr);
            CloseIgnoring(stdout);
        }
    }

    private static void ThrowIfInvalid(IntPtr handle, string operation)
    {
        if (handle == IntPtr.Zero || handle == InvalidHandleValue)
            throw new Win32Exception(Marshal.GetLastWin32Error(), operation);
    }

    private static void CloseIgnoring(IntPtr handle)
    {
        if (handle != IntPtr.Zero && handle != InvalidHandleValue)
            CloseHandle(handle);
    }

    public static QueueBackReplayJobSnapshot Snapshot(IntPtr job)
    {
        BasicAndIoAccounting accounting = QueryAccounting(job);
        return new QueueBackReplayJobSnapshot
        {
            CpuTime100ns = checked((ulong)(accounting.BasicInfo.TotalUserTime + accounting.BasicInfo.TotalKernelTime)),
            TotalProcesses = accounting.BasicInfo.TotalProcesses,
            ActiveProcesses = accounting.BasicInfo.ActiveProcesses,
            TotalTerminatedProcesses = accounting.BasicInfo.TotalTerminatedProcesses,
            ReadOperations = accounting.IoInfo.ReadOperationCount,
            WriteOperations = accounting.IoInfo.WriteOperationCount,
            OtherOperations = accounting.IoInfo.OtherOperationCount,
            ReadBytes = accounting.IoInfo.ReadTransferCount,
            WriteBytes = accounting.IoInfo.WriteTransferCount,
            OtherBytes = accounting.IoInfo.OtherTransferCount,
            ProcessIds = QueryProcessIds(job)
        };
    }

    public static QueueBackReplaySystemTimes SnapshotSystemTimes()
    {
        FileTime idle;
        FileTime kernel;
        FileTime user;
        if (!GetSystemTimes(out idle, out kernel, out user))
            throw new Win32Exception(Marshal.GetLastWin32Error(), "GetSystemTimes failed");
        return new QueueBackReplaySystemTimes
        {
            IdleTime100ns = ToUInt64(idle),
            KernelTime100ns = ToUInt64(kernel),
            UserTime100ns = ToUInt64(user)
        };
    }

    public static QueueBackReplayProcessSnapshot[] SnapshotProcesses(IntPtr job)
    {
        Dictionary<int, ProcessEntrySnapshot> entries = SnapshotProcessEntries();
        List<QueueBackReplayProcessSnapshot> result = new List<QueueBackReplayProcessSnapshot>();
        foreach (long rawProcessId in QueryProcessIds(job))
        {
            if (rawProcessId <= 0 || rawProcessId > Int32.MaxValue)
                continue;
            int processId = (int)rawProcessId;
            IntPtr handle = IntPtr.Zero;
            try
            {
                handle = OpenProcess(
                    PROCESS_QUERY_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
                    false,
                    processId);
                if (handle == IntPtr.Zero)
                    throw new Win32Exception(Marshal.GetLastWin32Error(), "OpenProcess failed");
                IoCounters io;
                if (!GetProcessIoCounters(handle, out io))
                    throw new Win32Exception(Marshal.GetLastWin32Error(), "GetProcessIoCounters failed");
                FileTime creation;
                FileTime exit;
                FileTime kernel;
                FileTime user;
                if (!GetProcessTimes(handle, out creation, out exit, out kernel, out user))
                    throw new Win32Exception(Marshal.GetLastWin32Error(), "GetProcessTimes failed");
                ProcessMemoryCountersEx memory = new ProcessMemoryCountersEx();
                memory.Size = checked((uint)Marshal.SizeOf(typeof(ProcessMemoryCountersEx)));
                if (!GetProcessMemoryInfo(handle, out memory, memory.Size))
                    throw new Win32Exception(Marshal.GetLastWin32Error(), "GetProcessMemoryInfo failed");
                uint handleCount;
                if (!GetProcessHandleCount(handle, out handleCount))
                    throw new Win32Exception(Marshal.GetLastWin32Error(), "GetProcessHandleCount failed");
                string executablePath = null;
                uint pathCapacity = 32768;
                StringBuilder path = new StringBuilder(checked((int)pathCapacity));
                if (QueryFullProcessImageName(handle, 0, path, ref pathCapacity))
                    executablePath = path.ToString();
                ProcessEntrySnapshot entry;
                entries.TryGetValue(processId, out entry);
                int parentProcessId = entry == null ? 0 : entry.ParentProcessId;
                string name = entry == null ? Path.GetFileName(executablePath) : entry.Name;
                if (String.IsNullOrWhiteSpace(name))
                    name = "process-" + processId.ToString();
                if (!name.EndsWith(".exe", StringComparison.OrdinalIgnoreCase))
                    name += ".exe";
                result.Add(new QueueBackReplayProcessSnapshot
                {
                    ProcessId = processId,
                    ParentProcessId = parentProcessId,
                    CreationDate = DateTime.FromFileTimeUtc(unchecked((long)ToUInt64(creation))),
                    Name = name,
                    ExecutablePath = executablePath,
                    KernelModeTime = ToUInt64(kernel),
                    UserModeTime = ToUInt64(user),
                    PrivatePageCount = memory.PrivateUsage.ToUInt64(),
                    WorkingSetSize = memory.WorkingSetSize.ToUInt64(),
                    ReadOperationCount = io.ReadOperationCount,
                    WriteOperationCount = io.WriteOperationCount,
                    OtherOperationCount = io.OtherOperationCount,
                    ReadTransferCount = io.ReadTransferCount,
                    WriteTransferCount = io.WriteTransferCount,
                    OtherTransferCount = io.OtherTransferCount,
                    HandleCount = handleCount,
                    ThreadCount = entry == null ? 0 : entry.ThreadCount
                });
            }
            catch (Win32Exception) { }
            finally
            {
                CloseIgnoring(handle);
            }
        }
        return result.ToArray();
    }

    private static Dictionary<int, ProcessEntrySnapshot> SnapshotProcessEntries()
    {
        Dictionary<int, ProcessEntrySnapshot> entries = new Dictionary<int, ProcessEntrySnapshot>();
        IntPtr snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if (snapshot == InvalidHandleValue)
            return entries;
        try
        {
            ProcessEntry32 entry = new ProcessEntry32();
            entry.Size = checked((uint)Marshal.SizeOf(typeof(ProcessEntry32)));
            if (!Process32First(snapshot, ref entry))
                return entries;
            do
            {
                if (entry.ProcessId <= Int32.MaxValue && entry.ParentProcessId <= Int32.MaxValue)
                    entries[(int)entry.ProcessId] = new ProcessEntrySnapshot
                    {
                        ParentProcessId = (int)entry.ParentProcessId,
                        ThreadCount = entry.Threads,
                        Name = entry.ExeFile
                    };
                entry.Size = checked((uint)Marshal.SizeOf(typeof(ProcessEntry32)));
            }
            while (Process32Next(snapshot, ref entry));
            return entries;
        }
        finally
        {
            CloseIgnoring(snapshot);
        }
    }

    private static ulong ToUInt64(FileTime value)
    {
        return ((ulong)value.High << 32) | value.Low;
    }

    private static BasicAndIoAccounting QueryAccounting(IntPtr job)
    {
        int size = Marshal.SizeOf(typeof(BasicAndIoAccounting));
        IntPtr buffer = Marshal.AllocHGlobal(size);
        try
        {
            uint returned;
            if (!QueryInformationJobObject(job, JobObjectBasicAndIoAccountingInformation, buffer, (uint)size, out returned))
                throw new Win32Exception(Marshal.GetLastWin32Error(), "job accounting query failed");
            return (BasicAndIoAccounting)Marshal.PtrToStructure(buffer, typeof(BasicAndIoAccounting));
        }
        finally
        {
            Marshal.FreeHGlobal(buffer);
        }
    }

    private static long[] QueryProcessIds(IntPtr job)
    {
        int capacity = 64;
        while (true)
        {
            int size = 8 + IntPtr.Size * capacity;
            IntPtr buffer = Marshal.AllocHGlobal(size);
            try
            {
                uint returned;
                if (QueryInformationJobObject(job, JobObjectBasicProcessIdList, buffer, (uint)size, out returned))
                {
                    uint count = unchecked((uint)Marshal.ReadInt32(buffer, 4));
                    long[] result = new long[count];
                    for (int index = 0; index < count; index++)
                    {
                        IntPtr value = Marshal.ReadIntPtr(buffer, 8 + index * IntPtr.Size);
                        result[index] = value.ToInt64();
                    }
                    return result;
                }
                int error = Marshal.GetLastWin32Error();
                if (error != ERROR_MORE_DATA)
                    throw new Win32Exception(error, "job process-list query failed");
                uint assigned = unchecked((uint)Marshal.ReadInt32(buffer, 0));
                capacity = Math.Max(capacity * 2, checked((int)assigned + 16));
            }
            finally
            {
                Marshal.FreeHGlobal(buffer);
            }
        }
    }

    public static void Terminate(IntPtr job, uint exitCode)
    {
        if (!TerminateJobObject(job, exitCode))
            throw new Win32Exception(Marshal.GetLastWin32Error(), "TerminateJobObject failed");
    }

    public static void Close(IntPtr job)
    {
        if (job != IntPtr.Zero && !CloseHandle(job))
            throw new Win32Exception(Marshal.GetLastWin32Error(), "CloseHandle(job) failed");
    }

    public static void AtomicReplaceFile(string sourcePath, string destinationPath)
    {
        if (!MoveFileEx(
            sourcePath,
            destinationPath,
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH))
            throw new Win32Exception(Marshal.GetLastWin32Error(), "MoveFileEx atomic replace failed");
    }
}
'@

function Stop-Benchmark {
    param([string]$Code, [string]$Message)
    throw "REPLAY-BENCHMARK-$Code`: $Message"
}

function Write-Utf8Text {
    param([string]$Path, [string]$Text)
    [System.IO.File]::WriteAllText($Path, $Text, [System.Text.UTF8Encoding]::new($false))
}

function Write-JsonFile {
    param([string]$Path, [object]$Value)
    Write-Utf8Text -Path $Path -Text (($Value | ConvertTo-Json -Depth 100) + "`n")
}

function Write-JsonFileAtomic {
    param([string]$Path, [object]$Value)
    $partial = "$Path.runner-$([Guid]::NewGuid().ToString('N')).partial"
    Write-JsonFile -Path $partial -Value $Value
    try {
        [QueueBackReplayJob]::AtomicReplaceFile($partial, $Path)
    }
    catch {
        Stop-Benchmark "ATOMIC_WRITE" "Could not atomically replace '$Path'; original and partial evidence were preserved: $($_.Exception.Message)"
    }
}

function Get-RequiredProperty {
    param([object]$Object, [string]$Name, [string]$Context)
    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) {
        Stop-Benchmark "MANIFEST" "$Context is missing required property '$Name'."
    }
    return $property.Value
}

function Assert-OnlyProperties {
    param([object]$Object, [string[]]$Allowed, [string]$Context)
    $unexpected = @($Object.PSObject.Properties.Name | Where-Object { $_ -notin $Allowed })
    if ($unexpected.Count -gt 0) {
        Stop-Benchmark "MANIFEST" "$Context contains unsupported properties: $($unexpected -join ', ')."
    }
}

function Get-FullAbsolutePath {
    param([string]$Value, [string]$Label)
    if ([string]::IsNullOrWhiteSpace($Value) -or $Value -notmatch '^[A-Za-z]:[\\/]') {
        Stop-Benchmark "PATH" "$Label must be an absolute drive-qualified Windows path."
    }
    try { return [System.IO.Path]::GetFullPath($Value) }
    catch { Stop-Benchmark "PATH" "$Label is not valid: $($_.Exception.Message)" }
}

function Test-PathEqual {
    param([string]$Left, [string]$Right)
    return [string]::Equals(
        $Left.TrimEnd('\', '/'),
        $Right.TrimEnd('\', '/'),
        [System.StringComparison]::OrdinalIgnoreCase
    )
}

function Assert-NotBroadRoot {
    param([string]$Path, [string]$Label)
    $full = Get-FullAbsolutePath -Value $Path -Label $Label
    $volume = [System.IO.Path]::GetPathRoot($full)
    if (Test-PathEqual $full $volume) { Stop-Benchmark "BROAD_ROOT" "$Label cannot be a volume root." }
    $parent = [System.IO.Directory]::GetParent($full)
    if ($null -eq $parent -or (Test-PathEqual $parent.FullName $volume)) {
        Stop-Benchmark "BROAD_ROOT" "$Label cannot be a direct child of a volume root."
    }
    foreach ($folder in @(
        [Environment]::GetFolderPath([Environment+SpecialFolder]::UserProfile),
        [Environment]::GetFolderPath([Environment+SpecialFolder]::Windows),
        [Environment]::GetFolderPath([Environment+SpecialFolder]::ProgramFiles),
        [Environment]::GetFolderPath([Environment+SpecialFolder]::CommonApplicationData)
    )) {
        if (-not [string]::IsNullOrWhiteSpace($folder) -and (Test-PathEqual $full ([System.IO.Path]::GetFullPath($folder)))) {
            Stop-Benchmark "BROAD_ROOT" "$Label cannot be a system or user-profile root."
        }
    }
    return $full
}

function Assert-ReparseFree {
    param([string]$Path, [string]$Label)
    $full = Get-FullAbsolutePath -Value $Path -Label $Label
    $cursor = $full
    while (-not (Test-Path -LiteralPath $cursor)) {
        $parent = [System.IO.Directory]::GetParent($cursor)
        if ($null -eq $parent) { break }
        $cursor = $parent.FullName
    }
    while (-not [string]::IsNullOrWhiteSpace($cursor)) {
        $item = Get-Item -LiteralPath $cursor -Force
        if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
            Stop-Benchmark "REPARSE_PATH" "$Label traverses reparse point '$($item.FullName)'."
        }
        $parent = [System.IO.Directory]::GetParent($item.FullName)
        if ($null -eq $parent) { break }
        $cursor = $parent.FullName
    }
    return $full
}

function Assert-StrictDescendant {
    param([string]$Root, [string]$Path, [string]$Label)
    $prefix = $Root.TrimEnd('\', '/') + [System.IO.Path]::DirectorySeparatorChar
    if (-not $Path.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        Stop-Benchmark "PATH_ESCAPE" "$Label must be strictly below sentinel_root."
    }
}

function Assert-DisjointRoots {
    param([object[]]$Entries)
    for ($leftIndex = 0; $leftIndex -lt $Entries.Count; $leftIndex++) {
        for ($rightIndex = $leftIndex + 1; $rightIndex -lt $Entries.Count; $rightIndex++) {
            $left = [string]$Entries[$leftIndex].path
            $right = [string]$Entries[$rightIndex].path
            $leftPrefix = $left.TrimEnd('\', '/') + [System.IO.Path]::DirectorySeparatorChar
            $rightPrefix = $right.TrimEnd('\', '/') + [System.IO.Path]::DirectorySeparatorChar
            if (
                (Test-PathEqual $left $right) -or
                $left.StartsWith($rightPrefix, [System.StringComparison]::OrdinalIgnoreCase) -or
                $right.StartsWith($leftPrefix, [System.StringComparison]::OrdinalIgnoreCase)
            ) {
                Stop-Benchmark "ROOT_OVERLAP" "$($Entries[$leftIndex].label) and $($Entries[$rightIndex].label) must be disjoint."
            }
        }
    }
}

function Get-SafeRelativePath {
    param([string]$Value, [string]$Label)
    if (
        [string]::IsNullOrWhiteSpace($Value) -or
        [System.IO.Path]::IsPathRooted($Value) -or
        $Value.Contains(':')
    ) {
        Stop-Benchmark "RELATIVE_PATH" "$Label must be a safe relative path."
    }
    $parts = @($Value -split '[\\/]')
    if (@($parts | Where-Object { $_ -in @("", ".", "..") }).Count -gt 0) {
        Stop-Benchmark "RELATIVE_PATH" "$Label contains an empty, current, or parent component."
    }
    return ($parts -join [System.IO.Path]::DirectorySeparatorChar)
}

function Get-Sha256 {
    param([string]$Path)
    $stream = [System.IO.File]::OpenRead($Path)
    $hasher = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = $hasher.ComputeHash($stream)
        return ([System.BitConverter]::ToString($bytes) -replace '-', '').ToLowerInvariant()
    }
    finally {
        $hasher.Dispose()
        $stream.Dispose()
    }
}

function Get-DirectoryFingerprint {
    param([string]$Root, [string]$Label)
    $fullRoot = Get-FullAbsolutePath -Value $Root -Label $Label
    if (-not (Test-Path -LiteralPath $fullRoot -PathType Container)) {
        Stop-Benchmark "DDRAGON" "$Label is missing: $fullRoot"
    }
    $fullRoot = Assert-ReparseFree -Path $fullRoot -Label $Label
    $rootPrefix = $fullRoot.TrimEnd('\', '/') + [System.IO.Path]::DirectorySeparatorChar
    $pending = [System.Collections.Generic.Queue[string]]::new()
    $pending.Enqueue($fullRoot)
    $files = New-Object 'System.Collections.Generic.List[object]'
    while ($pending.Count -gt 0) {
        $directory = $pending.Dequeue()
        foreach ($entry in @(Get-ChildItem -LiteralPath $directory -Force -ErrorAction Stop)) {
            if (($entry.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
                Stop-Benchmark "REPARSE_PATH" "$Label contains reparse entry '$($entry.FullName)'."
            }
            $full = [System.IO.Path]::GetFullPath($entry.FullName)
            Assert-StrictDescendant -Root $fullRoot -Path $full -Label $Label
            if ($entry.PSIsContainer) {
                $pending.Enqueue($full)
            }
            else {
                $files.Add([pscustomobject]@{
                    full_path = $full
                    relative_path = $full.Substring($rootPrefix.Length).Replace('\', '/')
                    size_bytes = [uint64]$entry.Length
                })
            }
        }
    }
    $lines = New-Object 'System.Collections.Generic.List[string]'
    foreach ($entry in @($files.ToArray() | Sort-Object relative_path)) {
        $lines.Add("$([string]$entry.relative_path)|$([uint64]$entry.size_bytes)|$(Get-Sha256 ([string]$entry.full_path))")
    }
    $bytes = [System.Text.UTF8Encoding]::new($false).GetBytes(($lines -join "`n"))
    $hasher = [System.Security.Cryptography.SHA256]::Create()
    try {
        return "sha256:" + (([System.BitConverter]::ToString($hasher.ComputeHash($bytes)) -replace '-', '').ToLowerInvariant())
    }
    finally { $hasher.Dispose() }
}

function Get-BenchmarkConfigText {
    param([string]$LibraryRoot)
    $tomlPath = $LibraryRoot.Replace('\', '\\').Replace('"', '\"')
    return (@(
        '[recording]',
        'profile = "auto"',
        'codec = "auto"',
        '',
        '[storage]',
        "output_path = `"$tomlPath`"",
        'auto_delete_days = 0',
        '',
        '[app]',
        'autostart = true',
        'hevc_playback_supported = false',
        ''
    ) -join "`n")
}

function ConvertTo-NativeArgument {
    param([string]$Value)
    if ($Value.Length -gt 0 -and $Value -notmatch '[\s"]') { return $Value }
    $builder = [System.Text.StringBuilder]::new()
    [void]$builder.Append('"')
    $slashes = 0
    foreach ($character in $Value.ToCharArray()) {
        if ($character -eq '\') { $slashes++; continue }
        if ($character -eq '"') {
            [void]$builder.Append(('\' * (($slashes * 2) + 1)))
            [void]$builder.Append('"')
        }
        else {
            if ($slashes -gt 0) { [void]$builder.Append(('\' * $slashes)) }
            [void]$builder.Append($character)
        }
        $slashes = 0
    }
    if ($slashes -gt 0) { [void]$builder.Append(('\' * ($slashes * 2))) }
    [void]$builder.Append('"')
    return $builder.ToString()
}

function Invoke-NativeTool {
    param(
        [string]$FilePath,
        [string[]]$Arguments,
        [int]$FiniteTimeoutSeconds,
        [string]$Label
    )
    $start = [System.Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $FilePath
    $start.Arguments = (($Arguments | ForEach-Object { ConvertTo-NativeArgument ([string]$_) }) -join ' ')
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $start
    if (-not $process.Start()) { Stop-Benchmark "TOOL_START" "$Label did not start." }
    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    if (-not $process.WaitForExit($FiniteTimeoutSeconds * 1000)) {
        try { $process.Kill() } catch {}
        try { $process.WaitForExit() } catch {}
        Stop-Benchmark "TOOL_TIMEOUT" "$Label exceeded its finite timeout."
    }
    return [pscustomobject][ordered]@{
        exit_code = [int]$process.ExitCode
        stdout = [string]$stdoutTask.GetAwaiter().GetResult()
        stderr = [string]$stderrTask.GetAwaiter().GetResult()
    }
}

function Start-BenchmarkProcess {
    param(
        [string]$FilePath,
        [string]$ArgumentLine,
        [string]$StdoutPath,
        [string]$StderrPath
    )
    try {
        $script:BenchmarkJob = [QueueBackReplayJob]::CreateKillOnClose()
        $script:BenchmarkCompletionPort = [QueueBackReplayJob]::CreateCompletionPort()
        [QueueBackReplayJob]::AssociateCompletionPort(
            $script:BenchmarkJob,
            $script:BenchmarkCompletionPort
        )
        $process = [QueueBackReplayJob]::StartSuspended(
            $script:BenchmarkJob,
            $FilePath,
            $ArgumentLine,
            $StdoutPath,
            $StderrPath
        )
        $script:AppOutput = [pscustomobject]@{
            stdout_path = $StdoutPath
            stderr_path = $StderrPath
        }
        return $process
    }
    catch {
        if ($script:BenchmarkCompletionPort -ne [IntPtr]::Zero) {
            try { [QueueBackReplayJob]::Close($script:BenchmarkCompletionPort) } catch {}
            $script:BenchmarkCompletionPort = [IntPtr]::Zero
        }
        if ($script:BenchmarkJob -ne [IntPtr]::Zero) {
            try { [QueueBackReplayJob]::Close($script:BenchmarkJob) } catch {}
            $script:BenchmarkJob = [IntPtr]::Zero
        }
        Stop-Benchmark "PROCESS_JOB" "Could not create the production app suspended and assign it to the benchmark Job Object before execution: $($_.Exception.Message)"
    }
}

function Get-BenchmarkJobSnapshot {
    if ($script:BenchmarkJob -eq [IntPtr]::Zero) {
        Stop-Benchmark "PROCESS_JOB" "The benchmark Job Object is unavailable."
    }
    try { return [QueueBackReplayJob]::Snapshot($script:BenchmarkJob) }
    catch { Stop-Benchmark "PROCESS_JOB" "Could not query benchmark Job Object accounting: $($_.Exception.Message)" }
}

function Receive-JobNotifications {
    param([ValidateRange(0, 1000)][int]$WaitMilliseconds = 0)
    if ($script:BenchmarkCompletionPort -eq [IntPtr]::Zero) {
        Stop-Benchmark "PROCESS_JOB" "The benchmark Job Object completion port is unavailable."
    }
    try {
        $received = @([QueueBackReplayJob]::DrainNotifications(
            $script:BenchmarkCompletionPort,
            [uint32]$WaitMilliseconds
        ))
    }
    catch {
        Stop-Benchmark "PROCESS_JOB" "Could not drain benchmark Job Object notifications: $($_.Exception.Message)"
    }
    $records = New-Object 'System.Collections.Generic.List[object]'
    foreach ($notification in $received) {
        $record = [ordered]@{
            message_id = [uint32]$notification.MessageId
            message = [string]$notification.Message
            process_id = [int64]$notification.ProcessId
            observed_utc = ([DateTime]$notification.ObservedUtc).ToString("o")
        }
        $script:JobNotifications.Add($record)
        $records.Add($record)
        if ([uint32]$notification.MessageId -eq 6) {
            $script:JobNewProcessNotificationCount = [uint64]$script:JobNewProcessNotificationCount + 1
        }
    }
    return $records.ToArray()
}

function Sync-JobNotifications {
    param(
        [uint64]$ExpectedTotalProcesses,
        [ValidateRange(0, 1000)][int]$BudgetMilliseconds = 100
    )
    $records = New-Object 'System.Collections.Generic.List[object]'
    foreach ($record in @(Receive-JobNotifications -WaitMilliseconds 0)) { $records.Add($record) }
    $timer = [System.Diagnostics.Stopwatch]::StartNew()
    while (
        [uint64]$script:JobNewProcessNotificationCount -lt $ExpectedTotalProcesses -and
        $timer.Elapsed.TotalMilliseconds -lt $BudgetMilliseconds
    ) {
        foreach ($record in @(Receive-JobNotifications -WaitMilliseconds 10)) { $records.Add($record) }
    }
    return $records.ToArray()
}

function Close-BenchmarkJob {
    if ($script:BenchmarkJob -ne [IntPtr]::Zero) {
        try { [QueueBackReplayJob]::Close($script:BenchmarkJob) }
        catch { [Console]::Error.WriteLine("Could not close benchmark Job Object: $($_.Exception.Message)") }
        $script:BenchmarkJob = [IntPtr]::Zero
    }
    if ($script:BenchmarkCompletionPort -ne [IntPtr]::Zero) {
        try { [QueueBackReplayJob]::Close($script:BenchmarkCompletionPort) }
        catch { [Console]::Error.WriteLine("Could not close benchmark completion port: $($_.Exception.Message)") }
        $script:BenchmarkCompletionPort = [IntPtr]::Zero
    }
}

function Complete-BenchmarkOutput {
    if ($null -eq $script:AppOutput) { return }
    $script:AppOutput = $null
}

function Resolve-ExecutableCommand {
    param([string]$Value, [string]$Label)
    if ([string]::IsNullOrWhiteSpace($Value)) { Stop-Benchmark "EXECUTABLE" "$Label is required." }
    if ($Value -match '^[A-Za-z]:[\\/]') {
        $full = Assert-ReparseFree -Path $Value -Label $Label
        if (-not (Test-Path -LiteralPath $full -PathType Leaf)) {
            Stop-Benchmark "EXECUTABLE" "$Label does not exist: $full"
        }
        return $full
    }
    $command = Get-Command $Value -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($null -eq $command) { Stop-Benchmark "EXECUTABLE" "$Label command '$Value' was not found." }
    return [string]$command.Source
}

function Assert-PeExecutable {
    param([string]$Path)
    if ([System.IO.Path]::GetExtension($Path) -ne ".exe") {
        Stop-Benchmark "APP_BINARY" "The production app binary must be an .exe file."
    }
    if ($Path -match '(?i)[\\/]debug[\\/]') {
        Stop-Benchmark "APP_BINARY" "A debug binary cannot be used for the production-WebView baseline."
    }
    $stream = [System.IO.File]::OpenRead($Path)
    try {
        if ($stream.ReadByte() -ne 0x4d -or $stream.ReadByte() -ne 0x5a) {
            Stop-Benchmark "APP_BINARY" "The app binary is not a Windows PE executable."
        }
    }
    finally { $stream.Dispose() }
}

function Assert-PackagedMediaRuntime {
    param([string]$AppPath, [object]$ManifestValue)
    if (-not $ManifestValue.PSObject.Properties["media_tools"]) {
        return [ordered]@{
            required = $false
            root = $null
            runtime_id = $null
            ffmpeg_sha256 = $null
            ffprobe_sha256 = $null
        }
    }

    $runtimeRoot = Assert-ReparseFree `
        -Path (Join-Path (Split-Path -Parent $AppPath) "resources\media-runtime") `
        -Label "packaged media runtime"
    if (-not (Test-Path -LiteralPath $runtimeRoot -PathType Container)) {
        Stop-Benchmark "PACKAGED_RUNTIME" "The production app has no packaged media runtime at $runtimeRoot."
    }
    $runtimeManifestPath = Assert-ReparseFree `
        -Path (Join-Path $runtimeRoot "runtime-manifest.json") `
        -Label "packaged media runtime manifest"
    if (-not (Test-Path -LiteralPath $runtimeManifestPath -PathType Leaf)) {
        Stop-Benchmark "PACKAGED_RUNTIME" "The packaged media runtime has no runtime-manifest.json."
    }
    try {
        $runtimeManifest = Get-Content -LiteralPath $runtimeManifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
    }
    catch {
        Stop-Benchmark "PACKAGED_RUNTIME" "The packaged media runtime manifest is malformed: $($_.Exception.Message)"
    }
    $expectedRuntimeId = [string]$ManifestValue.media_tools.runtime_id
    if (
        -not $runtimeManifest.PSObject.Properties["runtime_id"] -or
        [string]::IsNullOrWhiteSpace([string]$runtimeManifest.runtime_id) -or
        [string]$runtimeManifest.runtime_id -ne $expectedRuntimeId
    ) {
        Stop-Benchmark "PACKAGED_RUNTIME" "The packaged media runtime id does not match the prepared runtime '$expectedRuntimeId'."
    }

    $identities = [ordered]@{}
    foreach ($toolName in @("ffmpeg", "ffprobe")) {
        $expected = $ManifestValue.media_tools.$toolName
        $packagedPath = Assert-ReparseFree `
            -Path (Join-Path $runtimeRoot "bin\$toolName.exe") `
            -Label "packaged $toolName"
        if (-not (Test-Path -LiteralPath $packagedPath -PathType Leaf)) {
            Stop-Benchmark "PACKAGED_RUNTIME" "The packaged media runtime is missing bin\$toolName.exe."
        }
        $item = Get-Item -LiteralPath $packagedPath
        $hash = Get-Sha256 $packagedPath
        if ([uint64]$item.Length -ne [uint64]$expected.size_bytes -or $hash -ne ([string]$expected.sha256).ToLowerInvariant()) {
            Stop-Benchmark "PACKAGED_RUNTIME" "The packaged $toolName identity does not match the prepared media tool."
        }
        $identities[$toolName] = [ordered]@{
            path = $packagedPath
            size_bytes = [uint64]$item.Length
            sha256 = $hash
        }
    }
    return [ordered]@{
        required = $true
        root = $runtimeRoot
        runtime_id = $expectedRuntimeId
        ffmpeg_sha256 = [string]$identities.ffmpeg.sha256
        ffprobe_sha256 = [string]$identities.ffprobe.sha256
    }
}

function Assert-Scenario {
    param([object]$Scenario, [string[]]$FixtureIds)
    Assert-OnlyProperties -Object $Scenario -Allowed $script:AllowedScenarioFields -Context "scenario"
    $id = [string](Get-RequiredProperty $Scenario "id" "scenario")
    if ($id -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$') { Stop-Benchmark "SCENARIO" "Unsafe scenario id '$id'." }
    $kind = [string](Get-RequiredProperty $Scenario "kind" "scenario '$id'")
    if ($kind -notin $script:ScenarioKinds) { Stop-Benchmark "SCENARIO" "Unsupported scenario kind '$kind'." }
    $trialId = [string](Get-RequiredProperty $Scenario "trial_id" "scenario '$id'")
    if ($trialId -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$') { Stop-Benchmark "SCENARIO" "Scenario '$id' has unsafe trial_id." }
    $references = @((Get-RequiredProperty $Scenario "fixture_ids" "scenario '$id'"))
    if ($references.Count -eq 0) { Stop-Benchmark "SCENARIO" "Scenario '$id' has no fixtures." }
    if ($kind -ne "lifecycle" -and $references.Count -ne 1) {
        Stop-Benchmark "SCENARIO" "Only lifecycle scenarios may reference multiple fixtures."
    }
    foreach ($fixtureId in $references) {
        if ([string]$fixtureId -notin $FixtureIds) {
            Stop-Benchmark "SCENARIO" "Scenario '$id' references unknown fixture '$fixtureId'."
        }
    }
    if ($Scenario.PSObject.Properties["seek_reason"] -and [string]$Scenario.seek_reason -notin @("benchmark", "event-jump", "endpoint-edit")) {
        Stop-Benchmark "SCENARIO" "Scenario '$id' has an unsupported seek_reason."
    }
    if ($Scenario.PSObject.Properties["seek_playback_mode"] -and [string]$Scenario.seek_playback_mode -notin @("playing", "paused")) {
        Stop-Benchmark "SCENARIO" "Scenario '$id' has an unsupported seek_playback_mode."
    }
    if ($kind -eq "export") {
        foreach ($field in @(
            "clip_start_ms", "clip_end_ms", "expected_duration_ms", "expected_video_codec",
            "expected_audio_codec", "export_presets"
        )) {
            [void](Get-RequiredProperty $Scenario $field "export scenario '$id'")
        }
        [int64]$clipStartMs = $Scenario.clip_start_ms
        [int64]$clipEndMs = $Scenario.clip_end_ms
        [int64]$expectedDurationMs = $Scenario.expected_duration_ms
        if ($clipStartMs -lt 0 -or $clipEndMs -le $clipStartMs -or ($clipEndMs - $clipStartMs) -lt 5000) {
            Stop-Benchmark "SCENARIO" "Export scenario '$id' must declare a clip of at least five seconds."
        }
        if ($expectedDurationMs -ne ($clipEndMs - $clipStartMs)) {
            Stop-Benchmark "SCENARIO" "Export scenario '$id' expected_duration_ms must equal clip_end_ms minus clip_start_ms."
        }
        $presets = @($Scenario.export_presets)
        if ($presets.Count -lt 1 -or $presets.Count -gt 3 -or @($presets | Select-Object -Unique).Count -ne $presets.Count) {
            Stop-Benchmark "SCENARIO" "Export scenario '$id' must declare one to three unique export_presets."
        }
        if (@($presets | Where-Object { [string]$_ -notin @("horizontal", "vertical", "discord") }).Count -gt 0) {
            Stop-Benchmark "SCENARIO" "Export scenario '$id' has an unsupported export preset."
        }
        if ([string]$Scenario.expected_video_codec -ne "h264" -or [string]$Scenario.expected_audio_codec -ne "aac") {
            Stop-Benchmark "SCENARIO" "Export scenario '$id' must validate the current H.264/AAC export contract."
        }
        if ($Scenario.PSObject.Properties["duration_tolerance_ms"] -and ([int64]$Scenario.duration_tolerance_ms -lt 0 -or [int64]$Scenario.duration_tolerance_ms -gt 5000)) {
            Stop-Benchmark "SCENARIO" "Export scenario '$id' duration_tolerance_ms must be between 0 and 5000."
        }
        if ($Scenario.PSObject.Properties["music_mode"] -and [string]$Scenario.music_mode -notin @("none", "built_in")) {
            Stop-Benchmark "SCENARIO" "Export scenario '$id' has an unsupported music_mode."
        }
        if ($Scenario.PSObject.Properties["gain_mode"] -and [string]$Scenario.gain_mode -notin @("unity", "non_unity")) {
            Stop-Benchmark "SCENARIO" "Export scenario '$id' has an unsupported gain_mode."
        }
        if ($Scenario.PSObject.Properties["built_in_music_filename"] -and [string]$Scenario.built_in_music_filename -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$') {
            Stop-Benchmark "SCENARIO" "Export scenario '$id' built_in_music_filename must be a safe filename."
        }
    }
}

function Resolve-ConfiguredPath {
    param([string]$CommandLineValue, [object]$ManifestValue, [string]$Label)
    $configured = if (-not [string]::IsNullOrWhiteSpace($CommandLineValue)) {
        $CommandLineValue
    }
    elseif ($null -ne $ManifestValue -and -not [string]::IsNullOrWhiteSpace([string]$ManifestValue)) {
        [string]$ManifestValue
    }
    else { $null }
    if (
        -not [string]::IsNullOrWhiteSpace($CommandLineValue) -and
        $null -ne $ManifestValue -and
        -not [string]::IsNullOrWhiteSpace([string]$ManifestValue)
    ) {
        $left = Get-FullAbsolutePath -Value $CommandLineValue -Label $Label
        $right = Get-FullAbsolutePath -Value ([string]$ManifestValue) -Label "manifest $Label"
        if (-not (Test-PathEqual $left $right)) {
            Stop-Benchmark "IDENTITY" "$Label command-line value disagrees with the manifest."
        }
    }
    return $configured
}

function Read-AndValidateManifest {
    param([string]$Path)
    $manifestPath = Get-FullAbsolutePath -Value $Path -Label "Manifest"
    if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
        Stop-Benchmark "MANIFEST_MISSING" "Manifest does not exist: $manifestPath"
    }
    $manifestPath = Assert-ReparseFree -Path $manifestPath -Label "Manifest"
    try { $value = Get-Content -LiteralPath $manifestPath -Raw -Encoding UTF8 | ConvertFrom-Json }
    catch { Stop-Benchmark "MANIFEST_JSON" "Manifest is not valid JSON: $($_.Exception.Message)" }
    Assert-OnlyProperties -Object $value -Allowed $script:AllowedTopLevel -Context "manifest"
    foreach ($field in $script:RequiredTopLevel) { [void](Get-RequiredProperty $value $field "manifest") }
    if ([int]$value.schema_version -ne 1) { Stop-Benchmark "SCHEMA_VERSION" "schema_version must be 1." }
    if ([string]$value.run_id -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$') { Stop-Benchmark "RUN_ID" "run_id is unsafe." }
    if ([string]$value.observer_profile -notin @("minimal", "full")) { Stop-Benchmark "OBSERVER" "observer_profile must be minimal or full." }
    Assert-OnlyProperties -Object $value.ddragon -Allowed @("mode", "cache_root", "cache_fingerprint") -Context "ddragon"
    if ([string]$value.ddragon.mode -ne "offline") { Stop-Benchmark "DDRAGON" "ddragon.mode must be exactly 'offline'." }

    $sentinelRoot = Assert-NotBroadRoot -Path ([string]$value.sentinel_root) -Label "sentinel_root"
    if (-not (Test-Path -LiteralPath $sentinelRoot -PathType Container)) { Stop-Benchmark "SENTINEL_ROOT" "sentinel_root is missing." }
    $sentinelRoot = Assert-ReparseFree -Path $sentinelRoot -Label "sentinel_root"
    if ([System.IO.Path]::GetFileName($sentinelRoot) -ne $script:SentinelName) {
        Stop-Benchmark "SENTINEL_NAME" "sentinel_root itself must end with '$script:SentinelName'."
    }
    Assert-StrictDescendant -Root $sentinelRoot -Path $manifestPath -Label "Manifest"

    $libraryRoot = Get-FullAbsolutePath -Value ([string]$value.library_root) -Label "library_root"
    $configPath = Get-FullAbsolutePath -Value ([string]$value.config_path) -Label "config_path"
    $appDataRoot = Get-FullAbsolutePath -Value ([string]$value.app_data_root) -Label "app_data_root"
    $resultRoot = Get-FullAbsolutePath -Value ([string]$value.result_root) -Label "result_root"
    $scratchRoot = Get-FullAbsolutePath -Value ([string]$value.scratch_root) -Label "scratch_root"
    Assert-DisjointRoots -Entries @(
        [pscustomobject]@{ label = "library_root"; path = $libraryRoot },
        [pscustomobject]@{ label = "config parent"; path = (Split-Path -Parent $configPath) },
        [pscustomobject]@{ label = "app_data_root"; path = $appDataRoot },
        [pscustomobject]@{ label = "result_root"; path = $resultRoot },
        [pscustomobject]@{ label = "scratch_root"; path = $scratchRoot }
    )
    foreach ($pair in @(
        @($libraryRoot, "library_root"), @($configPath, "config_path"),
        @($appDataRoot, "app_data_root"), @($resultRoot, "result_root"),
        @($scratchRoot, "scratch_root")
    )) {
        Assert-StrictDescendant -Root $sentinelRoot -Path $pair[0] -Label $pair[1]
        [void](Assert-ReparseFree -Path $pair[0] -Label $pair[1])
    }
    foreach ($directory in @($libraryRoot, $appDataRoot, $scratchRoot, (Split-Path -Parent $configPath), (Split-Path -Parent $resultRoot))) {
        if (-not (Test-Path -LiteralPath $directory -PathType Container)) {
            Stop-Benchmark "PATH_MISSING" "Required prepared directory is missing: $directory"
        }
    }
    if ([System.IO.Path]::GetFileName($configPath) -ne "config.toml") {
        Stop-Benchmark "CONFIG_PATH" "config_path must end with config.toml."
    }
    if (-not (Test-Path -LiteralPath $configPath -PathType Leaf)) {
        Stop-Benchmark "CONFIG_PATH" "The canonical benchmark config is missing."
    }
    $expectedConfig = Get-BenchmarkConfigText -LibraryRoot $libraryRoot
    $actualConfig = [System.IO.File]::ReadAllText($configPath, [System.Text.Encoding]::UTF8)
    if ($actualConfig -ne $expectedConfig) {
        Stop-Benchmark "CONFIG_PATH" "config_path is not the canonical no-retention benchmark configuration."
    }
    if (Test-Path -LiteralPath $resultRoot) { Stop-Benchmark "RESULT_EXISTS" "result_root already exists; runs are immutable." }
    if ([System.IO.Path]::GetFileName($resultRoot) -ne [string]$value.run_id) {
        Stop-Benchmark "RESULT_ROOT" "result_root leaf must equal run_id."
    }
    foreach ($field in @("cache_root", "cache_fingerprint")) {
        [void](Get-RequiredProperty $value.ddragon $field "ddragon")
    }
    $cacheRoot = Get-FullAbsolutePath -Value ([string]$value.ddragon.cache_root) -Label "ddragon.cache_root"
    Assert-StrictDescendant -Root $sentinelRoot -Path $cacheRoot -Label "ddragon.cache_root"
    $cacheRoot = Assert-ReparseFree -Path $cacheRoot -Label "ddragon.cache_root"
    if (-not (Test-PathEqual $cacheRoot (Join-Path $appDataRoot "ddragon"))) {
        Stop-Benchmark "DDRAGON" "ddragon.cache_root must equal app_data_root\ddragon."
    }
    $expectedCacheFingerprint = [string]$value.ddragon.cache_fingerprint
    if ($expectedCacheFingerprint -notmatch '^sha256:[a-f0-9]{64}$') {
        Stop-Benchmark "DDRAGON" "ddragon.cache_fingerprint must be a lowercase sha256 identity."
    }
    $actualCacheFingerprint = Get-DirectoryFingerprint -Root $cacheRoot -Label "ddragon.cache_root"
    if ($actualCacheFingerprint -ne $expectedCacheFingerprint) {
        Stop-Benchmark "DDRAGON" "Data Dragon cache identity changed since preparation."
    }

    $fixtureIds = New-Object System.Collections.Generic.List[string]
    $fixtureAliases = New-Object System.Collections.Generic.List[string]
    $fixtureTimestamps = New-Object System.Collections.Generic.List[string]
    $verifiedFiles = New-Object 'System.Collections.Generic.List[object]'
    foreach ($fixture in @($value.fixtures)) {
        Assert-OnlyProperties -Object $fixture -Allowed @(
            "id", "alias", "game_timestamp", "kind", "relative_path", "backend", "codec", "negative", "files", "media_validation"
        ) -Context "fixture"
        foreach ($field in @("id", "alias", "game_timestamp", "kind", "relative_path", "files", "media_validation")) {
            [void](Get-RequiredProperty $fixture $field "fixture")
        }
        $fixtureId = [string]$fixture.id
        if ($fixtureId -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$' -or $fixtureIds.Contains($fixtureId)) {
            Stop-Benchmark "FIXTURE" "Fixture id '$fixtureId' is unsafe or duplicated."
        }
        if ([string]$fixture.kind -notin @("recording_bundle", "media", "negative_fixture")) {
            Stop-Benchmark "FIXTURE" "Fixture '$fixtureId' has unsupported kind."
        }
        if ([string]$fixture.alias -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$') {
            Stop-Benchmark "FIXTURE" "Fixture '$fixtureId' has unsafe alias."
        }
        if ($fixtureAliases.Contains([string]$fixture.alias)) {
            Stop-Benchmark "FIXTURE" "Fixture alias '$($fixture.alias)' is duplicated."
        }
        if ([string]$fixture.game_timestamp -notmatch '^\d+(?:-[1-9]\d{0,2})?$') {
            Stop-Benchmark "FIXTURE" "Fixture '$fixtureId' has invalid game_timestamp."
        }
        if ($fixtureTimestamps.Contains([string]$fixture.game_timestamp)) {
            Stop-Benchmark "FIXTURE" "Fixture game_timestamp '$($fixture.game_timestamp)' is duplicated."
        }
        [void](Get-SafeRelativePath -Value ([string]$fixture.relative_path) -Label "fixture '$fixtureId' relative_path")
        $files = @($fixture.files)
        if ($files.Count -eq 0) { Stop-Benchmark "FIXTURE" "Fixture '$fixtureId' has no file identities." }
        foreach ($file in $files) {
            Assert-OnlyProperties -Object $file -Allowed @("relative_path", "size_bytes", "sha256", "last_write_utc") -Context "fixture file"
            foreach ($field in @("relative_path", "size_bytes", "sha256", "last_write_utc")) {
                [void](Get-RequiredProperty $file $field "fixture '$fixtureId' file")
            }
            $relative = Get-SafeRelativePath -Value ([string]$file.relative_path) -Label "fixture '$fixtureId' file"
            $full = Get-FullAbsolutePath -Value (Join-Path $libraryRoot $relative) -Label "fixture file"
            Assert-StrictDescendant -Root $libraryRoot -Path $full -Label "fixture '$fixtureId' file"
            $full = Assert-ReparseFree -Path $full -Label "fixture '$fixtureId' file"
            if (-not (Test-Path -LiteralPath $full -PathType Leaf)) { Stop-Benchmark "FIXTURE_MISSING" "Fixture file is missing: $full" }
            $item = Get-Item -LiteralPath $full
            try {
                $rawLastWriteUtc = $file.last_write_utc
                if ($rawLastWriteUtc -is [DateTime]) {
                    $expectedLastWriteUtc = ([DateTime]$rawLastWriteUtc).ToUniversalTime()
                }
                elseif ($rawLastWriteUtc -is [DateTimeOffset]) {
                    $expectedLastWriteUtc = ([DateTimeOffset]$rawLastWriteUtc).UtcDateTime
                }
                else {
                    $expectedLastWriteUtc = [DateTimeOffset]::Parse(
                        [string]$rawLastWriteUtc,
                        [System.Globalization.CultureInfo]::InvariantCulture,
                        [System.Globalization.DateTimeStyles]::RoundtripKind
                    ).UtcDateTime
                }
            }
            catch { Stop-Benchmark "FIXTURE_IDENTITY" "Fixture '$fixtureId' has an invalid file timestamp: $relative" }
            if (
                [uint64]$item.Length -ne [uint64]$file.size_bytes -or
                $item.LastWriteTimeUtc.Ticks -ne $expectedLastWriteUtc.Ticks
            ) {
                Stop-Benchmark "FIXTURE_IDENTITY" (
                    "Fixture '$fixtureId' file size or write timestamp changed: $relative; " +
                    "expected size=$([uint64]$file.size_bytes), timestamp=$($expectedLastWriteUtc.ToString('o')); " +
                    "observed size=$([uint64]$item.Length), timestamp=$($item.LastWriteTimeUtc.ToString('o'))."
                )
            }
            $verifiedFiles.Add([ordered]@{
                fixture_id = $fixtureId
                relative_path = $relative
                absolute_path = $full
                size_bytes = [uint64]$item.Length
                sha256 = ([string]$file.sha256).ToLowerInvariant()
            })
        }
        $fixtureIds.Add($fixtureId)
        $fixtureAliases.Add([string]$fixture.alias)
        $fixtureTimestamps.Add([string]$fixture.game_timestamp)
    }
    if ($fixtureIds.Count -eq 0) { Stop-Benchmark "FIXTURE" "At least one fixture is required." }
    $scenarioIds = New-Object System.Collections.Generic.List[string]
    foreach ($scenario in @($value.scenarios)) {
        Assert-Scenario -Scenario $scenario -FixtureIds $fixtureIds.ToArray()
        if ($scenarioIds.Contains([string]$scenario.id)) { Stop-Benchmark "SCENARIO" "Duplicate scenario id '$($scenario.id)'." }
        $scenarioIds.Add([string]$scenario.id)
    }
    if ($scenarioIds.Count -ne 1) { Stop-Benchmark "SCENARIO" "A launch manifest must contain exactly one scenario/trial." }

    if ($value.PSObject.Properties["preparation_receipt"]) {
        $receipt = Get-FullAbsolutePath -Value ([string]$value.preparation_receipt) -Label "preparation_receipt"
        Assert-StrictDescendant -Root $sentinelRoot -Path $receipt -Label "preparation_receipt"
        $receipt = Assert-ReparseFree -Path $receipt -Label "preparation_receipt"
        if (-not (Test-Path -LiteralPath $receipt -PathType Leaf)) { Stop-Benchmark "RECEIPT" "preparation_receipt is missing." }
    }
    if ($value.PSObject.Properties["media_tools"]) {
        Assert-OnlyProperties -Object $value.media_tools -Allowed @("runtime_id", "ffmpeg", "ffprobe") -Context "media_tools"
        $runtimeId = [string](Get-RequiredProperty $value.media_tools "runtime_id" "media_tools")
        if ([string]::IsNullOrWhiteSpace($runtimeId)) { Stop-Benchmark "MEDIA_TOOL" "media_tools.runtime_id is required." }
        foreach ($toolName in @("ffmpeg", "ffprobe")) {
            $tool = Get-RequiredProperty $value.media_tools $toolName "media_tools"
            Assert-OnlyProperties -Object $tool -Allowed @("path", "size_bytes", "sha256", "version_line") -Context "media_tools.$toolName"
            $toolPath = Assert-ReparseFree -Path ([string]$tool.path -replace '^$', ' ') -Label "media_tools.$toolName.path"
            if (-not (Test-Path -LiteralPath $toolPath -PathType Leaf)) { Stop-Benchmark "MEDIA_TOOL" "$toolName is missing." }
            $toolItem = Get-Item -LiteralPath $toolPath
            if ([uint64]$toolItem.Length -ne [uint64]$tool.size_bytes -or (Get-Sha256 $toolPath) -ne ([string]$tool.sha256).ToLowerInvariant()) {
                Stop-Benchmark "MEDIA_TOOL" "$toolName identity changed since preparation."
            }
        }
    }
    if ([string](@($value.scenarios)[0].kind) -eq "export" -and -not $value.PSObject.Properties["media_tools"]) {
        Stop-Benchmark "MEDIA_TOOL" "Export scenarios require packaged ffmpeg/ffprobe identities during preflight."
    }

    return [pscustomobject][ordered]@{
        path = $manifestPath
        hash = Get-Sha256 $manifestPath
        value = $value
        sentinel_root = $sentinelRoot
        library_root = $libraryRoot
        config_path = $configPath
        app_data_root = $appDataRoot
        result_root = $resultRoot
        scratch_root = $scratchRoot
        ddragon_cache_root = $cacheRoot
        ddragon_cache_fingerprint = $actualCacheFingerprint
        config_sha256 = Get-Sha256 $configPath
        fixture_files = $verifiedFiles.ToArray()
    }
}

function Get-ProcessCreationIdentity {
    param([object]$Row)
    if ($null -eq $Row.CreationDate) { return "unknown" }
    try { return ([DateTime]$Row.CreationDate).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ss.fffZ") }
    catch { return [string]$Row.CreationDate }
}

function Get-ProcessRows {
    if ($script:BenchmarkJob -eq [IntPtr]::Zero) {
        Stop-Benchmark "PROCESS_JOB" "The benchmark Job Object is unavailable for process snapshots."
    }
    return @([QueueBackReplayJob]::SnapshotProcesses($script:BenchmarkJob))
}

function Update-CreatedProcessTree {
    param([object[]]$Rows, [int]$RootPid, [object]$JobSnapshot = $null)
    $byPid = @{}
    foreach ($row in $Rows) { $byPid[[string][int]$row.ProcessId] = $row }
    if ($null -eq $JobSnapshot -and $script:BenchmarkJob -ne [IntPtr]::Zero) {
        $JobSnapshot = Get-BenchmarkJobSnapshot
    }
    if ($null -ne $JobSnapshot) {
        foreach ($jobPid in @($JobSnapshot.ProcessIds)) {
            $pidKey = [string][int64]$jobPid
            if ($byPid.ContainsKey($pidKey) -and -not $script:KnownProcesses.ContainsKey($pidKey)) {
                $script:KnownProcesses[$pidKey] = Get-ProcessCreationIdentity $byPid[$pidKey]
            }
        }
    }
    if ($script:KnownProcesses.Count -eq 0 -and $byPid.ContainsKey([string]$RootPid)) {
        $rootRow = $byPid[[string]$RootPid]
        $script:KnownProcesses[[string]$RootPid] = Get-ProcessCreationIdentity $rootRow
    }
    $changed = $true
    while ($changed) {
        $changed = $false
        foreach ($row in $Rows) {
            $pidKey = [string][int]$row.ProcessId
            $parentKey = [string][int]$row.ParentProcessId
            if ($script:KnownProcesses.ContainsKey($pidKey) -or -not $script:KnownProcesses.ContainsKey($parentKey)) { continue }
            if (
                $byPid.ContainsKey($parentKey) -and
                (Get-ProcessCreationIdentity $byPid[$parentKey]) -ne $script:KnownProcesses[$parentKey]
            ) {
                continue
            }
            $script:KnownProcesses[$pidKey] = Get-ProcessCreationIdentity $row
            $changed = $true
        }
    }
    return $byPid
}

function Get-AliveCreatedRows {
    param([hashtable]$ByPid)
    $alive = New-Object 'System.Collections.Generic.List[object]'
    foreach ($pidKey in @($script:KnownProcesses.Keys)) {
        if (-not $ByPid.ContainsKey($pidKey)) { continue }
        $row = $ByPid[$pidKey]
        if ((Get-ProcessCreationIdentity $row) -eq $script:KnownProcesses[$pidKey]) { $alive.Add($row) }
    }
    return $alive.ToArray()
}

function Disable-GpuCollection {
    param([string]$Limitation)
    $script:GpuCollectionEnabled = $false
    $script:GpuCollectionLimitation = $Limitation
}

function Initialize-GpuObservation {
    param([string]$Profile)
    if ($Profile -ne "full") {
        Disable-GpuCollection -Limitation "observer_profile=minimal"
        return [ordered]@{
            requested = $false
            enabled = $false
            method = "disabled"
            maximum_live_query_ms = 0.0
            prelaunch_probe_duration_ms = 0.0
            limitation = $script:GpuCollectionLimitation
        }
    }
    Disable-GpuCollection -Limitation (
        "Optional Windows GPU CIM telemetry is disabled because its provider has no runner-enforced " +
        "finite deadline; it cannot safely share the one-second core process-sampling cadence."
    )
    return [ordered]@{
        requested = $true
        enabled = $false
        method = "disabled-unbounded-windows-cim"
        maximum_live_query_ms = 0.0
        prelaunch_probe_duration_ms = 0.0
        limitation = $script:GpuCollectionLimitation
    }
}

function Get-GpuObservation {
    param([int[]]$ProcessIds, [string]$Profile)
    return [ordered]@{
        available = $false
        limitation = $script:GpuCollectionLimitation
        collection_duration_ms = 0.0
        engines = @()
        memory = @()
    }
}

function Get-SystemCpuPercent {
    param([hashtable]$PreviousSystemTimes)
    try {
        $snapshot = [QueueBackReplayJob]::SnapshotSystemTimes()
        [uint64]$idle = [uint64]$snapshot.IdleTime100ns
        [uint64]$kernel = [uint64]$snapshot.KernelTime100ns
        [uint64]$user = [uint64]$snapshot.UserTime100ns
        $percent = $null
        if (
            $PreviousSystemTimes.ContainsKey("idle") -and
            $PreviousSystemTimes.ContainsKey("kernel") -and
            $PreviousSystemTimes.ContainsKey("user")
        ) {
            [double]$idleDelta = [double]$idle - [double]$PreviousSystemTimes["idle"]
            [double]$kernelDelta = [double]$kernel - [double]$PreviousSystemTimes["kernel"]
            [double]$userDelta = [double]$user - [double]$PreviousSystemTimes["user"]
            [double]$totalDelta = $kernelDelta + $userDelta
            if ($idleDelta -ge 0 -and $kernelDelta -ge 0 -and $userDelta -ge 0 -and $totalDelta -gt 0) {
                $percent = [Math]::Max(0.0, [Math]::Min(100.0, (($totalDelta - $idleDelta) / $totalDelta) * 100.0))
            }
        }
        $PreviousSystemTimes["idle"] = $idle
        $PreviousSystemTimes["kernel"] = $kernel
        $PreviousSystemTimes["user"] = $user
        return $percent
    }
    catch { return $null }
}

function Get-EnvironmentIdentity {
    $identity = [ordered]@{
        os = $null
        computer = $null
        processors = @()
        video_controllers = @()
        powershell_version = $PSVersionTable.PSVersion.ToString()
        limitations = @()
    }
    $limitations = New-Object 'System.Collections.Generic.List[string]'
    try {
        $os = Get-CimInstance -ClassName Win32_OperatingSystem -ErrorAction Stop
        $identity.os = [ordered]@{
            caption = [string]$os.Caption
            version = [string]$os.Version
            build_number = [string]$os.BuildNumber
            architecture = [string]$os.OSArchitecture
        }
    }
    catch { $limitations.Add("OS identity unavailable: $($_.Exception.Message)") }
    try {
        $computer = Get-CimInstance -ClassName Win32_ComputerSystem -ErrorAction Stop
        $identity.computer = [ordered]@{
            manufacturer = [string]$computer.Manufacturer
            model = [string]$computer.Model
            total_physical_memory_bytes = [uint64]$computer.TotalPhysicalMemory
        }
    }
    catch { $limitations.Add("Computer identity unavailable: $($_.Exception.Message)") }
    try {
        $identity.processors = @(
            Get-CimInstance -ClassName Win32_Processor -ErrorAction Stop |
                Sort-Object DeviceID |
                ForEach-Object {
                    [ordered]@{
                        name = [string]$_.Name
                        manufacturer = [string]$_.Manufacturer
                        cores = [uint32]$_.NumberOfCores
                        logical_processors = [uint32]$_.NumberOfLogicalProcessors
                        max_clock_mhz = [uint32]$_.MaxClockSpeed
                    }
                }
        )
    }
    catch { $limitations.Add("Processor identity unavailable: $($_.Exception.Message)") }
    try {
        $identity.video_controllers = @(
            Get-CimInstance -ClassName Win32_VideoController -ErrorAction Stop |
                Sort-Object DeviceID |
                ForEach-Object {
                    [ordered]@{
                        name = [string]$_.Name
                        driver_version = [string]$_.DriverVersion
                        adapter_ram_bytes = $(if ($null -eq $_.AdapterRAM) { $null } else { [uint64]$_.AdapterRAM })
                        current_horizontal_resolution = $(if ($null -eq $_.CurrentHorizontalResolution) { $null } else { [uint32]$_.CurrentHorizontalResolution })
                        current_vertical_resolution = $(if ($null -eq $_.CurrentVerticalResolution) { $null } else { [uint32]$_.CurrentVerticalResolution })
                        current_refresh_rate_hz = $(if ($null -eq $_.CurrentRefreshRate) { $null } else { [uint32]$_.CurrentRefreshRate })
                    }
                }
        )
    }
    catch { $limitations.Add("GPU/display identity unavailable: $($_.Exception.Message)") }
    $identity.limitations = $limitations.ToArray()
    return $identity
}

function Get-SourceIdentity {
    $repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
    $git = Get-Command git.exe -ErrorAction SilentlyContinue
    if ($null -eq $git) { $git = Get-Command git -ErrorAction SilentlyContinue }
    if ($null -eq $git -or -not (Test-Path -LiteralPath (Join-Path $repoRoot ".git"))) {
        return [ordered]@{
            revision = $null
            dirty = $null
            limitation = "Git source identity is unavailable; app_binary_sha256 remains authoritative."
        }
    }
    try {
        $revision = Invoke-NativeTool `
            -FilePath ([string]$git.Source) `
            -Arguments @("-C", $repoRoot, "rev-parse", "HEAD") `
            -FiniteTimeoutSeconds 30 `
            -Label "git revision"
        $status = Invoke-NativeTool `
            -FilePath ([string]$git.Source) `
            -Arguments @("-C", $repoRoot, "status", "--porcelain", "--untracked-files=normal") `
            -FiniteTimeoutSeconds 30 `
            -Label "git dirty state"
        if ($revision.exit_code -ne 0 -or $status.exit_code -ne 0) { throw "git identity command failed" }
        return [ordered]@{
            revision = $revision.stdout.Trim()
            dirty = -not [string]::IsNullOrWhiteSpace($status.stdout)
            limitation = $null
        }
    }
    catch {
        return [ordered]@{
            revision = $null
            dirty = $null
            limitation = "Git source identity failed: $($_.Exception.Message)"
        }
    }
}

function Measure-CpuAccountingQuantum {
    $reportedUnitMs = 0.0001
    try {
        $probe = Get-Process -Id $PID -ErrorAction Stop
        [int64]$previousTicks = $probe.TotalProcessorTime.Ticks
        [int64]$minimumPositiveTicks = [int64]::MaxValue
        $timer = [System.Diagnostics.Stopwatch]::StartNew()
        [double]$busyValue = 1
        while ($timer.ElapsedMilliseconds -lt 500) {
            for ($index = 1; $index -le 2048; $index++) {
                $busyValue = [Math]::Sqrt($busyValue + $index)
            }
            $probe.Refresh()
            [int64]$currentTicks = $probe.TotalProcessorTime.Ticks
            [int64]$deltaTicks = $currentTicks - $previousTicks
            if ($deltaTicks -gt 0 -and $deltaTicks -lt $minimumPositiveTicks) {
                $minimumPositiveTicks = $deltaTicks
            }
            $previousTicks = $currentTicks
        }
        if ($minimumPositiveTicks -ne [int64]::MaxValue) {
            return [pscustomobject][ordered]@{
                quantum_ms = [double]$minimumPositiveTicks / [TimeSpan]::TicksPerMillisecond
                method = "minimum positive TotalProcessorTime delta while the runner consumed CPU for 500 ms before app launch"
                reported_counter_unit_ms = $reportedUnitMs
                limitation = "Measured on the runner process; effective update granularity can differ by process and Windows version."
            }
        }
    }
    catch {}
    return [pscustomobject][ordered]@{
        quantum_ms = $reportedUnitMs
        method = "Win32_Process KernelModeTime/UserModeTime documented 100 ns counter unit fallback"
        reported_counter_unit_ms = $reportedUnitMs
        limitation = "No positive pre-launch TotalProcessorTime delta was observed; this is storage resolution, not effective scheduler accounting granularity."
    }
}

function New-ObserverSample {
    param(
        [object[]]$Rows,
        [double]$MonotonicMs,
        [double]$IntervalMs,
        [hashtable]$PreviousCpu,
        [hashtable]$PreviousJobAccounting,
        [hashtable]$PreviousSystemTimes,
        [string]$Profile,
        [int]$LogicalProcessors
    )
    $newJobNotifications = New-Object 'System.Collections.Generic.List[object]'
    foreach ($record in @(Receive-JobNotifications -WaitMilliseconds 0)) { $newJobNotifications.Add($record) }
    $jobSnapshot = Get-BenchmarkJobSnapshot
    foreach ($record in @(Sync-JobNotifications -ExpectedTotalProcesses ([uint64]$jobSnapshot.TotalProcesses))) {
        $newJobNotifications.Add($record)
    }
    $byPid = Update-CreatedProcessTree `
        -Rows $Rows `
        -RootPid ([int]$script:AppProcess.Id) `
        -JobSnapshot $jobSnapshot
    $alive = @(Get-AliveCreatedRows -ByPid $byPid)
    $processes = New-Object 'System.Collections.Generic.List[object]'
    [uint64]$privateTotal = 0
    [uint64]$workingTotal = 0
    [uint64]$readTotal = 0
    [uint64]$writeTotal = 0
    [uint64]$otherTotal = 0
    [uint64]$readOperationsTotal = 0
    [uint64]$writeOperationsTotal = 0
    [uint64]$otherOperationsTotal = 0
    [uint64]$handlesTotal = 0
    [uint64]$threadsTotal = 0
    [uint64]$cpuTimeTotal = [uint64]$jobSnapshot.CpuTime100ns
    [double]$cpuTotal = 0
    $cpuAvailable = $false
    foreach ($row in $alive) {
        $processPid = [int]$row.ProcessId
        $identity = Get-ProcessCreationIdentity $row
        $key = "$processPid|$identity"
        [uint64]$cpu100ns = [uint64]$row.KernelModeTime + [uint64]$row.UserModeTime
        $cpuPercent = $null
        if ($IntervalMs -gt 0 -and $PreviousCpu.ContainsKey($key)) {
            $delta100ns = [double]$cpu100ns - [double]$PreviousCpu[$key]
            if ($delta100ns -ge 0) {
                $cpuPercent = ($delta100ns / 10000.0) / $IntervalMs * 100.0 / [Math]::Max(1, $LogicalProcessors)
            }
        }
        $PreviousCpu[$key] = $cpu100ns
        $private = [uint64]$row.PrivatePageCount
        $working = [uint64]$row.WorkingSetSize
        $read = [uint64]$row.ReadTransferCount
        $write = [uint64]$row.WriteTransferCount
        $other = [uint64]$row.OtherTransferCount
        $readOperations = [uint64]$row.ReadOperationCount
        $writeOperations = [uint64]$row.WriteOperationCount
        $otherOperations = [uint64]$row.OtherOperationCount
        $handles = if ($null -eq $row.HandleCount) { 0 } else { [uint64]$row.HandleCount }
        $threads = if ($null -eq $row.ThreadCount) { 0 } else { [uint64]$row.ThreadCount }
        $executablePath = [string]$row.ExecutablePath
        if (
            [string]$row.Name -ieq "msedgewebview2.exe" -and
            -not [string]::IsNullOrWhiteSpace($executablePath) -and
            -not $script:WebViewVersionPaths.ContainsKey($executablePath)
        ) {
            $script:WebViewVersionPaths[$executablePath] = $true
            try {
                $webViewVersion = [System.Diagnostics.FileVersionInfo]::GetVersionInfo($executablePath).FileVersion
                if (-not [string]::IsNullOrWhiteSpace($webViewVersion)) {
                    $script:WebViewVersions[[string]$webViewVersion] = $true
                }
            }
            catch {}
        }
        $privateTotal += $private
        $workingTotal += $working
        $readTotal += $read
        $writeTotal += $write
        $otherTotal += $other
        $readOperationsTotal += $readOperations
        $writeOperationsTotal += $writeOperations
        $otherOperationsTotal += $otherOperations
        $handlesTotal += $handles
        $threadsTotal += $threads
        $processes.Add([ordered]@{
            pid = $processPid
            parent_pid = [int]$row.ParentProcessId
            creation_utc = $identity
            name = [string]$row.Name
            cpu_percent_normalized = $cpuPercent
            cpu_time_100ns = $cpu100ns
            private_bytes = $private
            working_set_bytes = $working
            io_read_bytes = $read
            io_write_bytes = $write
            io_other_bytes = $other
            io_read_operations = $readOperations
            io_write_operations = $writeOperations
            io_other_operations = $otherOperations
            handles = $handles
            threads = $threads
        })
    }
    if ($IntervalMs -gt 0 -and $PreviousJobAccounting.ContainsKey("cpu_time_100ns")) {
        $jobCpuDelta = [double]$cpuTimeTotal - [double]$PreviousJobAccounting["cpu_time_100ns"]
        if ($jobCpuDelta -ge 0) {
            $cpuTotal = ($jobCpuDelta / 10000.0) / $IntervalMs * 100.0 / [Math]::Max(1, $LogicalProcessors)
            $cpuAvailable = $true
        }
    }
    $PreviousJobAccounting["cpu_time_100ns"] = $cpuTimeTotal
    $readTotal = [uint64]$jobSnapshot.ReadBytes
    $writeTotal = [uint64]$jobSnapshot.WriteBytes
    $otherTotal = [uint64]$jobSnapshot.OtherBytes
    $readOperationsTotal = [uint64]$jobSnapshot.ReadOperations
    $writeOperationsTotal = [uint64]$jobSnapshot.WriteOperations
    $otherOperationsTotal = [uint64]$jobSnapshot.OtherOperations
    $currentJobMembers = @{}
    $joinedMembers = New-Object 'System.Collections.Generic.List[object]'
    foreach ($jobPid in @($jobSnapshot.ProcessIds)) {
        $pidKey = [string][int64]$jobPid
        if (-not $byPid.ContainsKey($pidKey)) { continue }
        $identity = Get-ProcessCreationIdentity $byPid[$pidKey]
        $currentJobMembers[$pidKey] = $identity
        if (-not $script:PreviousJobMembers.ContainsKey($pidKey) -or $script:PreviousJobMembers[$pidKey] -ne $identity) {
            $joinedMembers.Add([ordered]@{ pid = [int64]$jobPid; creation_utc = $identity })
        }
    }
    $exitedMembers = New-Object 'System.Collections.Generic.List[object]'
    foreach ($pidKey in @($script:PreviousJobMembers.Keys)) {
        if (-not $currentJobMembers.ContainsKey($pidKey)) {
            $exitedMembers.Add([ordered]@{
                pid = [int64]$pidKey
                creation_utc = [string]$script:PreviousJobMembers[$pidKey]
            })
        }
    }
    $script:PreviousJobMembers = $currentJobMembers
    $unobservedProcessCount = [Math]::Abs(
        [int64]$jobSnapshot.TotalProcesses - [int64]$script:JobNewProcessNotificationCount
    )
    $gpu = Get-GpuObservation -ProcessIds @($alive | ForEach-Object { [int]$_.ProcessId }) -Profile $Profile
    return [ordered]@{
        schema_version = 1
        timestamp_utc = [DateTime]::UtcNow.ToString("o")
        monotonic_ms = $MonotonicMs
        interval_ms = $IntervalMs
        cadence_gap = ($IntervalMs -gt 1500)
        process_tree = $processes.ToArray()
        job_membership = [ordered]@{
            joined = $joinedMembers.ToArray()
            exited = $exitedMembers.ToArray()
            total_processes = [uint32]$jobSnapshot.TotalProcesses
            active_processes = [uint32]$jobSnapshot.ActiveProcesses
            terminated_processes = [uint32]$jobSnapshot.TotalTerminatedProcesses
            unobserved_process_count = [uint64]$unobservedProcessCount
            new_process_notification_count = [uint64]$script:JobNewProcessNotificationCount
            completion_notifications = $newJobNotifications.ToArray()
        }
        aggregate = [ordered]@{
            process_count = $processes.Count
            cpu_percent_normalized = $(if ($cpuAvailable) { $cpuTotal } else { $null })
            cpu_time_100ns = $cpuTimeTotal
            private_bytes = $privateTotal
            working_set_bytes = $workingTotal
            io_read_bytes = $readTotal
            io_write_bytes = $writeTotal
            io_other_bytes = $otherTotal
            io_read_operations = $readOperationsTotal
            io_write_operations = $writeOperationsTotal
            io_other_operations = $otherOperationsTotal
            handles = $handlesTotal
            threads = $threadsTotal
            job_total_processes = [uint32]$jobSnapshot.TotalProcesses
            job_active_processes = [uint32]$jobSnapshot.ActiveProcesses
            job_terminated_processes = [uint32]$jobSnapshot.TotalTerminatedProcesses
            job_unobserved_process_count = [uint64]$unobservedProcessCount
            job_new_process_notification_count = [uint64]$script:JobNewProcessNotificationCount
        }
        whole_system_cpu_percent = Get-SystemCpuPercent -PreviousSystemTimes $PreviousSystemTimes
        gpu = $gpu
    }
}

function Stop-CreatedProcessTree {
    param([string]$Reason)
    try {
        $snapshot = Get-BenchmarkJobSnapshot
        if ([uint32]$snapshot.ActiveProcesses -gt 0) {
            $script:ForcedTerminationReason = $Reason
            [QueueBackReplayJob]::Terminate($script:BenchmarkJob, 2)
            Write-Host "Terminated benchmark Job Object reason=$Reason active_processes=$([uint32]$snapshot.ActiveProcesses)"
            Start-Sleep -Milliseconds 200
            return
        }
    }
    catch {
        [Console]::Error.WriteLine("Job Object termination failed; falling back to identity-bound process termination: $($_.Exception.Message)")
    }
    for ($pass = 0; $pass -lt 3; $pass++) {
        try { $rows = Get-ProcessRows } catch { return }
        $byPid = Update-CreatedProcessTree -Rows $rows -RootPid ([int]$script:AppProcess.Id)
        $alive = @(Get-AliveCreatedRows -ByPid $byPid)
        if ($alive.Count -eq 0) { return }
        $script:ForcedTerminationReason = $Reason
        $depth = @{}
        foreach ($row in $alive) {
            $value = 0
            $parent = [string][int]$row.ParentProcessId
            $seen = @{}
            while ($script:KnownProcesses.ContainsKey($parent) -and $byPid.ContainsKey($parent) -and $value -lt 64) {
                if ($seen.ContainsKey($parent)) { break }
                $seen[$parent] = $true
                $value++
                $parent = [string][int]$byPid[$parent].ParentProcessId
            }
            $depth[[string][int]$row.ProcessId] = $value
        }
        foreach ($row in @($alive | Sort-Object { $depth[[string][int]$_.ProcessId] } -Descending)) {
            $processPid = [int]$row.ProcessId
            try {
                Stop-Process -Id $processPid -Force -ErrorAction Stop
                Write-Host "Stopped created benchmark process pid=$processPid reason=$Reason"
            }
            catch {
                if (Get-Process -Id $processPid -ErrorAction SilentlyContinue) {
                    [Console]::Error.WriteLine("Could not stop created benchmark process pid=$processPid`: $($_.Exception.Message)")
                }
            }
        }
        Start-Sleep -Milliseconds 200
    }
}

function Get-PostHashes {
    param(
        [object[]]$FixtureFiles,
        [string]$ConfigPath,
        [string]$ExpectedConfigHash,
        [string]$DdragonCacheRoot,
        [string]$ExpectedDdragonCacheFingerprint
    )
    $rows = New-Object 'System.Collections.Generic.List[object]'
    $allMatch = $true
    foreach ($file in $FixtureFiles) {
        $exists = Test-Path -LiteralPath ([string]$file.absolute_path) -PathType Leaf
        $actualHash = if ($exists) { Get-Sha256 ([string]$file.absolute_path) } else { $null }
        $match = $exists -and $actualHash -eq [string]$file.sha256
        if (-not $match) { $allMatch = $false }
        $rows.Add([ordered]@{
            fixture_id = [string]$file.fixture_id
            relative_path = [string]$file.relative_path
            expected_sha256 = [string]$file.sha256
            actual_sha256 = $actualHash
            exists = $exists
            match = $match
        })
    }
    $configExists = Test-Path -LiteralPath $ConfigPath -PathType Leaf
    $configHash = if ($configExists) { Get-Sha256 $ConfigPath } else { $null }
    $configMatches = $configExists -and $configHash -eq $ExpectedConfigHash
    if (-not $configMatches) { $allMatch = $false }
    $ddragonCacheExists = Test-Path -LiteralPath $DdragonCacheRoot -PathType Container
    $ddragonCacheFingerprint = if ($ddragonCacheExists) {
        Get-DirectoryFingerprint -Root $DdragonCacheRoot -Label "ddragon.cache_root"
    }
    else { $null }
    $ddragonCacheMatches = $ddragonCacheExists -and $ddragonCacheFingerprint -eq $ExpectedDdragonCacheFingerprint
    if (-not $ddragonCacheMatches) { $allMatch = $false }
    return [ordered]@{
        schema_version = 1
        checked_utc = [DateTime]::UtcNow.ToString("o")
        all_match = $allMatch
        benchmark_config = [ordered]@{
            expected_sha256 = $ExpectedConfigHash
            actual_sha256 = $configHash
            exists = $configExists
            match = $configMatches
        }
        ddragon_cache = [ordered]@{
            path = $DdragonCacheRoot
            expected_fingerprint = $ExpectedDdragonCacheFingerprint
            actual_fingerprint = $ddragonCacheFingerprint
            exists = $ddragonCacheExists
            match = $ddragonCacheMatches
        }
        files = $rows.ToArray()
    }
}

function Get-JsonLineCount {
    param([string]$Path, [string]$Label)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        Stop-Benchmark "ARTIFACT_MISSING" "$Label is missing: $Path"
    }
    $count = 0
    $reader = [System.IO.File]::OpenText($Path)
    try {
        while ($null -ne ($line = $reader.ReadLine())) {
            if ([string]::IsNullOrWhiteSpace($line)) {
                Stop-Benchmark "ARTIFACT_INVALID" "$Label contains a blank JSONL record."
            }
            $count++
        }
    }
    finally { $reader.Dispose() }
    return $count
}

function Test-FrontendStartupObserved {
    param([string]$Path, [string]$RunId)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $false }
    try {
        $stream = [System.IO.File]::Open(
            $Path,
            [System.IO.FileMode]::Open,
            [System.IO.FileAccess]::Read,
            [System.IO.FileShare]::ReadWrite
        )
        $reader = [System.IO.StreamReader]::new($stream, [System.Text.UTF8Encoding]::new($false))
        try {
            while ($null -ne ($line = $reader.ReadLine())) {
                if ([string]::IsNullOrWhiteSpace($line)) { continue }
                try { $event = $line | ConvertFrom-Json }
                catch { continue }
                $frontendInitialized = (
                    [int]$event.schema_version -eq 1 -and
                    [string]$event.run_id -eq $RunId -and
                    [string]$event.source -eq "frontend" -and
                    [string]$event.kind -eq "frontend_initialized"
                )
                $frontendSessionRequested = (
                    [int]$event.schema_version -eq 1 -and
                    [string]$event.run_id -eq $RunId -and
                    [string]$event.source -eq "app" -and
                    [string]$event.kind -eq "frontend_session_requested"
                )
                if ($frontendInitialized -or $frontendSessionRequested) {
                    return $true
                }
            }
        }
        finally { $reader.Dispose() }
    }
    catch { return $false }
    return $false
}

function Test-PreparedMediaValid {
    param([object]$ManifestValue)
    if (-not $ManifestValue.PSObject.Properties["media_tools"]) { return $false }
    foreach ($fixture in @($ManifestValue.fixtures)) {
        if ([bool]$fixture.negative -or [string]$fixture.kind -eq "negative_fixture") { continue }
        $validations = @($fixture.media_validation)
        if ($validations.Count -eq 0) { return $false }
        foreach ($validation in $validations) {
            if ($validation.decode_ok -ne $true -or $null -eq $validation.ffprobe) { return $false }
            if (
                $validation.ffprobe.PSObject.Properties["error"] -or
                $validation.ffprobe.PSObject.Properties["parse_error"]
            ) {
                return $false
            }
        }
    }
    return $true
}

function Get-ClipDirectorySnapshot {
    param([string]$SentinelRoot, [string]$LibraryRoot)
    $clipsRoot = Get-FullAbsolutePath -Value (Join-Path $LibraryRoot "clips") -Label "benchmark clips root"
    Assert-StrictDescendant -Root $SentinelRoot -Path $clipsRoot -Label "benchmark clips root"
    Assert-StrictDescendant -Root $LibraryRoot -Path $clipsRoot -Label "benchmark clips root"
    [void](Assert-ReparseFree -Path $clipsRoot -Label "benchmark clips root")
    $entries = @{}
    if (Test-Path -LiteralPath $clipsRoot) {
        if (-not (Test-Path -LiteralPath $clipsRoot -PathType Container)) {
            Stop-Benchmark "CLIPS_ROOT" "The benchmark clips root exists but is not a directory."
        }
        foreach ($entry in @(Get-ChildItem -LiteralPath $clipsRoot -Force -ErrorAction Stop)) {
            if (($entry.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
                Stop-Benchmark "REPARSE_PATH" "The benchmark clips root contains a reparse entry."
            }
            $full = [System.IO.Path]::GetFullPath($entry.FullName)
            Assert-StrictDescendant -Root $clipsRoot -Path $full -Label "benchmark clip entry"
            $entries[[string]$entry.Name] = [pscustomobject][ordered]@{
                name = [string]$entry.Name
                is_directory = [bool]$entry.PSIsContainer
                size_bytes = $(if ($entry.PSIsContainer) { $null } else { [uint64]$entry.Length })
                absolute_path = $full
            }
        }
    }
    return [pscustomobject][ordered]@{ clips_root = $clipsRoot; entries = $entries }
}

function Get-ExportCompletionPayload {
    param([string]$EventsPath, [string]$RunId, [string]$ScenarioId, [string]$TrialId)
    $matches = New-Object 'System.Collections.Generic.List[object]'
    $reader = [System.IO.File]::OpenText($EventsPath)
    try {
        while ($null -ne ($line = $reader.ReadLine())) {
            if ([string]::IsNullOrWhiteSpace($line)) { continue }
            try { $event = $line | ConvertFrom-Json }
            catch { throw "events.jsonl contains malformed JSON while locating export evidence." }
            if (
                [int]$event.schema_version -eq 1 -and
                [string]$event.run_id -eq $RunId -and
                [string]$event.scenario_id -eq $ScenarioId -and
                [string]$event.trial_id -eq $TrialId -and
                [string]$event.kind -eq "export_completed"
            ) {
                $matches.Add($event.payload)
            }
        }
    }
    finally { $reader.Dispose() }
    if ($matches.Count -ne 1) {
        throw "Expected exactly one export_completed event, found $($matches.Count)."
    }
    return $matches[0]
}

function Get-TruncatedText {
    param([string]$Value, [int]$MaximumLength = 16384)
    if ($null -eq $Value) { return "" }
    if ($Value.Length -le $MaximumLength) { return $Value }
    return $Value.Substring(0, $MaximumLength)
}

function Invoke-ExportValidation {
    param(
        [object]$ManifestValue,
        [object]$Scenario,
        [object]$BeforeSnapshot,
        [string]$EventsPath,
        [string]$SentinelRoot,
        [string]$LibraryRoot,
        [int]$FiniteTimeoutSeconds
    )
    $validationStopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    $errors = New-Object 'System.Collections.Generic.List[string]'
    $validatedOutputs = New-Object 'System.Collections.Generic.List[object]'
    $newEntryEvidence = New-Object 'System.Collections.Generic.List[object]'
    $toleranceMs = if ($Scenario.PSObject.Properties["duration_tolerance_ms"]) {
        [int64]$Scenario.duration_tolerance_ms
    }
    else { 1000 }
    $report = [ordered]@{
        schema_version = 1
        run_id = [string]$ManifestValue.run_id
        scenario_id = [string]$Scenario.id
        trial_id = [string]$Scenario.trial_id
        checked_utc = [DateTime]::UtcNow.ToString("o")
        clips_root = "library_root\clips"
        expected_output_count = @($Scenario.export_presets).Count
        expected_duration_ms = [int64]$Scenario.expected_duration_ms
        duration_tolerance_ms = $toleranceMs
        expected_streams = [ordered]@{
            video_count = 1
            video_codec = [string]$Scenario.expected_video_codec
            audio_count = 1
            audio_codec = [string]$Scenario.expected_audio_codec
        }
        discord_limit_bytes_exclusive = 10000000
        validation_elapsed_ms = 0.0
        preexisting_entry_count = [int]$BeforeSnapshot.entries.Count
        newly_created_entries = @()
        outputs = @()
        errors = @()
        all_valid = $false
    }
    try {
        if (-not $ManifestValue.PSObject.Properties["media_tools"]) {
            throw "Export validation requires the packaged ffprobe and ffmpeg identities in media_tools."
        }
        $afterSnapshot = Get-ClipDirectorySnapshot -SentinelRoot $SentinelRoot -LibraryRoot $LibraryRoot
        if (-not (Test-PathEqual $afterSnapshot.clips_root $BeforeSnapshot.clips_root)) {
            throw "The clips root identity changed during the run."
        }
        $newEntries = @($afterSnapshot.entries.Values | Where-Object {
            -not $BeforeSnapshot.entries.ContainsKey([string]$_.name)
        })
        foreach ($entry in @($newEntries | Sort-Object name)) {
            $newEntryEvidence.Add([ordered]@{
                relative_path = "clips\$([string]$entry.name)"
                kind = $(if ($entry.is_directory) { "directory" } else { "file" })
                size_bytes = $entry.size_bytes
            })
            if ($entry.is_directory) {
                $errors.Add("Export created an unexpected directory '$([string]$entry.name)' under clips.")
            }
        }

        $payload = Get-ExportCompletionPayload `
            -EventsPath $EventsPath `
            -RunId ([string]$ManifestValue.run_id) `
            -ScenarioId ([string]$Scenario.id) `
            -TrialId ([string]$Scenario.trial_id)
        $eventOutputs = @($payload.outputs)
        $expectedPresets = @($Scenario.export_presets | ForEach-Object { [string]$_ })
        if ($eventOutputs.Count -ne $expectedPresets.Count) {
            $errors.Add("export_completed output count $($eventOutputs.Count) does not match the $($expectedPresets.Count) requested presets.")
        }
        $fixtureId = [string](@($Scenario.fixture_ids)[0])
        $fixture = @($ManifestValue.fixtures | Where-Object { [string]$_.id -eq $fixtureId })[0]
        if ($null -eq $fixture) { throw "The export fixture identity is absent from the manifest." }
        $expectedFilenamePrefix = ([string]$fixture.game_timestamp) + "_"
        $allowedNewNames = @{}
        $seenPresets = New-Object 'System.Collections.Generic.List[string]'
        $seenFilenames = @{}
        foreach ($eventOutput in $eventOutputs) {
            $outputErrors = New-Object 'System.Collections.Generic.List[string]'
            $filename = [string]$eventOutput.filename
            $preset = [string]$eventOutput.preset
            if (
                $filename -notmatch '^\d+(?:-[1-9]\d{0,2})?_\d+$' -or
                -not $filename.StartsWith($expectedFilenamePrefix, [System.StringComparison]::Ordinal)
            ) {
                $outputErrors.Add("export_completed contains an unsafe or fixture-mismatched filename.")
            }
            if ($seenFilenames.ContainsKey($filename)) { $outputErrors.Add("Duplicate export filename was reported.") }
            else { $seenFilenames[$filename] = $true }
            if ($preset -notin $expectedPresets) { $outputErrors.Add("Unexpected export preset '$preset'.") }
            else { $seenPresets.Add($preset) }

            $videoName = "$filename.mp4"
            $thumbnailName = "$filename.jpg"
            $allowedNewNames[$videoName] = $true
            $allowedNewNames[$thumbnailName] = $true
            if ($BeforeSnapshot.entries.ContainsKey($videoName) -or $BeforeSnapshot.entries.ContainsKey($thumbnailName)) {
                $outputErrors.Add("Export reused a preexisting clip filename instead of creating a new output.")
            }
            $videoEntry = if ($afterSnapshot.entries.ContainsKey($videoName)) { $afterSnapshot.entries[$videoName] } else { $null }
            $thumbnailEntry = if ($afterSnapshot.entries.ContainsKey($thumbnailName)) { $afterSnapshot.entries[$thumbnailName] } else { $null }
            if ($null -eq $videoEntry -or $videoEntry.is_directory) { $outputErrors.Add("Expected newly created MP4 is missing.") }
            if ($null -eq $thumbnailEntry -or $thumbnailEntry.is_directory) { $outputErrors.Add("Expected newly created thumbnail is missing.") }

            $probeEvidence = $null
            $decodeOk = $false
            $decodeStderr = ""
            $durationMs = $null
            $actualSize = if ($null -ne $videoEntry -and -not $videoEntry.is_directory) { [uint64]$videoEntry.size_bytes } else { $null }
            $videoHash = $null
            $thumbnailHash = $null
            if ($null -ne $videoEntry -and -not $videoEntry.is_directory) {
                $videoPath = Assert-ReparseFree -Path ([string]$videoEntry.absolute_path) -Label "new benchmark export"
                Assert-StrictDescendant -Root $afterSnapshot.clips_root -Path $videoPath -Label "new benchmark export"
                $videoHash = Get-Sha256 $videoPath
                if ($eventOutput.PSObject.Properties["file_size_bytes"] -and [uint64]$eventOutput.file_size_bytes -ne $actualSize) {
                    $outputErrors.Add("Export event size does not match the created MP4 size.")
                }
                if ($preset -eq "discord" -and $actualSize -ge 10000000) {
                    $outputErrors.Add("Discord output is not strictly below 10,000,000 bytes.")
                }
                try {
                    $probe = Invoke-NativeTool `
                        -FilePath ([string]$ManifestValue.media_tools.ffprobe.path) `
                        -Arguments @(
                            "-v", "error", "-show_entries", "format=duration,size:stream=index,codec_type,codec_name",
                            "-of", "json", $videoPath
                        ) `
                        -FiniteTimeoutSeconds $FiniteTimeoutSeconds `
                        -Label "ffprobe newly created export"
                    if ($probe.exit_code -ne 0) { throw "ffprobe exited with code $($probe.exit_code)." }
                    try { $probeEvidence = [string]$probe.stdout | ConvertFrom-Json }
                    catch { throw "ffprobe returned malformed JSON." }
                    $streams = @($probeEvidence.streams)
                    $videoStreams = @($streams | Where-Object { [string]$_.codec_type -eq "video" })
                    $audioStreams = @($streams | Where-Object { [string]$_.codec_type -eq "audio" })
                    if (
                        $streams.Count -ne 2 -or
                        $videoStreams.Count -ne 1 -or
                        [string]$videoStreams[0].codec_name -ne [string]$Scenario.expected_video_codec -or
                        $audioStreams.Count -ne 1 -or
                        [string]$audioStreams[0].codec_name -ne [string]$Scenario.expected_audio_codec
                    ) {
                        $outputErrors.Add("Export stream count or codecs do not match the declared one-video/one-audio contract.")
                    }
                    [double]$durationSeconds = 0
                    if (-not [double]::TryParse(
                        [string]$probeEvidence.format.duration,
                        [System.Globalization.NumberStyles]::Float,
                        [System.Globalization.CultureInfo]::InvariantCulture,
                        [ref]$durationSeconds
                    ) -or [double]::IsNaN($durationSeconds) -or [double]::IsInfinity($durationSeconds)) {
                        $outputErrors.Add("Export duration is missing or non-finite.")
                    }
                    else {
                        $durationMs = $durationSeconds * 1000.0
                        if ([Math]::Abs($durationMs - [double]$Scenario.expected_duration_ms) -gt $toleranceMs) {
                            $outputErrors.Add("Export duration is outside the declared tolerance.")
                        }
                    }
                    [uint64]$probedSize = 0
                    if (
                        -not [uint64]::TryParse([string]$probeEvidence.format.size, [ref]$probedSize) -or
                        $probedSize -ne $actualSize
                    ) {
                        $outputErrors.Add("ffprobe size does not match the created MP4 size.")
                    }
                }
                catch { $outputErrors.Add("ffprobe validation failed: $($_.Exception.Message)") }
                try {
                    $decode = Invoke-NativeTool `
                        -FilePath ([string]$ManifestValue.media_tools.ffmpeg.path) `
                        -Arguments @(
                            "-nostdin", "-v", "error", "-threads", "1", "-i", $videoPath,
                            "-map", "0:v:0", "-map", "0:a:0", "-threads", "1", "-f", "null", "NUL"
                        ) `
                        -FiniteTimeoutSeconds $FiniteTimeoutSeconds `
                        -Label "full single-thread decode of newly created export"
                    $decodeOk = $decode.exit_code -eq 0
                    $decodeStderr = Get-TruncatedText -Value ([string]$decode.stderr)
                    if (-not $decodeOk) { $outputErrors.Add("Full single-thread decode failed with code $($decode.exit_code).") }
                }
                catch { $outputErrors.Add("Full single-thread decode failed: $($_.Exception.Message)") }
            }
            if ($null -ne $thumbnailEntry -and -not $thumbnailEntry.is_directory) {
                $thumbnailPath = Assert-ReparseFree -Path ([string]$thumbnailEntry.absolute_path) -Label "new benchmark export thumbnail"
                Assert-StrictDescendant -Root $afterSnapshot.clips_root -Path $thumbnailPath -Label "new benchmark export thumbnail"
                $thumbnailHash = Get-Sha256 $thumbnailPath
            }
            foreach ($message in $outputErrors) { $errors.Add("$preset/$filename`: $message") }
            $validatedOutputs.Add([ordered]@{
                filename = $filename
                preset = $preset
                relative_path = "clips\$videoName"
                thumbnail_relative_path = "clips\$thumbnailName"
                size_bytes = $actualSize
                sha256 = $videoHash
                thumbnail_sha256 = $thumbnailHash
                duration_ms = $durationMs
                ffprobe = $probeEvidence
                full_single_thread_decode_ok = $decodeOk
                decode_stderr = $decodeStderr
                discord_size_valid = $(if ($preset -eq "discord") { $null -ne $actualSize -and $actualSize -lt 10000000 } else { $null })
                valid = ($outputErrors.Count -eq 0)
            })
        }
        $actualPresets = @($seenPresets | Sort-Object)
        $sortedExpectedPresets = @($expectedPresets | Sort-Object)
        if (($actualPresets -join "|") -ne ($sortedExpectedPresets -join "|")) {
            $errors.Add("The export_completed presets do not exactly match the requested presets.")
        }
        foreach ($entry in $newEntries) {
            if (-not $allowedNewNames.ContainsKey([string]$entry.name)) {
                $errors.Add("Unexpected newly created clips entry '$([string]$entry.name)'.")
            }
        }
    }
    catch { $errors.Add([string]$_.Exception.Message) }
    $report.newly_created_entries = $newEntryEvidence.ToArray()
    $report.outputs = $validatedOutputs.ToArray()
    $report.errors = $errors.ToArray()
    $report.all_valid = $errors.Count -eq 0 -and $validatedOutputs.Count -eq [int]$report.expected_output_count
    $report.validation_elapsed_ms = $validationStopwatch.Elapsed.TotalMilliseconds
    return $report
}

function Test-ProcessTelemetryComplete {
    param(
        [string]$Path,
        [string]$RunId,
        [string]$ScenarioId,
        [string]$TrialId,
        [double]$MaximumGapMs = 2500
    )
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $false }
    $records = New-Object 'System.Collections.Generic.List[object]'
    $reader = [System.IO.File]::OpenText($Path)
    try {
        while ($null -ne ($line = $reader.ReadLine())) {
            if ([string]::IsNullOrWhiteSpace($line)) { return $false }
            try { $record = $line | ConvertFrom-Json } catch { return $false }
            if (
                [int]$record.schema_version -ne 1 -or
                [string]$record.run_id -ne $RunId -or
                [string]$record.scenario_id -ne $ScenarioId -or
                [string]$record.trial_id -ne $TrialId -or
                [int]$record.process_count -lt 1 -or
                $null -eq $record.process_tree_cpu_percent -or
                $null -eq $record.process_tree_private_bytes -or
                [uint64]$record.job_unobserved_process_count -ne 0
            ) {
                return $false
            }
            $records.Add($record)
        }
    }
    finally { $reader.Dispose() }
    if ($records.Count -lt 2) { return $false }
    for ($index = 1; $index -lt $records.Count; $index++) {
        $gap = [double]$records[$index].monotonic_ms - [double]$records[$index - 1].monotonic_ms
        if ($gap -lt 0 -or $gap -gt $MaximumGapMs) { return $false }
    }
    return $true
}

function Get-ObserverContext {
    param([string]$Path, [string]$RunId, [hashtable]$ExpectedTrials)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return [pscustomobject]@{ active = $false; status = "missing" }
    }
    try {
        $context = Get-Content -LiteralPath $Path -Raw -Encoding UTF8 | ConvertFrom-Json
        if ([int]$context.schema_version -ne 1 -or [string]$context.run_id -ne $RunId) {
            return [pscustomobject]@{ active = $false; status = "identity_mismatch" }
        }
        $scenarioId = [string]$context.scenario_id
        $trialId = [string]$context.trial_id
        if ([string]::IsNullOrWhiteSpace($scenarioId) -or [string]::IsNullOrWhiteSpace($trialId)) {
            return [pscustomobject]@{ active = $false; status = "incomplete" }
        }
        if (-not $ExpectedTrials.ContainsKey("$scenarioId|$trialId")) {
            return [pscustomobject]@{ active = $false; status = "undeclared_trial" }
        }
        if ($context.PSObject.Properties["active"] -and [bool]$context.active -eq $false) {
            return [pscustomobject]@{
                active = $false
                status = "inactive"
                scenario_id = $scenarioId
                trial_id = $trialId
            }
        }
        return [pscustomobject]@{
            active = $true
            status = "active"
            scenario_id = $scenarioId
            trial_id = $trialId
        }
    }
    catch {
        return [pscustomobject]@{ active = $false; status = "unreadable" }
    }
}

function Update-TerminalForObserver {
    param(
        [string]$Path,
        [string]$RunId,
        [int]$EventCount,
        [int]$RequestCount,
        [int]$SampleCount,
        [bool]$SourceHashesMatch,
        [bool]$MediaValid,
        [bool]$TelemetryComplete
    )
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        Stop-Benchmark "TERMINAL" "The app did not publish terminal.json."
    }
    try { $terminal = Get-Content -LiteralPath $Path -Raw -Encoding UTF8 | ConvertFrom-Json }
    catch { Stop-Benchmark "TERMINAL" "terminal.json is malformed: $($_.Exception.Message)" }
    $appTerminalPath = Join-Path (Split-Path -Parent $Path) "terminal.app.json"
    if (Test-Path -LiteralPath $appTerminalPath) {
        Stop-Benchmark "TERMINAL" "terminal.app.json already exists; the immutable run root was reused."
    }
    [System.IO.File]::Copy($Path, $appTerminalPath, $false)
    if ([int]$terminal.schema_version -ne 1 -or [string]$terminal.run_id -ne $RunId) {
        Stop-Benchmark "TERMINAL" "App terminal identity does not match the launch manifest."
    }
    $payload = if ($terminal.PSObject.Properties["payload"] -and $null -ne $terminal.payload) { $terminal.payload } else { $null }
    $appStatus = if ($terminal.PSObject.Properties["status"]) {
        [string]$terminal.status
    }
    elseif ($null -ne $payload -and $payload.PSObject.Properties["status"]) {
        [string]$payload.status
    }
    else { "" }
    if ($appStatus.ToLowerInvariant() -notin @("complete", "completed", "success", "passed")) {
        Stop-Benchmark "TERMINAL" "App terminal status '$appStatus' is not successful."
    }
    if ($terminal.PSObject.Properties["source_hash_unchanged"] -and $terminal.source_hash_unchanged -eq $false) {
        Stop-Benchmark "SOURCE_CHANGED" "The app reported source mutation."
    }
    $terminal | Add-Member -NotePropertyName "source_hash_unchanged" -NotePropertyValue $SourceHashesMatch -Force
    if ($null -eq $terminal.PSObject.Properties["record_counts"] -or $null -eq $terminal.record_counts) {
        $terminal | Add-Member -NotePropertyName "record_counts" -NotePropertyValue ([pscustomobject]@{}) -Force
    }
    if ($null -eq $terminal.PSObject.Properties["dropped_counts"] -or $null -eq $terminal.dropped_counts) {
        $terminal | Add-Member -NotePropertyName "dropped_counts" -NotePropertyValue ([pscustomobject]@{}) -Force
    }
    foreach ($name in @("events", "server_requests")) {
        $existingDropped = if ($terminal.dropped_counts.PSObject.Properties[$name]) {
            [int]$terminal.dropped_counts.$name
        }
        elseif ($null -ne $payload -and $payload.PSObject.Properties["dropped_counts"] -and $payload.dropped_counts.PSObject.Properties[$name]) {
            [int]$payload.dropped_counts.$name
        }
        else { 0 }
        if ($existingDropped -ne 0) {
            Stop-Benchmark "TELEMETRY_LOSS" "App terminal reports dropped $name records."
        }
    }
    $terminal.record_counts | Add-Member -NotePropertyName "events" -NotePropertyValue $EventCount -Force
    $terminal.record_counts | Add-Member -NotePropertyName "server_requests" -NotePropertyValue $RequestCount -Force
    $terminal.record_counts | Add-Member -NotePropertyName "process_samples" -NotePropertyValue $SampleCount -Force
    $terminal.dropped_counts | Add-Member -NotePropertyName "events" -NotePropertyValue 0 -Force
    $terminal.dropped_counts | Add-Member -NotePropertyName "server_requests" -NotePropertyValue 0 -Force
    $terminal.dropped_counts | Add-Member -NotePropertyName "process_samples" -NotePropertyValue 0 -Force
    $terminal | Add-Member -NotePropertyName "completed" -NotePropertyValue $true -Force
    $terminal | Add-Member -NotePropertyName "finished" -NotePropertyValue $true -Force
    $terminal | Add-Member -NotePropertyName "telemetry_complete" -NotePropertyValue $TelemetryComplete -Force
    $terminal | Add-Member -NotePropertyName "media_valid" -NotePropertyValue $MediaValid -Force
    $terminal | Add-Member -NotePropertyName "media_integrity_ok" -NotePropertyValue $MediaValid -Force
    $terminal | Add-Member -NotePropertyName "observer_finalized_utc" -NotePropertyValue ([DateTime]::UtcNow.ToString("o")) -Force
    Write-JsonFileAtomic -Path $Path -Value $terminal
}

try {
    if ($env:OS -ne "Windows_NT") { Stop-Benchmark "WINDOWS_REQUIRED" "Replay benchmark runner supports Windows only." }
    if ($PSVersionTable.PSVersion.Major -lt 5) { Stop-Benchmark "POWERSHELL_VERSION" "PowerShell 5.1 or newer is required." }

    $validated = Read-AndValidateManifest -Path $Manifest
    $manifestValue = $validated.value
    $manifestApp = if ($manifestValue.PSObject.Properties["app_binary"]) { $manifestValue.app_binary } else { $null }
    $configuredApp = Resolve-ConfiguredPath -CommandLineValue $AppBinary -ManifestValue $manifestApp -Label "AppBinary"
    if ([string]::IsNullOrWhiteSpace([string]$configuredApp)) { Stop-Benchmark "APP_BINARY" "Provide -AppBinary or manifest app_binary." }
    $appPath = Assert-ReparseFree -Path ([string]$configuredApp) -Label "AppBinary"
    if (-not (Test-Path -LiteralPath $appPath -PathType Leaf)) { Stop-Benchmark "APP_BINARY" "AppBinary does not exist." }
    Assert-PeExecutable -Path $appPath
    $packagedMediaRuntime = Assert-PackagedMediaRuntime -AppPath $appPath -ManifestValue $manifestValue

    $manifestAnalyzer = if ($manifestValue.PSObject.Properties["analyzer_path"]) { $manifestValue.analyzer_path } else { $null }
    $configuredAnalyzer = Resolve-ConfiguredPath -CommandLineValue $AnalyzerPath -ManifestValue $manifestAnalyzer -Label "AnalyzerPath"
    if ([string]::IsNullOrWhiteSpace([string]$configuredAnalyzer)) {
        $configuredAnalyzer = Join-Path $PSScriptRoot "analyze.py"
    }
    $analyzer = Assert-ReparseFree -Path ([string]$configuredAnalyzer -replace '^$', ' ') -Label "AnalyzerPath"
    if (-not (Test-Path -LiteralPath $analyzer -PathType Leaf)) { Stop-Benchmark "ANALYZER" "Analyzer does not exist: $analyzer" }

    $manifestPython = if ($manifestValue.PSObject.Properties["python_path"]) { [string]$manifestValue.python_path } else { $null }
    $configuredPython = if (-not [string]::IsNullOrWhiteSpace($PythonPath)) { $PythonPath } elseif (-not [string]::IsNullOrWhiteSpace($manifestPython)) { $manifestPython } else { "python" }
    $python = Resolve-ExecutableCommand -Value $configuredPython -Label "PythonPath"

    $manifestTimeout = if ($manifestValue.PSObject.Properties["timeout_seconds"]) { [int]$manifestValue.timeout_seconds } else { 3600 }
    $finiteTimeout = if ($TimeoutSeconds -gt 0) { $TimeoutSeconds } else { $manifestTimeout }
    if ($finiteTimeout -lt 30 -or $finiteTimeout -gt 86400) { Stop-Benchmark "TIMEOUT" "Timeout must be between 30 and 86400 seconds." }
    $frontendStartupTimeout = [int][Math]::Min(30, $finiteTimeout)

    if ($PreflightOnly) {
        Write-Host "QB-REPLAY-RUN-PREFLIGHT-OK: schema v1, sentinel/root safety, new result root, app/analyzer and packaged-runtime identity, and $($validated.fixture_files.Count) prepared fixture file identity record(s) passed without warming media via pre-run hashing."
        Write-Host "LIVE-CHECKS-DEFERRED: production WebView launch, descendant sampling, finite exit, post-hashes, and analyzer execution."
        exit 0
    }

    if (Test-Path -LiteralPath $validated.result_root) { Stop-Benchmark "RESULT_EXISTS" "result_root appeared after preflight." }
    New-Item -ItemType Directory -Path $validated.result_root -ErrorAction Stop | Out-Null
    $script:ResultRootCreated = $validated.result_root
    [void](Assert-ReparseFree -Path $validated.result_root -Label "created result_root")
    [System.IO.File]::Copy($validated.path, (Join-Path $validated.result_root "manifest.json"), $false)
    $launchScenario = @($manifestValue.scenarios)[0]
    $beforeClipSnapshot = $null
    if ([string]$launchScenario.kind -eq "export") {
        $beforeClipSnapshot = Get-ClipDirectorySnapshot `
            -SentinelRoot $validated.sentinel_root `
            -LibraryRoot $validated.library_root
    }
    $cpuAccounting = Measure-CpuAccountingQuantum
    $gpuProbe = Initialize-GpuObservation -Profile ([string]$manifestValue.observer_profile)

    $runnerMetadata = [ordered]@{
        schema_version = 1
        run_id = [string]$manifestValue.run_id
        started_utc = [DateTime]::UtcNow.ToString("o")
        manifest_path = $validated.path
        manifest_sha256 = $validated.hash
        config_sha256 = $validated.config_sha256
        app_binary = $appPath
        app_binary_sha256 = Get-Sha256 $appPath
        analyzer_path = $analyzer
        python_path = $python
        timeout_seconds = $finiteTimeout
        frontend_startup_timeout_seconds = $frontendStartupTimeout
        observer_profile = [string]$manifestValue.observer_profile
        observer_cadence_ms = 1000
        logical_processors = [Environment]::ProcessorCount
        cpu_accounting_quantum_ms = [double]$cpuAccounting.quantum_ms
        cpu_accounting_quantum_method = [string]$cpuAccounting.method
        cpu_accounting_reported_counter_unit_ms = [double]$cpuAccounting.reported_counter_unit_ms
        cpu_accounting_limitation = [string]$cpuAccounting.limitation
        gpu_collection = "Optional Windows GPU CIM counters are disabled because the provider has no runner-enforced finite deadline; the limitation is recorded per sample"
        gpu_collection_probe = $gpuProbe
        process_tree_collection_method = "Windows Job Object IO-completion-port notifications prove every member creation/exit; Toolhelp/PInvoke snapshots provide live identities and counters without managed module enumeration; cumulative Job accounting retains exited-member CPU and I/O"
        process_tree_assignment_limitation = "The root process is created suspended and assigned before its primary thread resumes; descendants cannot request breakaway because the Job does not enable a breakaway limit."
        environment = Get-EnvironmentIdentity
        source = Get-SourceIdentity
        media_runtime_id = $(if ($manifestValue.PSObject.Properties["media_tools"]) { [string]$manifestValue.media_tools.runtime_id } else { $null })
        packaged_media_runtime = $packagedMediaRuntime
        ddragon_cache_root = $validated.ddragon_cache_root
        ddragon_cache_fingerprint = $validated.ddragon_cache_fingerprint
    }
    Write-JsonFile -Path (Join-Path $validated.result_root "runner-metadata.json") -Value $runnerMetadata

    $stdoutPath = Join-Path $validated.result_root "app.stdout.log"
    $stderrPath = Join-Path $validated.result_root "app.stderr.log"
    $argumentLine = (ConvertTo-NativeArgument "--replay-benchmark-manifest") + " " + (ConvertTo-NativeArgument $validated.path)
    $script:AppProcess = Start-BenchmarkProcess `
        -FilePath $appPath `
        -ArgumentLine $argumentLine `
        -StdoutPath $stdoutPath `
        -StderrPath $stderrPath
    $script:KnownProcesses[[string][int]$script:AppProcess.Id] = `
        $script:AppProcess.StartTime.ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ss.fffZ")

    $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    $previousCpu = @{}
    $previousJobAccounting = @{}
    $previousSystemTimes = @{}
    $previousSampleMs = 0.0
    $telemetryPath = Join-Path $validated.result_root "observer.jsonl"
    $writer = [System.IO.StreamWriter]::new($telemetryPath, $false, [System.Text.UTF8Encoding]::new($false))
    $writer.AutoFlush = $true
    $processSamplesPath = Join-Path $validated.result_root "process_samples.jsonl"
    $processWriter = [System.IO.StreamWriter]::new($processSamplesPath, $false, [System.Text.UTF8Encoding]::new($false))
    $processWriter.AutoFlush = $true
    $observerContextPath = Join-Path $validated.result_root "observer-context.json"
    $expectedTrials = @{}
    foreach ($scenario in @($manifestValue.scenarios)) {
        $expectedTrials["$([string]$scenario.id)|$([string]$scenario.trial_id)"] = $true
    }
    $processSampleCount = 0
    $timedOut = $false
    $startupTimedOut = $false
    $frontendStartupObserved = $false
    $rootExitObservedAt = $null
    $eventsPath = Join-Path $validated.result_root "events.jsonl"
    try {
        while ($true) {
            $script:AppProcess.Refresh()
            if ($script:AppProcess.HasExited) {
                if ($null -eq $rootExitObservedAt) {
                    $rootExitObservedAt = $stopwatch.Elapsed.TotalMilliseconds
                }
                $jobAtExit = Get-BenchmarkJobSnapshot
                if (
                    [uint32]$jobAtExit.ActiveProcesses -eq 0 -or
                    $stopwatch.Elapsed.TotalMilliseconds - $rootExitObservedAt -ge 5000
                ) {
                    break
                }
                Start-Sleep -Milliseconds 100
                continue
            }
            $sampleStarted = $stopwatch.Elapsed.TotalMilliseconds
            $rows = Get-ProcessRows
            $interval = if ($previousSampleMs -eq 0) { 0.0 } else { $sampleStarted - $previousSampleMs }
            $sample = New-ObserverSample `
                -Rows $rows `
                -MonotonicMs $sampleStarted `
                -IntervalMs $interval `
                -PreviousCpu $previousCpu `
                -PreviousJobAccounting $previousJobAccounting `
                -PreviousSystemTimes $previousSystemTimes `
                -Profile ([string]$manifestValue.observer_profile) `
                -LogicalProcessors ([Environment]::ProcessorCount)
            $sample["collection_duration_ms"] = $stopwatch.Elapsed.TotalMilliseconds - $sampleStarted
            $context = Get-ObserverContext `
                -Path $observerContextPath `
                -RunId ([string]$manifestValue.run_id) `
                -ExpectedTrials $expectedTrials
            $sample["observer_context_status"] = [string]$context.status
            $writer.WriteLine(($sample | ConvertTo-Json -Depth 20 -Compress))
            if (
                $context.active -and
                [int]$sample.aggregate.process_count -gt 0 -and
                $null -ne $sample.aggregate.cpu_percent_normalized
            ) {
                $processRecord = [ordered]@{
                    schema_version = 1
                    run_id = [string]$manifestValue.run_id
                    scenario_id = [string]$context.scenario_id
                    trial_id = [string]$context.trial_id
                    monotonic_ms = $sampleStarted
                    source = "windows_observer"
                    kind = "process_sample"
                    process_count = [int]$sample.aggregate.process_count
                    process_tree_cpu_percent = [double]$sample.aggregate.cpu_percent_normalized
                    process_tree_cpu_time_100ns = [uint64]$sample.aggregate.cpu_time_100ns
                    cpu_accounting_quantum_ms = [double]$cpuAccounting.quantum_ms
                    process_tree_private_bytes = [uint64]$sample.aggregate.private_bytes
                    working_set_bytes = [uint64]$sample.aggregate.working_set_bytes
                    io_read_bytes = [uint64]$sample.aggregate.io_read_bytes
                    io_write_bytes = [uint64]$sample.aggregate.io_write_bytes
                    io_other_bytes = [uint64]$sample.aggregate.io_other_bytes
                    io_read_operations = [uint64]$sample.aggregate.io_read_operations
                    io_write_operations = [uint64]$sample.aggregate.io_write_operations
                    io_other_operations = [uint64]$sample.aggregate.io_other_operations
                    handles = [uint64]$sample.aggregate.handles
                    threads = [uint64]$sample.aggregate.threads
                    job_total_processes = [uint32]$sample.aggregate.job_total_processes
                    job_active_processes = [uint32]$sample.aggregate.job_active_processes
                    job_terminated_processes = [uint32]$sample.aggregate.job_terminated_processes
                    job_unobserved_process_count = [uint64]$sample.aggregate.job_unobserved_process_count
                    job_membership = $sample.job_membership
                    whole_system_cpu_percent = $sample.whole_system_cpu_percent
                    cadence_gap = [bool]$sample.cadence_gap
                    collection_duration_ms = [double]$sample.collection_duration_ms
                    gpu = $sample.gpu
                }
                $processWriter.WriteLine(($processRecord | ConvertTo-Json -Depth 20 -Compress))
                $processSampleCount++
            }
            $previousSampleMs = $sampleStarted

            if (-not $frontendStartupObserved) {
                $frontendStartupObserved = Test-FrontendStartupObserved `
                    -Path $eventsPath `
                    -RunId ([string]$manifestValue.run_id)
                if (
                    -not $frontendStartupObserved -and
                    $stopwatch.Elapsed.TotalSeconds -ge $frontendStartupTimeout
                ) {
                    $startupTimedOut = $true
                    break
                }
            }
            if ($stopwatch.Elapsed.TotalSeconds -ge $finiteTimeout) {
                $timedOut = $true
                break
            }
            $script:AppProcess.Refresh()
            if ($script:AppProcess.HasExited) {
                if ($null -eq $rootExitObservedAt) { $rootExitObservedAt = $stopwatch.Elapsed.TotalMilliseconds }
                $byPid = Update-CreatedProcessTree -Rows $rows -RootPid ([int]$script:AppProcess.Id)
                if (@(Get-AliveCreatedRows -ByPid $byPid).Count -eq 0) { break }
                if ($stopwatch.Elapsed.TotalMilliseconds - $rootExitObservedAt -ge 5000) { break }
            }
            $remaining = 1000.0 - ($stopwatch.Elapsed.TotalMilliseconds - $sampleStarted)
            if ($remaining -gt 0) { Start-Sleep -Milliseconds ([int][Math]::Ceiling($remaining)) }
        }
    }
    finally {
        $writer.Dispose()
        $processWriter.Dispose()
    }
    $observationDurationMs = $stopwatch.Elapsed.TotalMilliseconds

    if ($startupTimedOut) {
        Stop-CreatedProcessTree -Reason "frontend-startup-timeout"
    }
    elseif ($timedOut) {
        Stop-CreatedProcessTree -Reason "finite-timeout"
    }
    else {
        $script:AppProcess.Refresh()
        if (-not $script:AppProcess.HasExited) { Stop-CreatedProcessTree -Reason "post-exit-child-grace" }
        else {
            try {
                $jobAfterExit = Get-BenchmarkJobSnapshot
                if ([uint32]$jobAfterExit.ActiveProcesses -gt 0) {
                    Stop-CreatedProcessTree -Reason "post-exit-child-grace"
                }
            }
            catch {}
        }
    }
    Complete-BenchmarkOutput
    $finalJobSnapshot = Get-BenchmarkJobSnapshot
    $null = @(Sync-JobNotifications `
        -ExpectedTotalProcesses ([uint64]$finalJobSnapshot.TotalProcesses) `
        -BudgetMilliseconds 250)
    $finalJobSnapshot = Get-BenchmarkJobSnapshot
    $null = @(Sync-JobNotifications `
        -ExpectedTotalProcesses ([uint64]$finalJobSnapshot.TotalProcesses) `
        -BudgetMilliseconds 250)
    $finalUnobservedProcessCount = [Math]::Abs(
        [int64]$finalJobSnapshot.TotalProcesses - [int64]$script:JobNewProcessNotificationCount
    )

    $integrityStarted = [System.Diagnostics.Stopwatch]::StartNew()
    $postHashes = Get-PostHashes `
        -FixtureFiles $validated.fixture_files `
        -ConfigPath $validated.config_path `
        -ExpectedConfigHash $validated.config_sha256 `
        -DdragonCacheRoot $validated.ddragon_cache_root `
        -ExpectedDdragonCacheFingerprint $validated.ddragon_cache_fingerprint
    $integrityStarted.Stop()
    $postHashElapsedMs = $integrityStarted.Elapsed.TotalMilliseconds
    Write-JsonFile -Path (Join-Path $validated.result_root "post-hashes.json") -Value $postHashes
    $exitCode = $null
    $script:AppProcess.Refresh()
    if ($script:AppProcess.HasExited) { $exitCode = [int]$script:AppProcess.ExitCode }
    $collection = [ordered]@{
        schema_version = 1
        run_id = [string]$manifestValue.run_id
        completed_utc = [DateTime]::UtcNow.ToString("o")
        duration_ms = $observationDurationMs
        post_hash_elapsed_ms = $postHashElapsedMs
        app_pid = [int]$script:AppProcess.Id
        app_exit_code = $exitCode
        timed_out = $timedOut
        startup_timed_out = $startupTimedOut
        frontend_startup_observed = $frontendStartupObserved
        frontend_startup_timeout_seconds = $frontendStartupTimeout
        forced_termination_reason = $script:ForcedTerminationReason
        fixture_hashes_match = [bool]$postHashes.all_match
        discovered_process_identities = $script:KnownProcesses
        job_accounting = [ordered]@{
            cpu_time_100ns = [uint64]$finalJobSnapshot.CpuTime100ns
            total_processes = [uint32]$finalJobSnapshot.TotalProcesses
            active_processes = [uint32]$finalJobSnapshot.ActiveProcesses
            terminated_processes = [uint32]$finalJobSnapshot.TotalTerminatedProcesses
            unobserved_process_count = [uint64]$finalUnobservedProcessCount
            new_process_notification_count = [uint64]$script:JobNewProcessNotificationCount
            io_read_operations = [uint64]$finalJobSnapshot.ReadOperations
            io_write_operations = [uint64]$finalJobSnapshot.WriteOperations
            io_other_operations = [uint64]$finalJobSnapshot.OtherOperations
            io_read_bytes = [uint64]$finalJobSnapshot.ReadBytes
            io_write_bytes = [uint64]$finalJobSnapshot.WriteBytes
            io_other_bytes = [uint64]$finalJobSnapshot.OtherBytes
        }
        job_completion_notifications = $script:JobNotifications.ToArray()
        webview2_runtime_versions = @($script:WebViewVersions.Keys | Sort-Object)
    }
    $webViewVersions = @($script:WebViewVersions.Keys | Sort-Object)
    $runnerMetadata["webview2_runtime_versions"] = $webViewVersions
    $runnerMetadata["webview2_runtime_limitation"] = $(
        if ($webViewVersions.Count -eq 0) {
            "No descendant msedgewebview2.exe executable version was observable through Win32_Process."
        }
        else { $null }
    )
    Write-JsonFile -Path (Join-Path $validated.result_root "runner-metadata.json") -Value $runnerMetadata
    Write-JsonFile -Path (Join-Path $validated.result_root "collection-result.json") -Value $collection
    if ($startupTimedOut) {
        Stop-Benchmark "APP_STARTUP_TIMEOUT" "Benchmark frontend did not request its session or emit frontend_initialized within $frontendStartupTimeout seconds."
    }
    if ($timedOut) { Stop-Benchmark "APP_TIMEOUT" "Benchmark app exceeded the finite $finiteTimeout-second timeout." }
    if ($null -eq $exitCode -or $exitCode -ne 0) { Stop-Benchmark "APP_EXIT" "Benchmark app exited with code $exitCode." }
    if (-not [string]::IsNullOrWhiteSpace([string]$script:ForcedTerminationReason)) {
        Stop-Benchmark "PROCESS_LIFETIME" "The benchmark required forced Job Object termination: $($script:ForcedTerminationReason)."
    }
    if ([uint64]$finalUnobservedProcessCount -ne 0) {
        Stop-Benchmark "PROCESS_TREE" "Job accounting and NEW_PROCESS completion notifications differ by $finalUnobservedProcessCount process(es)."
    }
    if (-not [bool]$postHashes.all_match) { Stop-Benchmark "SOURCE_CHANGED" "A prepared fixture, benchmark config, or Data Dragon cache identity changed during the run." }
    $requestsPath = Join-Path $validated.result_root "server_requests.jsonl"
    $eventCount = Get-JsonLineCount -Path $eventsPath -Label "events.jsonl"
    $requestCount = Get-JsonLineCount -Path $requestsPath -Label "server_requests.jsonl"
    $processTelemetryComplete = Test-ProcessTelemetryComplete `
        -Path $processSamplesPath `
        -RunId ([string]$manifestValue.run_id) `
        -ScenarioId ([string]$launchScenario.id) `
        -TrialId ([string]$launchScenario.trial_id)
    $finalObserverContext = Get-ObserverContext `
        -Path $observerContextPath `
        -RunId ([string]$manifestValue.run_id) `
        -ExpectedTrials $expectedTrials
    $telemetryComplete = $processTelemetryComplete -and [string]$finalObserverContext.status -eq "inactive"
    $preparedMediaValid = Test-PreparedMediaValid -ManifestValue $manifestValue
    $exportMediaValid = $true
    if ([string]$launchScenario.kind -eq "export") {
        $exportValidation = Invoke-ExportValidation `
            -ManifestValue $manifestValue `
            -Scenario $launchScenario `
            -BeforeSnapshot $beforeClipSnapshot `
            -EventsPath $eventsPath `
            -SentinelRoot $validated.sentinel_root `
            -LibraryRoot $validated.library_root `
            -FiniteTimeoutSeconds ([int][Math]::Min(7200, [Math]::Max(30, $finiteTimeout)))
        Write-JsonFile `
            -Path (Join-Path $validated.result_root "export-validation.json") `
            -Value $exportValidation
        $exportMediaValid = [bool]$exportValidation.all_valid
    }
    $mediaValid = $preparedMediaValid -and $exportMediaValid
    Update-TerminalForObserver `
        -Path (Join-Path $validated.result_root "terminal.json") `
        -RunId ([string]$manifestValue.run_id) `
        -EventCount $eventCount `
        -RequestCount $requestCount `
        -SampleCount $processSampleCount `
        -SourceHashesMatch ([bool]$postHashes.all_match) `
        -MediaValid $mediaValid `
        -TelemetryComplete $telemetryComplete
    if (-not $mediaValid) {
        Stop-Benchmark "MEDIA_INVALID" "Prepared fixture validation or newly created export validation failed; preserved evidence identifies the exact checks."
    }
    if (-not $telemetryComplete) { Stop-Benchmark "TELEMETRY_INCOMPLETE" "The active trial lacks two complete process samples or exceeds the telemetry-gap contract." }

    $analysis = Invoke-NativeTool `
        -FilePath $python `
        -Arguments @($analyzer, "--run-root", $validated.result_root) `
        -FiniteTimeoutSeconds ([Math]::Min(3600, $finiteTimeout)) `
        -Label "replay benchmark analyzer"
    Write-Utf8Text -Path (Join-Path $validated.result_root "analyzer.stdout.log") -Text ([string]$analysis.stdout)
    Write-Utf8Text -Path (Join-Path $validated.result_root "analyzer.stderr.log") -Text ([string]$analysis.stderr)
    if ($analysis.exit_code -ne 0) { Stop-Benchmark "ANALYZER" "Analyzer rejected the run with exit code $($analysis.exit_code)." }

    Write-JsonFile -Path (Join-Path $validated.result_root "runner-result.json") -Value ([ordered]@{
        schema_version = 1
        run_id = [string]$manifestValue.run_id
        status = "complete"
        completed_utc = [DateTime]::UtcNow.ToString("o")
        app_exit_code = $exitCode
        fixture_hashes_match = $true
        analyzer_exit_code = [int]$analysis.exit_code
    })
    Close-BenchmarkJob
    Write-Host "QB-REPLAY-RUN-OK: production app exited cleanly, fixture post-hashes matched, and analyzer accepted the preserved run."
    Write-Host "RESULT_ROOT=$($validated.result_root)"
}
catch {
    $failureRecord = $_
    $message = $_.Exception.Message
    if ($script:BenchmarkJob -ne [IntPtr]::Zero) {
        try { Stop-CreatedProcessTree -Reason "runner-failure" } catch {}
    }
    Complete-BenchmarkOutput
    Close-BenchmarkJob
    if (-not [string]::IsNullOrWhiteSpace($script:ResultRootCreated) -and (Test-Path -LiteralPath $script:ResultRootCreated -PathType Container)) {
        try {
            Write-JsonFile -Path (Join-Path $script:ResultRootCreated "runner-error.json") -Value ([ordered]@{
                schema_version = 1
                status = "failed"
                failed_utc = [DateTime]::UtcNow.ToString("o")
                error = $message
                error_type = $failureRecord.Exception.GetType().FullName
                script_stack_trace = [string]$failureRecord.ScriptStackTrace
                preserved = $true
            })
        }
        catch {}
    }
    [Console]::Error.WriteLine($message)
    [Console]::Error.WriteLine("The runner preserves every created run artifact and performs no automatic cleanup.")
    exit 2
}
