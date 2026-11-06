using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Threading;
using System.Web.Script.Serialization;

// Activation-only feasibility probe. It never initializes/starts an audio client,
// reads audio samples, opens League, or changes any endpoint/session settings.
namespace Chronobreak.AudioProbe
{
    [ComImport, Guid("72A22D78-CDE4-431D-B8CC-843A71199B6D")]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    public interface IActivateAudioInterfaceAsyncOperation
    {
        [PreserveSig]
        int GetActivateResult(out int activateResult, out IntPtr activatedInterface);
    }

    [ComVisible(true), Guid("41D949AB-9862-444A-80F6-C261334DA5EB")]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    public interface IActivateAudioInterfaceCompletionHandler
    {
        [PreserveSig]
        int ActivateCompleted(IActivateAudioInterfaceAsyncOperation operation);
    }

    [ComVisible(true), Guid("94EA2B94-E9CC-49E0-C0FF-EE64CA8F5B90")]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    public interface IAgileObject { }

    [ComVisible(true), ClassInterface(ClassInterfaceType.None)]
    public sealed class Completion : IActivateAudioInterfaceCompletionHandler, IAgileObject
    {
        // A plain managed object uses the CLR free-threaded marshaler. Do not
        // derive from StandardOleMarshalObject (which substitutes STA marshaling).
        public readonly ManualResetEvent Finished = new ManualResetEvent(false);
        public int ResultCall = unchecked((int)0x8000FFFF);
        public int Activation = unchecked((int)0x8000FFFF);
        public bool HasClient;
        public IntPtr Client;
        private readonly bool retainClient;

        public Completion(bool retainClient = false) { this.retainClient = retainClient; }

        public int ActivateCompleted(IActivateAudioInterfaceAsyncOperation operation)
        {
            IntPtr client = IntPtr.Zero;
            try
            {
                ResultCall = operation.GetActivateResult(out Activation, out client);
                HasClient = client != IntPtr.Zero;
                if (retainClient) { Client = client; client = IntPtr.Zero; }
            }
            catch (Exception error)
            {
                ResultCall = Marshal.GetHRForException(error);
            }
            finally
            {
                if (client != IntPtr.Zero) Marshal.Release(client);
                Finished.Set();
            }
            return 0;
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct ActivationParams
    {
        public int ActivationType;
        public uint TargetProcessId;
        public int ProcessLoopbackMode;
    }

    // Native x64 PROPVARIANT: VT_BLOB at offset 0, BLOB at offset 8.
    [StructLayout(LayoutKind.Explicit, Size = 24)]
    internal struct PropVariant
    {
        [FieldOffset(0)] public ushort Type;
        [FieldOffset(8)] public uint BlobSize;
        [FieldOffset(16)] public IntPtr Blob;
    }

    internal static class Program
    {
        [DllImport("Mmdevapi.dll", ExactSpelling = true, CharSet = CharSet.Unicode)]
        internal static extern int ActivateAudioInterfaceAsync(
            string devicePath, ref Guid interfaceId, ref PropVariant activationParams,
            IActivateAudioInterfaceCompletionHandler completion,
            out IActivateAudioInterfaceAsyncOperation operation);

        private static string Hex(int value)
        {
            return "0x" + unchecked((uint)value).ToString("X8");
        }

        private static object Probe(int mode)
        {
            var parameters = new ActivationParams {
                ActivationType = 1,
                TargetProcessId = (uint)Process.GetCurrentProcess().Id,
                ProcessLoopbackMode = mode
            };
            IntPtr blob = Marshal.AllocHGlobal(Marshal.SizeOf(parameters));
            IActivateAudioInterfaceAsyncOperation operation = null;
            var completion = new Completion();
            try
            {
                Marshal.StructureToPtr(parameters, blob, false);
                var variant = new PropVariant {
                    Type = 65, // VT_BLOB
                    BlobSize = (uint)Marshal.SizeOf(parameters),
                    Blob = blob
                };
                var audioClient = new Guid("1CB9AD4C-DBFA-4C32-B178-C2F568A703B2");
                int call = ActivateAudioInterfaceAsync("VAD\\Process_Loopback",
                    ref audioClient, ref variant, completion, out operation);
                if (call >= 0 && !completion.Finished.WaitOne(5000))
                {
                    Console.Error.WriteLine("Audio activation callback exceeded five seconds.");
                    // The API offers no cancellation. End this dedicated process
                    // rather than freeing memory still owned by an async request.
                    Environment.Exit(3);
                }
                bool available = call >= 0 && completion.ResultCall >= 0 &&
                    completion.Activation >= 0 && completion.HasClient;
                return new {
                    mode = mode == 0 ? "include-target-tree" : "exclude-target-tree",
                    call_hresult = Hex(call),
                    result_call_hresult = call >= 0 ? Hex(completion.ResultCall) : null,
                    activation_hresult = call >= 0 && completion.ResultCall >= 0 ?
                        Hex(completion.Activation) : null,
                    client_returned = completion.HasClient,
                    activation_available = available
                };
            }
            finally
            {
                if (operation != null) Marshal.ReleaseComObject(operation);
                GC.KeepAlive(completion);
                completion.Finished.Dispose();
                Marshal.FreeHGlobal(blob);
            }
        }

        [MTAThread]
        public static int Main(string[] args)
        {
            if (IntPtr.Size != 8)
            {
                Console.Error.WriteLine("Run this probe from 64-bit Windows PowerShell.");
                return 2;
            }
            try
            {
                // The wrapper assigns this process to its kill-on-close Job
                // before releasing this gate. No renderer can escape that job.
                if (args.Length >= 2 && args[0] == "--start-event")
                {
                    using (var gate = EventWaitHandle.OpenExisting(args[1]))
                        if (!gate.WaitOne(5000)) return 3;
                    var remaining = new string[args.Length - 2];
                    Array.Copy(args, 2, remaining, 0, remaining.Length);
                    args = remaining;
                }
                if (args.Length == 2 && args[0] == "--tone")
                    return ControlledAudioProof.RenderTone(Int32.Parse(args[1]));
                if (args.Length == 2 && args[0] == "--tone-tree")
                    return ControlledAudioProof.RenderToneTree(Int32.Parse(args[1]));
                if (args.Length == 1 && args[0] == "--controlled-proof")
                    return ControlledAudioProof.Run();
                if (args.Length == 3 && args[0] == "--marker-tone")
                    return ControlledTimingProof.RenderMarkerTone(Int32.Parse(args[1]), args[2]);
                if (args.Length == 3 && args[0] == "--marker-window")
                    return ControlledTimingProof.RenderMarkerWindow(args[1], args[2]);
                if (args.Length == 3 && args[0] == "--timing-proof")
                    return ControlledTimingProof.Run(args[1], args[2]);
                var results = new List<object> { Probe(0), Probe(1) };
                Console.WriteLine(new JavaScriptSerializer().Serialize(results));
                return 0;
            }
            catch (Exception error)
            {
                Console.Error.WriteLine(error.GetType().Name + ": " + error.Message);
                return 2;
            }
        }
    }
}
