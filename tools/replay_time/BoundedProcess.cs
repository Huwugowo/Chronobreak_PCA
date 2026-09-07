using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

namespace Chronobreak.ReplayTime
{
    public sealed class BoundedProcessResult
    {
        public int ExitCode { get; internal set; }
        public byte[] StandardOutput { get; internal set; }
        public byte[] StandardError { get; internal set; }
        public bool TimedOut { get; internal set; }
        public bool OutputOverflow { get; internal set; }
        public long DurationMilliseconds { get; internal set; }
    }

    public static class BoundedProcess
    {
        private const uint CREATE_SUSPENDED = 0x00000004;
        private const uint CREATE_NO_WINDOW = 0x08000000;
        private const uint STARTF_USESTDHANDLES = 0x00000100;
        private const uint HANDLE_FLAG_INHERIT = 0x00000001;
        private const uint CREATE_UNICODE_ENVIRONMENT = 0x00000400;
        private const uint JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = 0x00002000;
        private const uint WAIT_OBJECT_0 = 0;
        private const uint WAIT_TIMEOUT = 258;
        private const uint INFINITE = 0xffffffff;
        private const int JobObjectExtendedLimitInformation = 9;
        private const uint GENERIC_READ = 0x80000000;
        private const uint FILE_SHARE_READ = 1;
        private const uint FILE_SHARE_WRITE = 2;
        private const uint OPEN_EXISTING = 3;
        private const uint FILE_ATTRIBUTE_NORMAL = 0x80;

