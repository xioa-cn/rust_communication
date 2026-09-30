#if NETFRAMEWORK
#define PLC_PINNED_BUFFERS
#elif NETSTANDARD
#if NETSTANDARD2_1 || NETSTANDARD2_1_OR_GREATER
#define PLC_SPAN_BUFFERS
#else
#define PLC_PINNED_BUFFERS
#endif
#elif NETCOREAPP
#if NETCOREAPP3_0 || NETCOREAPP3_1 || NET5_0 || NET5_0_OR_GREATER || NETCOREAPP3_0_OR_GREATER
#define PLC_SPAN_BUFFERS
#else
#define PLC_PINNED_BUFFERS
#endif
#else
#define PLC_PINNED_BUFFERS
#endif

using System;
using System.Runtime.InteropServices;

namespace RsCommunication
{
    public enum CipVendor : uint { Omron = 1, Melsec = 2, Inovance = 3 }

    public sealed class CipOptions : PlcConnectionOptions
    {
        public bool? Connected { get; set; }
        public int? ConnectionSize { get; set; }
        public byte[] Route { get; set; } = Array.Empty<byte>();
        public uint PacketIntervalUs { get; set; } = 2_000_000;
        public byte TimeoutMultiplier { get; set; } = 2;
        public ushort OriginatorVendorId { get; set; }
        public uint? OriginatorSerial { get; set; }

        public CipOptions() : base(44818) { }

        internal PlcCipOptions Configure(CipVendor vendor)
        {
            PlcOptions common = NativeOptions((PlcProtocol)((uint)vendor + 15));
            PlcCipOptions options = CipNative.Options(vendor);
            bool connected = Connected ?? vendor == CipVendor.Inovance;
            int size = ConnectionSize ?? (connected ? 1996 : 500);
            if (size < 128 || size > (connected ? 4000 : 504))
                throw new ArgumentOutOfRangeException(nameof(ConnectionSize));
            if (Route == null || Route.Length > 500 || (Route.Length & 1) != 0)
                throw new ArgumentException("Route must be an even EPATH of at most 500 bytes.", nameof(Route));
            if (PacketIntervalUs == 0) throw new ArgumentOutOfRangeException(nameof(PacketIntervalUs));
            if (TimeoutMultiplier > 7) throw new ArgumentOutOfRangeException(nameof(TimeoutMultiplier));
            options.Port = common.Port;
            options.ConnectTimeoutMs = common.ConnectTimeoutMs;
            options.ReceiveTimeoutMs = common.ReceiveTimeoutMs;
            options.Connected = connected ? 1u : 0u;
            options.ConnectionSize = (uint)size;
            options.PacketIntervalUs = PacketIntervalUs;
            options.TimeoutMultiplier = TimeoutMultiplier;
            options.OriginatorVendorId = OriginatorVendorId;
            options.OriginatorSerial = OriginatorSerial ?? options.OriginatorSerial;
            return options;
        }
    }

    public abstract class CipClient : PlcClient
    {
        public CipVendor Vendor { get; }

        private protected CipClient(string ip, CipVendor vendor, CipOptions options)
            : base(ip, (options ?? throw new ArgumentNullException(nameof(options))).Configure(vendor), options.Route)
        {
            Vendor = vendor;
        }
    }

    public sealed class OmronCip : CipClient
    {
        public OmronCip(string ip, int port = 44818) : this(ip, new CipOptions { Port = port }) { }
        public OmronCip(string ip, CipOptions options) : base(ip, CipVendor.Omron, options) { }
    }

    public sealed class MelsecCip : CipClient
    {
        public MelsecCip(string ip, int port = 44818) : this(ip, new CipOptions { Port = port }) { }
        public MelsecCip(string ip, CipOptions options) : base(ip, CipVendor.Melsec, options) { }
    }

    public sealed class InovanceCip : CipClient
    {
        public InovanceCip(string ip, int port = 44818) : this(ip, new CipOptions { Port = port }) { }
        public InovanceCip(string ip, CipOptions options) : base(ip, CipVendor.Inovance, options) { }
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct PlcCipOptions
    {
        public uint StructSize;
        public CipVendor Vendor;
        public uint Port;
        public uint ConnectTimeoutMs;
        public uint ReceiveTimeoutMs;
        public uint Connected;
        public uint ConnectionSize;
        public uint PacketIntervalUs;
        public uint TimeoutMultiplier;
        public uint OriginatorVendorId;
        public uint OriginatorSerial;
    }

    internal static class CipNative
    {
        [DllImport(PlcNative.Library, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true, EntryPoint = "plc_default_cip_options")]
        private static extern PlcStatus DefaultOptions(CipVendor vendor, out PlcCipOptions options, uint size);

        [DllImport(PlcNative.Library, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true, EntryPoint = "plc_create_cip")]
        private static extern PlcStatus CreateNative(ref PlcCipOptions options,
#if PLC_PINNED_BUFFERS
            [In] byte[] host, [In] byte[] route, uint routeLength, out ulong handle);
#else
            [MarshalAs(UnmanagedType.LPUTF8Str)] string host, [In] byte[] route, uint routeLength, out ulong handle);
#endif

        internal static PlcCipOptions Options(CipVendor vendor)
        {
            PlcNative.Check(DefaultOptions(vendor, out PlcCipOptions options, (uint)Marshal.SizeOf<PlcCipOptions>()));
            return options;
        }

        internal static PlcStatus Create(ref PlcCipOptions options, string host, byte[] route, out ulong handle)
        {
            if (string.IsNullOrEmpty(host) || host.IndexOf('\0') >= 0)
                throw new ArgumentException("Host must be nonempty and contain no NUL characters.", nameof(host));
            return CreateNative(ref options, PlcNative.ValidateText(host), route, (uint)route.Length, out handle);
        }
    }
}
