using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

namespace RsCommunication
{
    internal enum PlcProtocol : uint
    {
        S7 = 1, McBinaryTcp = 2, McAsciiTcp = 3, McBinaryUdp = 4, McAsciiUdp = 5,
        A1eBinaryTcp = 6, A1eAsciiTcp = 7, McRBinaryTcp = 8,
        FinsTcp = 9, FinsUdp = 10, ModbusTcp = 11, ModbusUdp = 12,
        ModbusRtu = 13, ModbusAscii = 14, InovanceModbusTcp = 15
    }

    internal enum PlcDataType : uint
    {
        Bool = 1, UInt8 = 2, Int8 = 3, UInt16 = 4, Int16 = 5, UInt32 = 6,
        Int32 = 7, UInt64 = 8, Int64 = 9, Float32 = 10, Float64 = 11
    }

    internal enum PlcStringKind : uint { RawUtf8 = 0, S7String = 1 }

    internal enum PlcStatus : int
    {
        Ok = 0, InvalidArgument = -1, InvalidHandle = -2, BufferTooSmall = -3,
        NotSupported = -4, OperationFailed = -5, InternalError = -6
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct PlcOptions
    {
        public uint StructSize;
        public PlcProtocol Protocol;
        public uint Port;
        public uint ConnectTimeoutMs;
        public uint ReceiveTimeoutMs;
        public uint S7Type;
        public uint Rack;
        public uint Slot;
        public uint UseTsap;
        public uint LocalTsap;
        public uint RemoteTsap;
        public uint UnitId;
        public PlcByteOrder ByteOrder;
        public uint InovanceType;
        public uint MelsecNetwork;
        public uint MelsecPc;
        public uint MelsecIo;
        public uint MelsecStation;
        public uint MonitoringTimer;
        public uint FinsDestinationNetwork;
        public uint FinsDestinationNode;
        public uint FinsDestinationUnit;
        public uint FinsSourceNetwork;
        public uint FinsSourceNode;
        public uint FinsSourceUnit;
        public uint FinsGatewayCount;
        public uint BaudRate;
    }

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal delegate int PlcSerialRead(UIntPtr context, IntPtr buffer, uint capacity);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal delegate int PlcSerialWrite(UIntPtr context, IntPtr buffer, uint length);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal delegate int PlcSerialFlush(UIntPtr context);

    [StructLayout(LayoutKind.Sequential)]
    internal struct PlcSerialCallbacks
    {
        public UIntPtr Context;
        [MarshalAs(UnmanagedType.FunctionPtr)] public PlcSerialRead Read;
        [MarshalAs(UnmanagedType.FunctionPtr)] public PlcSerialWrite Write;
        [MarshalAs(UnmanagedType.FunctionPtr)] public PlcSerialFlush Flush;
    }