        [StructLayout(LayoutKind.Sequential)]
        private struct SECURITY_ATTRIBUTES
        {
            public int nLength;
            public IntPtr lpSecurityDescriptor;
            [MarshalAs(UnmanagedType.Bool)] public bool bInheritHandle;
        }

        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        private struct STARTUPINFO
        {
            public int cb;
            public string lpReserved;
            public string lpDesktop;
            public string lpTitle;
            public int dwX;
            public int dwY;
            public int dwXSize;
            public int dwYSize;
            public int dwXCountChars;
            public int dwYCountChars;
            public int dwFillAttribute;
            public uint dwFlags;
            public short wShowWindow;
            public short cbReserved2;
            public IntPtr lpReserved2;
            public IntPtr hStdInput;
            public IntPtr hStdOutput;
            public IntPtr hStdError;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct PROCESS_INFORMATION
        {
            public IntPtr hProcess;
            public IntPtr hThread;
            public uint dwProcessId;
            public uint dwThreadId;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct JOBOBJECT_BASIC_LIMIT_INFORMATION
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
        private struct IO_COUNTERS
        {
            public ulong ReadOperationCount;
            public ulong WriteOperationCount;
            public ulong OtherOperationCount;
            public ulong ReadTransferCount;
            public ulong WriteTransferCount;
            public ulong OtherTransferCount;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION
        {
            public JOBOBJECT_BASIC_LIMIT_INFORMATION BasicLimitInformation;
            public IO_COUNTERS IoInfo;
            public UIntPtr ProcessMemoryLimit;
            public UIntPtr JobMemoryLimit;
            public UIntPtr PeakProcessMemoryUsed;
            public UIntPtr PeakJobMemoryUsed;
        }

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool CreatePipe(out IntPtr read, out IntPtr write, ref SECURITY_ATTRIBUTES attributes, int size);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool SetHandleInformation(IntPtr handle, uint mask, uint flags);

        [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        private static extern bool CreateProcessW(string applicationName, StringBuilder commandLine,
            IntPtr processAttributes, IntPtr threadAttributes, bool inheritHandles, uint creationFlags,
            IntPtr environment, string currentDirectory, ref STARTUPINFO startupInfo,
            out PROCESS_INFORMATION processInformation);

        [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        private static extern IntPtr CreateJobObject(IntPtr attributes, string name);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool SetInformationJobObject(IntPtr job, int infoClass, IntPtr info, uint length);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern uint ResumeThread(IntPtr thread);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool TerminateJobObject(IntPtr job, uint exitCode);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern uint WaitForSingleObject(IntPtr handle, uint milliseconds);
        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool TerminateProcess(IntPtr process, uint exitCode);


        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool GetExitCodeProcess(IntPtr process, out uint exitCode);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool CloseHandle(IntPtr handle);

        [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        private static extern IntPtr CreateFileW(string name, uint access, uint share,
            ref SECURITY_ATTRIBUTES security, uint creation, uint flags, IntPtr template);

        private sealed class CaptureState
        {
            internal readonly object Gate = new object();
            internal readonly MemoryStream StandardOutput = new MemoryStream();
            internal readonly MemoryStream StandardError = new MemoryStream();
            internal readonly long Limit;
            internal long Seen;
            internal bool Overflow;
            internal Exception ReadError;
            internal IntPtr Job;

            internal CaptureState(long limit, IntPtr job)
            {
                Limit = limit;
                Job = job;
            }
        }
        public static BoundedProcessResult Run(string executable, string[] arguments,
            string workingDirectory, int timeoutMilliseconds, int outputLimitBytes,
            IDictionary<string, string> environment)
        {
            if (String.IsNullOrWhiteSpace(executable) || !Path.IsPathRooted(executable))
                throw new ArgumentException("Executable must be an absolute path.", "executable");
            if (arguments == null) throw new ArgumentNullException("arguments");
            if (String.IsNullOrWhiteSpace(workingDirectory) || !Path.IsPathRooted(workingDirectory))
                throw new ArgumentException("Working directory must be absolute.", "workingDirectory");
            if (timeoutMilliseconds < 1) throw new ArgumentOutOfRangeException("timeoutMilliseconds");
            if (outputLimitBytes < 1024) throw new ArgumentOutOfRangeException("outputLimitBytes");
            if (environment == null) throw new ArgumentNullException("environment");
            IntPtr stdoutRead = IntPtr.Zero, stdoutWrite = IntPtr.Zero;
            IntPtr stderrRead = IntPtr.Zero, stderrWrite = IntPtr.Zero;
            IntPtr nullInput = new IntPtr(-1), job = IntPtr.Zero;
            IntPtr environmentPointer = IntPtr.Zero;
            PROCESS_INFORMATION process = new PROCESS_INFORMATION();
            Thread stdoutThread = null, stderrThread = null;
            CaptureState capture = null;
            DateTime started = DateTime.UtcNow;
            bool processCreated = false;
            bool jobOwnsProcess = false;
            bool timedOut = false;
            try
            {
                SECURITY_ATTRIBUTES security = new SECURITY_ATTRIBUTES();
                security.nLength = Marshal.SizeOf(typeof(SECURITY_ATTRIBUTES));
                security.bInheritHandle = true;
                if (!CreatePipe(out stdoutRead, out stdoutWrite, ref security, 0) ||
                    !CreatePipe(out stderrRead, out stderrWrite, ref security, 0))
                    ThrowLast("Could not create bounded-process output pipes");
                if (!SetHandleInformation(stdoutRead, HANDLE_FLAG_INHERIT, 0) ||
                    !SetHandleInformation(stderrRead, HANDLE_FLAG_INHERIT, 0))
                    ThrowLast("Could not make bounded-process read pipes private");
                nullInput = CreateFileW("NUL", GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE,
                    ref security, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, IntPtr.Zero);
                if (nullInput == new IntPtr(-1)) ThrowLast("Could not open bounded-process null input");

                job = CreateJobObject(IntPtr.Zero, null);
                if (job == IntPtr.Zero) ThrowLast("Could not create bounded-process job object");
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits = new JOBOBJECT_EXTENDED_LIMIT_INFORMATION();
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                int limitSize = Marshal.SizeOf(typeof(JOBOBJECT_EXTENDED_LIMIT_INFORMATION));
                IntPtr limitPointer = Marshal.AllocHGlobal(limitSize);
                try
                {
                    Marshal.StructureToPtr(limits, limitPointer, false);
                    if (!SetInformationJobObject(job, JobObjectExtendedLimitInformation,
                        limitPointer, (uint)limitSize))
                        ThrowLast("Could not configure bounded-process job object");
                }
                finally { Marshal.FreeHGlobal(limitPointer); }

                STARTUPINFO startup = new STARTUPINFO();
                startup.cb = Marshal.SizeOf(typeof(STARTUPINFO));
                startup.dwFlags = STARTF_USESTDHANDLES;
                startup.hStdInput = nullInput;
                startup.hStdOutput = stdoutWrite;
                startup.hStdError = stderrWrite;
                StringBuilder commandLine = new StringBuilder(BuildCommandLine(executable, arguments));
                environmentPointer = Marshal.StringToHGlobalUni(BuildEnvironmentBlock(environment));
                if (!CreateProcessW(executable, commandLine, IntPtr.Zero, IntPtr.Zero, true,
                    CREATE_SUSPENDED | CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT,
                    environmentPointer, workingDirectory,
                    ref startup, out process))
                    ThrowLast("Could not create bounded child process");
                processCreated = true;
                if (!AssignProcessToJobObject(job, process.hProcess))
                    ThrowLast("Could not contain bounded child process in its job");
                jobOwnsProcess = true;

                CloseHandle(stdoutWrite); stdoutWrite = IntPtr.Zero;
                CloseHandle(stderrWrite); stderrWrite = IntPtr.Zero;
                CloseHandle(nullInput); nullInput = new IntPtr(-1);

                capture = new CaptureState(outputLimitBytes, job);
                stdoutThread = StartReader(stdoutRead, capture.StandardOutput, capture);
                stdoutRead = IntPtr.Zero;
                stderrThread = StartReader(stderrRead, capture.StandardError, capture);
                stderrRead = IntPtr.Zero;
                if (ResumeThread(process.hThread) == 0xffffffff) ThrowLast("Could not resume bounded child process");
                CloseHandle(process.hThread); process.hThread = IntPtr.Zero;

                DateTime deadline = started.AddMilliseconds(timeoutMilliseconds);
                while (true)
                {
                    uint wait = WaitForSingleObject(process.hProcess, 50);
                    if (wait == WAIT_OBJECT_0) break;
                    if (wait != WAIT_TIMEOUT) ThrowLast("Could not wait for bounded child process");
                    lock (capture.Gate)
                    {
                        if (capture.Overflow) break;
                    }
                    if (DateTime.UtcNow >= deadline)
                    {
                        timedOut = true;
                        TerminateJobObject(job, 0xeeee0001);
                        break;
                    }
                }
                bool overflowBeforeDrain;
                lock (capture.Gate) { overflowBeforeDrain = capture.Overflow; }
                if (overflowBeforeDrain) TerminateJobObject(job, 0xeeee0002);
                if (timedOut || overflowBeforeDrain)
                {
                    if (WaitForSingleObject(process.hProcess, 5000) != WAIT_OBJECT_0)
                        throw new InvalidOperationException("Contained process tree did not terminate within five seconds.");
                }
                if (stdoutThread != null && !stdoutThread.Join(5000))
                    throw new InvalidOperationException("Bounded stdout pipe did not close.");
                if (stderrThread != null && !stderrThread.Join(5000))
                    throw new InvalidOperationException("Bounded stderr pipe did not close.");
                Exception readError;
                lock (capture.Gate) { readError = capture.ReadError; }
                if (readError != null)
                    throw new IOException("Bounded process output capture failed.", readError);
                bool overflow;
                lock (capture.Gate) { overflow = capture.Overflow; }
                uint rawExit;
                if (!GetExitCodeProcess(process.hProcess, out rawExit)) ThrowLast("Could not read bounded child exit code");
                return new BoundedProcessResult {
                    ExitCode = unchecked((int)rawExit),
                    StandardOutput = capture.StandardOutput.ToArray(),
                    StandardError = capture.StandardError.ToArray(),
                    TimedOut = timedOut,
                    OutputOverflow = overflow,
                    DurationMilliseconds = Math.Max(0L, (long)(DateTime.UtcNow - started).TotalMilliseconds)
                };
            }
            finally
            {
                if (processCreated && !jobOwnsProcess && process.hProcess != IntPtr.Zero)
                {
                    try { TerminateProcess(process.hProcess, 0xeeee0003); } catch { }
                    WaitForSingleObject(process.hProcess, 5000);
                }
                if (job != IntPtr.Zero) CloseHandle(job);
                if (stdoutThread != null && stdoutThread.IsAlive) stdoutThread.Join(5000);
                if (stderrThread != null && stderrThread.IsAlive) stderrThread.Join(5000);
                if (process.hThread != IntPtr.Zero) CloseHandle(process.hThread);
                if (process.hProcess != IntPtr.Zero) CloseHandle(process.hProcess);
                if (stdoutRead != IntPtr.Zero) CloseHandle(stdoutRead);
                if (stdoutWrite != IntPtr.Zero) CloseHandle(stdoutWrite);
                if (stderrRead != IntPtr.Zero) CloseHandle(stderrRead);
                if (stderrWrite != IntPtr.Zero) CloseHandle(stderrWrite);
                if (nullInput != new IntPtr(-1)) CloseHandle(nullInput);
                if (environmentPointer != IntPtr.Zero) Marshal.FreeHGlobal(environmentPointer);
                if (capture != null)
                {
                    capture.StandardOutput.Dispose();
                    capture.StandardError.Dispose();
                }
            }
        }

        private static string BuildEnvironmentBlock(IDictionary<string, string> environment)
        {
            List<string> keys = new List<string>(environment.Keys);
            keys.Sort(StringComparer.OrdinalIgnoreCase);
            StringBuilder block = new StringBuilder();
            foreach (string key in keys)
            {
                string value = environment[key];
                if (String.IsNullOrEmpty(key) || key.IndexOf('=') >= 0 ||
                    key.IndexOf('\0') >= 0 || value == null || value.IndexOf('\0') >= 0)
                    throw new ArgumentException("Child environment contains an invalid entry.", "environment");
                block.Append(key).Append('=').Append(value).Append('\0');
            }
            block.Append('\0');
            return block.ToString();
        }

        private static Thread StartReader(IntPtr handle, MemoryStream destination, CaptureState capture)
        {
            Microsoft.Win32.SafeHandles.SafeFileHandle safe =
                new Microsoft.Win32.SafeHandles.SafeFileHandle(handle, true);
            FileStream stream = new FileStream(safe, FileAccess.Read, 4096, false);
            Thread thread = new Thread(delegate()
            {
                using (stream)
                {
                    byte[] buffer = new byte[8192];
                    while (true)
                    {
                        int count;
                        try { count = stream.Read(buffer, 0, buffer.Length); }
                        catch (Exception error)
                        {
                            lock (capture.Gate)
                            {
                                if (capture.ReadError == null) capture.ReadError = error;
                            }
                            break;
                        }
                        if (count == 0) break;
                        lock (capture.Gate)
                        {
                            long room = capture.Limit - capture.Seen;
                            int retained = room > 0 ? (int)Math.Min((long)count, room) : 0;
                            if (retained > 0) destination.Write(buffer, 0, retained);
                            capture.Seen += count;
                            if (capture.Seen > capture.Limit && !capture.Overflow)
                            {
                                capture.Overflow = true;
                                TerminateJobObject(capture.Job, 0xeeee0002);
                            }
                        }
                    }
                }
            });
            thread.IsBackground = true;
            thread.Name = "qb-replay-012-output-drain";
            thread.Start();
            return thread;
        }

        private static string BuildCommandLine(string executable, string[] arguments)
        {
            StringBuilder result = new StringBuilder();
            AppendArgument(result, executable);
            foreach (string argument in arguments)
            {
                result.Append(' ');
                AppendArgument(result, argument ?? String.Empty);
            }
            return result.ToString();
        }

        private static void AppendArgument(StringBuilder result, string value)
        {
            bool quote = value.Length == 0 || value.IndexOfAny(new char[] { ' ', '\t', '\n', '\v', '"' }) >= 0;
            if (!quote) { result.Append(value); return; }
            result.Append('"');
            int slashes = 0;
            foreach (char character in value)
            {
                if (character == '\\') { slashes++; continue; }
                if (character == '"')
                {
                    result.Append('\\', slashes * 2 + 1);
                    result.Append('"');
                    slashes = 0;
                    continue;
                }
                if (slashes > 0) { result.Append('\\', slashes); slashes = 0; }
                result.Append(character);
            }
            if (slashes > 0) result.Append('\\', slashes * 2);
            result.Append('"');
        }

        private static void ThrowLast(string message)
        {
            throw new Win32Exception(Marshal.GetLastWin32Error(), message);
        }
    }
}
