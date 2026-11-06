using System;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;
using System.Web.Script.Serialization;
using System.Windows.Forms;

namespace Chronobreak.AudioProbe
{
    internal static class ControlledTimingProof
    {
        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool SetProcessDPIAware();

        private static long epoch;
        private static Form window;
        private static Exception windowError;

        public static int RenderMarkerTone(int frequency, string gateName)
        {
            if (frequency != 1301) return 2;
            byte[] wave = ControlledAudioProof.BuildWave(frequency, true);
            using (var gate = EventWaitHandle.OpenExisting(gateName))
            {
                Console.WriteLine("TIMING_TONE_READY"); Console.Out.Flush();
                if (!gate.WaitOne(10000)) return 3;
                return ControlledAudioProof.PlayWave(wave) ? 0 : 2;
            }
        }

        private static void ShowWindow(ManualResetEvent ready)
        {
            try
            {
                using (var form = new Form())
                using (var timer = new System.Windows.Forms.Timer())
                {
                    window = form;
                    form.Text = "Chronobreak generated audio timing fixture";
                    form.FormBorderStyle = FormBorderStyle.None;
                    form.AutoScaleMode = AutoScaleMode.None;
                    form.ClientSize = new Size(1920, 1080);
                    form.StartPosition = FormStartPosition.Manual;
                    form.Location = new Point(100, 100);
                    form.TopMost = true;
                    form.BackColor = Color.Black;
                    form.Paint += delegate(object sender, PaintEventArgs args) {
                        long start = Interlocked.Read(ref epoch);
                        double ms = start == 0 ? -1 : (Stopwatch.GetTimestamp() - start) * 1000.0 / Stopwatch.Frequency;
                        args.Graphics.Clear(Color.Black);
                        // Three fixed cells identify early/middle/late without OCR.
                        for (int marker = 0; marker < 3; marker++)
                        {
                            double from = 1000 + marker * 2000;
                            if (ms >= from && ms < from + 250)
                                args.Graphics.FillRectangle(Brushes.White, 20 + marker * 80, 20, 64, 64);
                        }
                        args.Graphics.DrawString("Generated fixture  " + ms.ToString("F0"),
                            SystemFonts.DefaultFont, Brushes.Gray, 330, 150);
                    };
                    form.Shown += delegate { ready.Set(); };
                    timer.Interval = 8;
                    timer.Tick += delegate { form.Invalidate(); };
                    timer.Start();
                    Application.Run(form);
                }
            }
            catch (Exception error) { windowError = error; ready.Set(); }
        }

        public static int RenderMarkerWindow(string gateName, string stopName)
        {
            if (!SetProcessDPIAware()) throw new InvalidOperationException("Could not establish fixture DPI awareness.");
            using (var gate = EventWaitHandle.OpenExisting(gateName))
            using (var stop = EventWaitHandle.OpenExisting(stopName))
            using (var ready = new ManualResetEvent(false))
            {
                var ui = new Thread(delegate() { ShowWindow(ready); });
                ui.SetApartmentState(ApartmentState.STA);
                ui.Start();
                try
                {
                    if (!ready.WaitOne(5000)) throw new TimeoutException("Generated window readiness deadline.");
                    if (windowError != null) throw windowError;
                    byte[] wave = ControlledAudioProof.BuildWave(697, true);
                    Console.WriteLine("TIMING_WINDOW_READY"); Console.Out.Flush();
                    if (!gate.WaitOne(10000)) return 3;
                    Interlocked.Exchange(ref epoch, Stopwatch.GetTimestamp());
                    Console.WriteLine("TIMING_WINDOW_EPOCH=" + Decimal.Truncate((decimal)Interlocked.Read(ref epoch) * 10000000 / Stopwatch.Frequency));
                    Console.Out.Flush();
                    if (!ControlledAudioProof.PlayWave(wave)) return 2;
                    if (!stop.WaitOne(10000)) return 3;
                    if (windowError != null) throw windowError;
                    return 0;
                }
                finally
                {
                    if (window != null && !window.IsDisposed) window.BeginInvoke(new Action(window.Close));
                    if (!ui.Join(3000)) Environment.Exit(3);
                }
            }
        }

