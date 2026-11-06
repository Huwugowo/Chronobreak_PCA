using System;
using System.Diagnostics;
using System.Threading;

namespace Chronobreak.AudioProbe
{
    internal static class ProbeLauncher
    {
        // The wrapper assigns this waiting process to its job before allowing any child.
        public static int Main(string[] args)
        {
            if (args.Length != 2) return 2;
            using (var gate = EventWaitHandle.OpenExisting(args[0]))
                if (!gate.WaitOne(5000)) return 3;
            using (var child = Process.Start(new ProcessStartInfo {
                FileName = args[1], UseShellExecute = false,
                WindowStyle = ProcessWindowStyle.Normal
            }))
            {
                child.WaitForExit();
                return child.ExitCode;
            }
        }
    }
}