    internal static class PlcNative
    {
        public const string Library = "rs_appliaction";
        public const uint MaxBufferBytes = 1_048_576;

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true, EntryPoint = "plc_abi_version")]
        public static extern uint AbiVersion();

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true, EntryPoint = "plc_default_options")]
        private static extern PlcStatus DefaultOptions(PlcProtocol protocol, out PlcOptions options, uint size);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true, EntryPoint = "plc_create")]
        private static extern PlcStatus CreateNative(ref PlcOptions options,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string host, out ulong handle);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true, EntryPoint = "plc_create_serial")]
        public static extern PlcStatus CreateSerial(ref PlcOptions options, ref PlcSerialCallbacks callbacks, out ulong handle);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true, EntryPoint = "plc_connect")]
        public static extern PlcStatus Connect(ulong handle);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true, EntryPoint = "plc_disconnect")]
        public static extern PlcStatus Disconnect(ulong handle);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true, EntryPoint = "plc_is_connected")]
        public static extern PlcStatus IsConnected(ulong handle, out byte connected);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true, EntryPoint = "plc_destroy")]
        public static extern PlcStatus Destroy(ulong handle);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true, EntryPoint = "plc_read")]
        private static extern PlcStatus ReadNative(ulong handle, [MarshalAs(UnmanagedType.LPUTF8Str)] string address,
            PlcDataType dataType, uint count, [Out] byte[] buffer, uint capacity, out uint written);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true, EntryPoint = "plc_write")]
        private static extern PlcStatus WriteNative(ulong handle, [MarshalAs(UnmanagedType.LPUTF8Str)] string address,
            PlcDataType dataType, uint count, [In] byte[] buffer, uint length);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true, EntryPoint = "plc_read_string")]
        private static extern PlcStatus ReadStringNative(ulong handle, [MarshalAs(UnmanagedType.LPUTF8Str)] string address,
            PlcStringKind kind, uint byteLength, [Out] byte[] buffer, uint capacity, out uint written);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true, EntryPoint = "plc_write_string")]
        private static extern PlcStatus WriteStringNative(ulong handle, [MarshalAs(UnmanagedType.LPUTF8Str)] string address,
            PlcStringKind kind, [In] byte[] buffer, uint byteLength);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, ExactSpelling = true, EntryPoint = "plc_last_error")]
        private static extern uint LastError([Out] byte[] buffer, uint capacity);

        private static string ValidateText(string value)
        {
            if (string.IsNullOrEmpty(value) || value.IndexOf('\0') >= 0)
                throw new ArgumentException("Host/address must be nonempty and contain no NUL characters.");
            return value;
        }

        private static uint BufferLength(byte[] buffer)
        {
            if (buffer == null) throw new ArgumentNullException(nameof(buffer));
            if (buffer.Length > MaxBufferBytes)
                throw new ArgumentException("The buffer must not exceed 1 MiB.", nameof(buffer));
            return (uint)buffer.Length;
        }

        public static PlcStatus Create(ref PlcOptions options, string host, out ulong handle)
        {
            return CreateNative(ref options, ValidateText(host), out handle);
        }

        public static PlcStatus Read(ulong handle, string address, PlcDataType dataType,
            uint count, byte[] buffer, out uint written)
        {
            return ReadNative(handle, ValidateText(address), dataType, count, buffer, BufferLength(buffer), out written);
        }

        public static PlcStatus Write(ulong handle, string address, PlcDataType dataType, uint count, byte[] buffer)
        {
            return WriteNative(handle, ValidateText(address), dataType, count, buffer, BufferLength(buffer));
        }

        public static PlcStatus ReadString(ulong handle, string address, PlcStringKind kind,
            uint byteLength, byte[] buffer, out uint written)
        {
            return ReadStringNative(handle, ValidateText(address), kind, byteLength, buffer, BufferLength(buffer), out written);
        }

        public static PlcStatus WriteString(ulong handle, string address, PlcStringKind kind, byte[] buffer)
        {
            return WriteStringNative(handle, ValidateText(address), kind, buffer, BufferLength(buffer));
        }

        public static PlcOptions Options(PlcProtocol protocol)
        {
            if (AbiVersion() != 1) throw new NotSupportedException("Unsupported PLC DLL ABI version.");
            Check(DefaultOptions(protocol, out PlcOptions options, (uint)Marshal.SizeOf<PlcOptions>()));
            return options;
        }

        public static string GetLastError()
        {
            uint capacity = LastError(Array.Empty<byte>(), 0);
            if (capacity == 0) return "Cannot retrieve the native error message.";
            byte[] buffer = new byte[checked((int)capacity)];
            uint actual = LastError(buffer, capacity);
            if (actual == 0 || actual > capacity) return "Cannot retrieve the native error message.";
            return Encoding.UTF8.GetString(buffer, 0, checked((int)actual - 1));
        }

        public static void Check(PlcStatus status)
        {
            if (status != PlcStatus.Ok)
                throw new InvalidOperationException($"PLC {status} ({(int)status}): {GetLastError()}");
        }
    }

    internal sealed class PlcSerialStream
    {
        private readonly Stream stream;
        public PlcSerialCallbacks Callbacks { get; }

        public PlcSerialStream(Stream stream)
        {
            this.stream = stream ?? throw new ArgumentNullException(nameof(stream));
            if (!stream.CanRead || !stream.CanWrite)
                throw new ArgumentException("The serial stream must support both reading and writing.", nameof(stream));
            Callbacks = new PlcSerialCallbacks { Read = Read, Write = Write, Flush = Flush };
        }

        private int Read(UIntPtr context, IntPtr buffer, uint capacity)
        {
            try
            {
                byte[] bytes = new byte[checked((int)capacity)];
                int actual = stream.Read(bytes, 0, bytes.Length);
                Marshal.Copy(bytes, 0, buffer, actual);
                return actual;
            }
            catch (TimeoutException) { return -2; }
            catch (Exception) { return -1; }
        }

        private int Write(UIntPtr context, IntPtr buffer, uint length)
        {
            try
            {
                byte[] bytes = new byte[checked((int)length)];
                Marshal.Copy(buffer, bytes, 0, bytes.Length);
                stream.Write(bytes, 0, bytes.Length);
                return bytes.Length;
            }
            catch (TimeoutException) { return -2; }
            catch (Exception) { return -1; }
        }

        private int Flush(UIntPtr context)
        {
            try { stream.Flush(); return 0; }
            catch (TimeoutException) { return -2; }
            catch (Exception) { return -1; }
        }
    }
}
