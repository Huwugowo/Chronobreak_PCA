using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Threading;
using Microsoft.Win32.SafeHandles;

namespace Chronobreak.AudioProbe
{
    // Own the entire generated process tree, including on wrapper failure.
    public sealed class ProbeJob : IDisposable
    {
        [StructLayout(LayoutKind.Sequential)]
        private struct BasicLimits
        {
            public long ProcessUserTime, JobUserTime;
            public uint Flags;
            public UIntPtr MinimumWorkingSet, MaximumWorkingSet;
            public uint ActiveProcessLimit;
            public UIntPtr Affinity;
            public uint PriorityClass, SchedulingClass;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct IoCounters
        {
            public ulong ReadOperations, WriteOperations, OtherOperations;
            public ulong ReadBytes, WriteBytes, OtherBytes;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct ExtendedLimits
        {
            public BasicLimits Basic;
            public IoCounters Io;
            public UIntPtr ProcessMemory, JobMemory, PeakProcessMemory, PeakJobMemory;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct Accounting
        {
            public long UserTime, KernelTime, PeriodUserTime, PeriodKernelTime;
            public uint PageFaults, TotalProcesses, ActiveProcesses, TerminatedProcesses;
        }

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern SafeFileHandle CreateJobObject(IntPtr attributes, string name);
        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool SetInformationJobObject(SafeFileHandle job, int kind,
            ref ExtendedLimits limits, uint size);
        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool AssignProcessToJobObject(SafeFileHandle job, IntPtr process);
        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool TerminateJobObject(SafeFileHandle job, uint exitCode);
        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool QueryInformationJobObject(SafeFileHandle job, int kind,
            out Accounting accounting, uint size, IntPtr returnedLength);

        private readonly SafeFileHandle handle;

        public ProbeJob()
        {
            handle = CreateJobObject(IntPtr.Zero, null);
            if (handle.IsInvalid) throw new Win32Exception(Marshal.GetLastWin32Error());
            var limits = new ExtendedLimits();
            limits.Basic.Flags = 0x2000; // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            if (!SetInformationJobObject(handle, 9, ref limits, (uint)Marshal.SizeOf(limits)))
            {
                int error = Marshal.GetLastWin32Error();
                handle.Dispose();
                throw new Win32Exception(error);
            }
        }

        public void Assign(IntPtr process)
        {
            if (!AssignProcessToJobObject(handle, process))
                throw new Win32Exception(Marshal.GetLastWin32Error());
        }

        public void TerminateAndWait()
        {
            if (!TerminateJobObject(handle, 3))
                throw new Win32Exception(Marshal.GetLastWin32Error());
            var elapsed = Stopwatch.StartNew();
            do
            {
                Accounting accounting;
                if (!QueryInformationJobObject(handle, 1, out accounting,
                    (uint)Marshal.SizeOf(typeof(Accounting)), IntPtr.Zero))
                    throw new Win32Exception(Marshal.GetLastWin32Error());
                if (accounting.ActiveProcesses == 0) return;
                Thread.Sleep(25);
            } while (elapsed.ElapsedMilliseconds < 5000);
            throw new TimeoutException("Generated audio process tree did not terminate within five seconds.");
        }

        public void Dispose() { handle.Dispose(); }
    }
}
