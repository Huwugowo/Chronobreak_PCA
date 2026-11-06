using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;
using System.Web.Script.Serialization;

namespace Chronobreak.AudioProbe
{
    [StructLayout(LayoutKind.Sequential, Pack = 2)]
    internal struct WaveFormat
    {
        public ushort FormatTag, Channels;
        public uint SamplesPerSec, AvgBytesPerSec;
        public ushort BlockAlign, BitsPerSample, ExtraSize;
    }

    [ComImport, Guid("1CB9AD4C-DBFA-4C32-B178-C2F568A703B2")]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    internal interface IAudioClient
    {
        [PreserveSig] int Initialize(int shareMode, uint flags, long duration,
            long periodicity, ref WaveFormat format, IntPtr sessionGuid);
        [PreserveSig] int GetBufferSize(out uint frames);
        [PreserveSig] int GetStreamLatency(out long latency);
        [PreserveSig] int GetCurrentPadding(out uint frames);
        [PreserveSig] int IsFormatSupported(int shareMode, ref WaveFormat format, out IntPtr closest);
        [PreserveSig] int GetMixFormat(out IntPtr format);
        [PreserveSig] int GetDevicePeriod(out long defaultPeriod, out long minimumPeriod);
        [PreserveSig] int Start();
        [PreserveSig] int Stop();
        [PreserveSig] int Reset();
        [PreserveSig] int SetEventHandle(IntPtr handle);
        [PreserveSig] int GetService(ref Guid service, [MarshalAs(UnmanagedType.IUnknown)] out object result);
    }

    [ComImport, Guid("C8ADBD64-E71E-48A0-A4DE-185C395CD317")]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    internal interface IAudioCaptureClient
    {
        [PreserveSig] int GetBuffer(out IntPtr data, out uint frames, out uint flags,
            out ulong devicePosition, out ulong qpcPosition);
        [PreserveSig] int ReleaseBuffer(uint frames);
        [PreserveSig] int GetNextPacketSize(out uint frames);
    }

    internal sealed class CaptureGroup : IDisposable
    {
        private IAudioClient client;
        private IAudioCaptureClient capture;
        private readonly AutoResetEvent signal = new AutoResetEvent(false);
        private readonly int expectedFrequency, excludedFrequency;
        private readonly long maximumFrames;
        private long sampleCount;
        private double expectedReal, expectedImaginary, excludedReal, excludedImaginary;
        private ulong firstQpc, lastQpc, firstDevice, lastDevice;
        private long lastPacketStart;
        private uint packets, timestampErrors, discontinuities;
        private uint previousFrames;
        private ulong previousQpc;
        private readonly List<object> packetLedger = new List<object>();

        private static void Check(int result) { Marshal.ThrowExceptionForHR(result); }

        public CaptureGroup(uint target, int mode, int expected, int excluded, int maximumSeconds = 10)
        {
            if (maximumSeconds < 1 || maximumSeconds > 15) throw new ArgumentOutOfRangeException("maximumSeconds");
            maximumFrames = 48000L * maximumSeconds;
            expectedFrequency = expected;
            excludedFrequency = excluded;
            IntPtr blob = Marshal.AllocHGlobal(Marshal.SizeOf(typeof(ActivationParams)));
            var completion = new Completion(true);
            IActivateAudioInterfaceAsyncOperation operation = null;
            try
            {
                var parameters = new ActivationParams {
                    ActivationType = 1, TargetProcessId = target, ProcessLoopbackMode = mode
                };
                Marshal.StructureToPtr(parameters, blob, false);
                var variant = new PropVariant {
                    Type = 65, BlobSize = (uint)Marshal.SizeOf(parameters), Blob = blob
                };
                var iid = typeof(IAudioClient).GUID;
                Check(Program.ActivateAudioInterfaceAsync("VAD\\Process_Loopback",
                    ref iid, ref variant, completion, out operation));
                if (!completion.Finished.WaitOne(5000))
                {
                    Console.Error.WriteLine("Controlled activation exceeded five seconds.");
                    Environment.Exit(3);
                }
                Check(completion.ResultCall);
                Check(completion.Activation);
                if (completion.Client == IntPtr.Zero) throw new InvalidOperationException("No audio client.");
                client = (IAudioClient)Marshal.GetObjectForIUnknown(completion.Client);
            }
            finally
            {
                if (completion.Client != IntPtr.Zero) Marshal.Release(completion.Client);
                if (operation != null) Marshal.ReleaseComObject(operation);
                GC.KeepAlive(completion);
                completion.Finished.Dispose();
                Marshal.FreeHGlobal(blob);
            }
            try
            {
                var format = new WaveFormat {
                    FormatTag = 1, Channels = 2, SamplesPerSec = 48000,
                    AvgBytesPerSec = 192000, BlockAlign = 4, BitsPerSample = 16
                };
                Check(client.Initialize(0, 0x80060000, 0, 0, ref format, IntPtr.Zero));
                Check(client.SetEventHandle(signal.SafeWaitHandle.DangerousGetHandle()));
                var iid = typeof(IAudioCaptureClient).GUID;
                object service;
                Check(client.GetService(ref iid, out service));
                capture = (IAudioCaptureClient)service;
                Check(client.Start());
            }
            catch { Dispose(); throw; }
        }