        public static int Run(string nativeProbe, string outputRoot)
        {
            string gateName = "Local\\Chronobreak.AudioMarkers." + Guid.NewGuid().ToString("N");
            string stopName = "Local\\Chronobreak.AudioMarkersStop." + Guid.NewGuid().ToString("N");
            using (var gate = new EventWaitHandle(false, EventResetMode.ManualReset, gateName))
            using (var stop = new EventWaitHandle(false, EventResetMode.ManualReset, stopName))
            {
                Process league = null, other = null, captureVideo = null;
                try
                {
                    league = Process.Start(new ProcessStartInfo {
                        FileName = Process.GetCurrentProcess().MainModule.FileName,
                        Arguments = "--marker-window " + gateName + " " + stopName,
                        UseShellExecute = false, CreateNoWindow = true, RedirectStandardOutput = true
                    });
                    var windowReady = league.StandardOutput.ReadLineAsync();
                    if (!windowReady.Wait(5000) || windowReady.Result != "TIMING_WINDOW_READY")
                        throw new InvalidOperationException("Generated window did not become ready.");
                    var windowOutput = league.StandardOutput.ReadToEndAsync();
                    other = Process.Start(new ProcessStartInfo {
                        FileName = Process.GetCurrentProcess().MainModule.FileName,
                        Arguments = "--marker-tone 1301 " + gateName, UseShellExecute = false,
                        CreateNoWindow = true, RedirectStandardOutput = true
                    });
                    var toneReady = other.StandardOutput.ReadLineAsync();
                    if (!toneReady.Wait(5000) || toneReady.Result != "TIMING_TONE_READY")
                        throw new InvalidOperationException("Independent marker source did not become ready.");
                    using (var included = new CaptureGroup((uint)league.Id, 0, 697, 1301, 15))
                    using (var excluded = new CaptureGroup((uint)league.Id, 1, 1301, 697, 15))
                    {
                        captureVideo = Process.Start(new ProcessStartInfo {
                            FileName = nativeProbe, UseShellExecute = false, CreateNoWindow = true,
                            RedirectStandardOutput = true, RedirectStandardError = true,
                            Arguments = "--pid " + league.Id +
                                " --duration-seconds 8 --output \"" + Path.Combine(outputRoot, "wgc.h264") +
                                "\" --qpc-ledger \"" + Path.Combine(outputRoot, "wgc-qpc.jsonl") + "\""
                        });
                        var videoErrors = captureVideo.StandardError.ReadToEndAsync();
                        var videoReady = captureVideo.StandardOutput.ReadLineAsync();
                        if (!videoReady.Wait(8000) || videoReady.Result == null ||
                            !videoReady.Result.StartsWith("CHRONOBREAK_NATIVE_NVENC_STARTED"))
                        {
                            if (!captureVideo.HasExited) captureVideo.Kill();
                            captureVideo.WaitForExit(2000);
                            File.WriteAllText(Path.Combine(outputRoot, "native.stderr.log"), videoErrors.Result);
                            File.WriteAllText(Path.Combine(outputRoot, "native.stdout.log"),
                                videoReady.IsCompleted ? videoReady.Result : "readiness deadline");
                            throw new InvalidOperationException("Native video probe did not become ready.");
                        }
                        string firstLine = videoReady.Result;
                        var videoOutput = captureVideo.StandardOutput.ReadToEndAsync();
                        Interlocked.Exchange(ref epoch, Stopwatch.GetTimestamp());
                        gate.Set();
                        var elapsed = Stopwatch.StartNew();
                        while (elapsed.ElapsedMilliseconds < 8000)
                        {
                            included.Drain(); excluded.Drain(); Thread.Sleep(5);
                        }
                        included.Drain(); excluded.Drain();
                        if (!captureVideo.WaitForExit(3000)) throw new TimeoutException("Native video shutdown deadline.");
                        File.WriteAllText(Path.Combine(outputRoot, "native.stdout.log"), firstLine + "\n" + videoOutput.Result);
                        File.WriteAllText(Path.Combine(outputRoot, "native.stderr.log"), videoErrors.Result);
                        if (captureVideo.ExitCode != 0) throw new InvalidOperationException("Native video probe failed; inspect native logs.");
                        stop.Set();
                        if (!league.WaitForExit(1000) || league.ExitCode != 0 || !other.WaitForExit(1000) || other.ExitCode != 0)
                            throw new InvalidOperationException("Generated marker audio did not finish normally.");
                        string windowLog = windowOutput.Result;
                        File.WriteAllText(Path.Combine(outputRoot, "window.stdout.log"), windowLog);
                        long windowEpoch = 0;
                        const string epochPrefix = "TIMING_WINDOW_EPOCH=";
                        foreach (string line in windowLog.Split('\n'))
                        {
                            if (line.StartsWith(epochPrefix) &&
                                !Int64.TryParse(line.Substring(epochPrefix.Length).Trim(), out windowEpoch))
                                throw new InvalidOperationException("Invalid marker-window QPC epoch.");
                        }
                        if (windowEpoch <= 0) throw new InvalidOperationException("Missing marker-window QPC epoch.");
                        bool aggregateInclude, aggregateExclude;
                        var include = included.Result(out aggregateInclude);
                        var exclude = excluded.Result(out aggregateExclude);
                        Console.WriteLine(new JavaScriptSerializer().Serialize(new {
                            scope = "generated window + two pulsed tones; numeric audio ledger and generated-window H264 only",
                            sample_rate = 48000, channels = 2,
                            included_source = "generated window process with 697-Hz markers",
                            excluded_source = "sibling process with 1301-Hz markers",
                            fixture_epoch_qpc_100ns = windowEpoch.ToString(),
                            fixture_epoch_source = "marker window child; nominal waveform submission, not audio-device presentation",
                            coordinator_release_qpc_100ns = Decimal.Truncate((decimal)Interlocked.Read(ref epoch) * 10000000 / Stopwatch.Frequency).ToString(),
                            include = include, exclude = exclude,
                            fixture_completed = true, analysis_required = true,
                            marker_membership_asserted = false, capture_video_alignment_proven = false,
                            replay_synchronization_proven = false
                        }));
                        return 0;
                    }
                }
                finally
                {
                    // The wrapper's job contains every generated descendant even on forced timeout.
                    if (captureVideo != null) { if (!captureVideo.HasExited) { captureVideo.Kill(); captureVideo.WaitForExit(2000); } captureVideo.Dispose(); }
                    if (other != null) { if (!other.HasExited) { other.Kill(); other.WaitForExit(2000); } other.Dispose(); }
                    stop.Set();
                    if (league != null) { if (!league.HasExited) { league.Kill(); league.WaitForExit(2000); } league.Dispose(); }
                }
            }
        }
    }
}