        public void Drain()
        {
            uint pending;
            Check(capture.GetNextPacketSize(out pending));
            // Six seconds of stereo PCM is bounded; never write captured samples.
            while (pending > 0)
            {
                IntPtr data;
                uint frames, flags;
                ulong device, qpc;
                Check(capture.GetBuffer(out data, out frames, out flags, out device, out qpc));
                try
                {
                    if (frames > 48000 || sampleCount + frames > maximumFrames)
                        throw new InvalidOperationException("Unexpected capture buffer/coverage.");
                    if (packets == 0) { firstQpc = qpc; firstDevice = device; }
                    lastQpc = qpc; lastDevice = device; lastPacketStart = sampleCount;
                    packets++;
                    if ((flags & 4) != 0) timestampErrors++;
                    if ((flags & 1) != 0 && packets > 1) discontinuities++;
                    var pcm = new short[checked((int)frames * 2)];
                    if ((flags & 2) == 0) Marshal.Copy(data, pcm, 0, pcm.Length);
                    double peak = 0;
                    double packetExpectedReal = 0, packetExpectedImaginary = 0;
                    double packetExcludedReal = 0, packetExcludedImaginary = 0;
                    int? firstSignalFrame = null;
                    for (int i = 0; i < frames; i++, sampleCount++)
                    {
                        double sample = (pcm[i * 2] + pcm[i * 2 + 1]) / 65536.0;
                        peak = Math.Max(peak, Math.Abs(sample));
                        if (!firstSignalFrame.HasValue && Math.Abs(sample) >= 0.005) firstSignalFrame = i;
                        double expectedPhase = 2 * Math.PI * expectedFrequency * sampleCount / 48000.0;
                        double excludedPhase = 2 * Math.PI * excludedFrequency * sampleCount / 48000.0;
                        double er = sample * Math.Cos(expectedPhase), ei = sample * Math.Sin(expectedPhase);
                        double xr = sample * Math.Cos(excludedPhase), xi = sample * Math.Sin(excludedPhase);
                        expectedReal += er; expectedImaginary += ei;
                        excludedReal += xr; excludedImaginary += xi;
                        packetExpectedReal += er; packetExpectedImaginary += ei;
                        packetExcludedReal += xr; packetExcludedImaginary += xi;
                    }
                    packetLedger.Add(new {
                        qpc_100ns = qpc.ToString(), device_frame = device.ToString(),
                        cumulative_start_frame = lastPacketStart, frames = frames, flags = flags,
                        observed_qpc_100ns = Decimal.Truncate((decimal)Stopwatch.GetTimestamp() * 10000000 / Stopwatch.Frequency).ToString(),
                        previous_end_to_start_ms = packets == 1 ? (decimal?)null :
                            ((decimal)qpc - previousQpc - previousFrames * 10000000m / 48000) / 10000,
                        peak = peak, first_signal_frame = firstSignalFrame,
                        expected_amplitude = 2 * Math.Sqrt(packetExpectedReal * packetExpectedReal + packetExpectedImaginary * packetExpectedImaginary) / frames,
                        excluded_amplitude = 2 * Math.Sqrt(packetExcludedReal * packetExcludedReal + packetExcludedImaginary * packetExcludedImaginary) / frames
                    });
                    previousQpc = qpc; previousFrames = frames;
                }
                finally { Check(capture.ReleaseBuffer(frames)); }
                Check(capture.GetNextPacketSize(out pending));
            }
        }

        public object Result(out bool passed)
        {
            double expected = sampleCount == 0 ? 0 : 2 * Math.Sqrt(
                expectedReal * expectedReal + expectedImaginary * expectedImaginary) / sampleCount;
            double excluded = sampleCount == 0 ? 0 : 2 * Math.Sqrt(
                excludedReal * excludedReal + excludedImaginary * excludedImaginary) / sampleCount;
            double ratio = expected == 0 ? Double.MaxValue : excluded / expected;
            bool devicePositionAdvanced = lastDevice > firstDevice;
            double? deviceClockErrorMs = !devicePositionAdvanced || firstQpc == 0 || lastQpc < firstQpc ?
                (double?)null : (lastQpc - firstQpc) / 10000.0 -
                (lastDevice - firstDevice) * 1000.0 / 48000.0;
            double? qpcVsSampleElapsedMs = firstQpc == 0 || lastQpc < firstQpc ? (double?)null :
                (lastQpc - firstQpc) / 10000.0 - lastPacketStart * 1000.0 / 48000.0;
            bool swappedMappingWouldPass = excluded >= 0.005 && excluded > 0 && expected / excluded <= 0.05;
            passed = expected >= 0.005 && ratio <= 0.05 && sampleCount >= 48000 &&
                timestampErrors == 0 && discontinuities == 0 && !swappedMappingWouldPass;
            return new {
                expected_hz = expectedFrequency, excluded_hz = excludedFrequency,
                expected_amplitude = expected, excluded_amplitude = excluded,
                excluded_to_expected_ratio = ratio,
                captured_frames = sampleCount, packets = packets,
                first_qpc_100ns = firstQpc.ToString(), last_qpc_100ns = lastQpc.ToString(),
                first_device_frame = firstDevice.ToString(), last_device_frame = lastDevice.ToString(),
                device_position_advanced = devicePositionAdvanced,
                qpc_device_clock_error_ms = deviceClockErrorMs,
                qpc_vs_cumulative_samples_elapsed_ms = qpcVsSampleElapsedMs,
                mapping_negative_control_rejected = !swappedMappingWouldPass,
                timing_proven = false,
                packet_ledger = packetLedger,
                timestamp_errors = timestampErrors, discontinuities = discontinuities,
                passed = passed
            };
        }

        public void Dispose()
        {
            if (client != null) client.Stop();
            if (capture != null) { Marshal.ReleaseComObject(capture); capture = null; }
            if (client != null) { Marshal.ReleaseComObject(client); client = null; }
            signal.Dispose();
        }
    }

    internal static class ControlledAudioProof
    {
        [DllImport("winmm.dll", EntryPoint = "PlaySoundW", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool PlaySound(byte[] sound, IntPtr module, uint flags);

        internal static bool PlayWave(byte[] wave) { return PlaySound(wave, IntPtr.Zero, 0x00000006); }

        public static int RenderTone(int frequency)
        {
            if (frequency != 697 && frequency != 1301) return 2;
            byte[] wave = BuildWave(frequency, false);
            Thread.Sleep(1500);
            return PlayWave(wave) ? 0 : 2;
        }

        internal static byte[] BuildWave(int frequency, bool markers)
        {
            byte[] wave;
            using (var stream = new MemoryStream())
            using (var writer = new BinaryWriter(stream))
            {
                int frames = markers ? 48000 * 6 : 48000 * 5 / 2;
                writer.Write(System.Text.Encoding.ASCII.GetBytes("RIFF")); writer.Write(36 + frames * 4);
                writer.Write(System.Text.Encoding.ASCII.GetBytes("WAVEfmt ")); writer.Write(16);
                writer.Write((ushort)1); writer.Write((ushort)2); writer.Write(48000); writer.Write(192000);
                writer.Write((ushort)4); writer.Write((ushort)16);
                writer.Write(System.Text.Encoding.ASCII.GetBytes("data")); writer.Write(frames * 4);
                for (int i = 0; i < frames; i++)
                {
                    bool audible = !markers || (i >= 48000 && i < 60000) ||
                        (i >= 144000 && i < 156000) || (i >= 240000 && i < 252000);
                    short sample = audible ? (short)(1310 * Math.Sin(2 * Math.PI * frequency * i / 48000.0)) : (short)0;
                    writer.Write(sample); writer.Write(sample);
                }
                wave = stream.ToArray();
            }
            return wave;
        }

        private static Process StartTone(int frequency, bool childTree = false)
        {
            return Process.Start(new ProcessStartInfo {
                FileName = Process.GetCurrentProcess().MainModule.FileName,
                Arguments = (childTree ? "--tone-tree " : "--tone ") + frequency,
                UseShellExecute = false, CreateNoWindow = true
            });
        }

        public static int RenderToneTree(int frequency)
        {
            using (var child = StartTone(frequency))
            {
                try
                {
                    if (!child.WaitForExit(6000)) return 2;
                    return child.ExitCode;
                }
                finally
                {
                    if (!child.HasExited) { child.Kill(); child.WaitForExit(); }
                }
            }
        }

        public static int Run()
        {
            using (var league = StartTone(697, true))
            using (var other = StartTone(1301))
            {
                try
                {
                    using (var included = new CaptureGroup((uint)league.Id, 0, 697, 1301))
                    using (var excluded = new CaptureGroup((uint)league.Id, 1, 1301, 697))
                    {
                        var elapsed = Stopwatch.StartNew();
                        while (elapsed.ElapsedMilliseconds < 6000)
                        {
                            included.Drain(); excluded.Drain(); Thread.Sleep(5);
                        }
                        included.Drain(); excluded.Drain();
                        bool includePass, excludePass;
                        object include = included.Result(out includePass);
                        object exclude = excluded.Result(out excludePass);
                        if (!league.WaitForExit(1000) || !other.WaitForExit(1000))
                            throw new InvalidOperationException("Generated tone renderer did not exit.");
                        bool passed = includePass && excludePass && league.ExitCode == 0 && other.ExitCode == 0;
                        Console.WriteLine(new JavaScriptSerializer().Serialize(new {
                            scope = "two generated tones; spectral summaries only; no captured samples saved",
                            included_source = "child of target process", excluded_source = "independent process tree",
                            sample_rate = 48000, channels = 2, include = include, exclude = exclude,
                            separation_passed = passed, replay_synchronization_proven = false,
                            capture_video_alignment_proven = false
                        }));
                        return passed ? 0 : 1;
                    }
                }
                finally
                {
                    if (!league.HasExited) { league.Kill(); league.WaitForExit(); }
                    if (!other.HasExited) { other.Kill(); other.WaitForExit(); }
                }
            }
        }
    }
}
