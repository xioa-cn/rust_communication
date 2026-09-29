using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

namespace RsCommunication
{
    public enum PlcByteOrder : uint { ABCD = 0, BADC = 1, CDAB = 2, DCBA = 3 }

    public abstract class PlcConnectionOptions
    {
        public int Port { get; set; }
        public int ConnectTimeoutMs { get; set; } = 5000;
        public int ReceiveTimeoutMs { get; set; } = 5000;

        protected PlcConnectionOptions(int port) { Port = port; }

        internal PlcOptions NativeOptions(PlcProtocol protocol)
        {
            if (Port < 1 || Port > ushort.MaxValue) throw new ArgumentOutOfRangeException(nameof(Port));
            if (ConnectTimeoutMs <= 0) throw new ArgumentOutOfRangeException(nameof(ConnectTimeoutMs));
            if (ReceiveTimeoutMs <= 0) throw new ArgumentOutOfRangeException(nameof(ReceiveTimeoutMs));
            PlcOptions options = PlcNative.Options(protocol);
            options.Port = (uint)Port;
            options.ConnectTimeoutMs = (uint)ConnectTimeoutMs;
            options.ReceiveTimeoutMs = (uint)ReceiveTimeoutMs;
            return options;
        }
    }

    public abstract class PlcClient : IDisposable
    {
        private static readonly Encoding Utf8 = new UTF8Encoding(false, true);
        private readonly object gate = new object();
        private readonly SessionHandle session;

        private protected PlcClient(string ip, PlcOptions options)
        {
            session = SessionHandle.Create(ip, options);
        }

        private protected PlcClient(Stream stream, PlcOptions options)
        {
            session = SessionHandle.CreateSerial(stream, options);
        }

        public bool IsConnected => Use(handle =>
        {
            PlcNative.Check(PlcNative.IsConnected(handle, out byte connected));
            return connected != 0;
        });

        public PlcResult Connect() => Execute(PlcNative.Connect);
        public PlcResult Disconnect() => Execute(PlcNative.Disconnect);

        public PlcResult<bool> GetConnectionState() => UseResult(handle =>
        {
            PlcStatus status = PlcNative.IsConnected(handle, out byte connected);
            return status == PlcStatus.Ok
                ? PlcResult<bool>.Success(connected != 0)
                : NativeFailure<bool>(status);
        });

        public PlcResult<T> Read<T>(string address) where T : struct
        {
            PlcResult<T[]> result = Read<T>(address, 1);
            return result.IsSuccess
                ? PlcResult<T>.Success(result.Content[0])
                : PlcResult<T>.Failure(result.ErrorCode, result.Message);
        }

        public PlcResult<T[]> Read<T>(string address, int count) where T : struct =>
            UseResult(handle =>
            {
                var (dataType, width) = TypeInfo<T>();
                byte[] buffer = new byte[ByteLength(count, width)];
                PlcStatus status = PlcNative.Read(handle, address, dataType, (uint)count, buffer, out uint written);
                if (status != PlcStatus.Ok) return NativeFailure<T[]>(status);
                if (written != buffer.Length) throw new InvalidDataException("PLC returned an unexpected byte count.");
                if (typeof(T) == typeof(bool))
                    foreach (byte value in buffer)
                        if (value > 1) throw new InvalidDataException("PLC returned an invalid Boolean value.");
                ConvertEndian(buffer, width);
                T[] values = new T[count];
                Buffer.BlockCopy(buffer, 0, values, 0, buffer.Length);
                return PlcResult<T[]>.Success(values);
            });

        public PlcResult<T> Write<T>(string address, T value) where T : struct
        {
            PlcResult<int> result = WriteAll(address, new[] { value });
            return result.IsSuccess
                ? PlcResult<T>.Success(value)
                : PlcResult<T>.Failure(result.ErrorCode, result.Message);
        }

        public PlcResult<int> WriteAll<T>(string address, params T[] values) where T : struct =>
            UseResult(handle =>
            {
                if (values == null) throw new ArgumentNullException(nameof(values));
                var (dataType, width) = TypeInfo<T>();
                byte[] buffer = new byte[ByteLength(values.Length, width)];
                Buffer.BlockCopy(values, 0, buffer, 0, buffer.Length);
                ConvertEndian(buffer, width);
                PlcStatus status = PlcNative.Write(handle, address, dataType, (uint)values.Length, buffer);
                return status == PlcStatus.Ok
                    ? PlcResult<int>.Success(values.Length)
                    : NativeFailure<int>(status);
            });

        public PlcResult<string> ReadString(string address, int byteLength) =>
            ReadText(address, PlcStringKind.RawUtf8, byteLength);

        public PlcResult<string> WriteString(string address, string value) => WriteText(address, PlcStringKind.RawUtf8, value);

        private protected PlcResult<string> ReadText(string address, PlcStringKind kind, int byteLength) =>
            UseResult(handle =>
            {
                byte[] buffer = new byte[kind == PlcStringKind.S7String ? 254 : ByteLength(byteLength, 1)];
                PlcStatus status = PlcNative.ReadString(handle, address, kind, (uint)byteLength, buffer, out uint written);
                if (status != PlcStatus.Ok) return NativeFailure<string>(status);
                if (written > buffer.Length) throw new InvalidDataException("PLC returned an unexpected byte count.");
                try { return PlcResult<string>.Success(Utf8.GetString(buffer, 0, (int)written)); }
                catch (DecoderFallbackException exception)
                {
                    return PlcResult<string>.Failure((int)PlcStatus.OperationFailed, exception.Message);
                }
            });

        private protected PlcResult<string> WriteText(string address, PlcStringKind kind, string value) =>
            UseResult(handle =>
            {
                if (value == null) throw new ArgumentNullException(nameof(value));
                if (Utf8.GetByteCount(value) > PlcNative.MaxBufferBytes)
                    throw new ArgumentOutOfRangeException(nameof(value), "UTF-8 payload exceeds 1 MiB.");
                byte[] buffer = Utf8.GetBytes(value);
                PlcStatus status = PlcNative.WriteString(handle, address, kind, buffer);
                return status == PlcStatus.Ok
                    ? PlcResult<string>.Success(value)
                    : NativeFailure<string>(status);
            });

        public void Dispose()
        {
            lock (gate) session.Dispose();
        }

        private TResult Use<TResult>(Func<ulong, TResult> action)
        {
            lock (gate)
            {
                if (session.IsClosed) throw new ObjectDisposedException(GetType().Name);
                bool retained = false;
                try
                {
                    session.DangerousAddRef(ref retained);
                    return action(session.Value);
                }
                finally
                {
                    if (retained) session.DangerousRelease();
                }
            }
        }

        private PlcResult Execute(Func<ulong, PlcStatus> operation) => UseResult(handle =>
        {
            PlcStatus status = operation(handle);
            return status == PlcStatus.Ok ? PlcResult<bool>.Success(true) : NativeFailure<bool>(status);
        });

        private PlcResult<T> UseResult<T>(Func<ulong, PlcResult<T>> operation)
        {
            try { return Use(operation); }
            catch (ObjectDisposedException exception)
            {
                return PlcResult<T>.Failure((int)PlcStatus.InvalidHandle, exception.Message);
            }
            catch (ArgumentException exception)
            {
                return PlcResult<T>.Failure((int)PlcStatus.InvalidArgument, exception.Message);
            }
            catch (NotSupportedException exception)
            {
                return PlcResult<T>.Failure((int)PlcStatus.NotSupported, exception.Message);
            }
            catch (InvalidDataException exception)
            {
                return PlcResult<T>.Failure((int)PlcStatus.OperationFailed, exception.Message);
            }
        }

        private static PlcResult<T> NativeFailure<T>(PlcStatus status)
        {
            string message = PlcNative.GetLastError();
            return PlcResult<T>.Failure((int)status, string.IsNullOrEmpty(message) ? status.ToString() : message);
        }

        private static int ByteLength(int count, int width)
        {
            if (count <= 0 || count > PlcNative.MaxBufferBytes / width)
                throw new ArgumentOutOfRangeException(nameof(count), "Payload must contain 1..1048576 bytes.");
            return count * width;
        }

        private static void ConvertEndian(byte[] buffer, int width)
        {
            if (BitConverter.IsLittleEndian || width == 1) return;
            for (int offset = 0; offset < buffer.Length; offset += width)
                Array.Reverse(buffer, offset, width);
        }

        private static (PlcDataType DataType, int Width) TypeInfo<T>() where T : struct
        {
            Type type = typeof(T);
            if (type == typeof(bool)) return (PlcDataType.Bool, 1);
            if (type == typeof(byte)) return (PlcDataType.UInt8, 1);
            if (type == typeof(sbyte)) return (PlcDataType.Int8, 1);
            if (type == typeof(ushort)) return (PlcDataType.UInt16, 2);
            if (type == typeof(short)) return (PlcDataType.Int16, 2);
            if (type == typeof(uint)) return (PlcDataType.UInt32, 4);
            if (type == typeof(int)) return (PlcDataType.Int32, 4);
            if (type == typeof(ulong)) return (PlcDataType.UInt64, 8);
            if (type == typeof(long)) return (PlcDataType.Int64, 8);
            if (type == typeof(float)) return (PlcDataType.Float32, 4);
            if (type == typeof(double)) return (PlcDataType.Float64, 8);
            throw new NotSupportedException($"PLC numeric type {type.Name} is not supported. Use ReadString/WriteString for text.");
        }

        private sealed class SessionHandle : SafeHandle
        {
            private readonly PlcSerialStream adapter;
            internal ulong Value { get; private set; }
            public override bool IsInvalid => handle == IntPtr.Zero;

            private SessionHandle(PlcSerialStream adapter = null) : base(IntPtr.Zero, true)
            {
                this.adapter = adapter;
            }

            internal static SessionHandle Create(string ip, PlcOptions options)
            {
                var session = new SessionHandle();
                try
                {
                    PlcNative.Check(PlcNative.Create(ref options, ip, out ulong value));
                    session.Value = value;
                    session.SetHandle(new IntPtr(1));
                    return session;
                }
                catch { session.Dispose(); throw; }
            }

            internal static SessionHandle CreateSerial(Stream stream, PlcOptions options)
            {
                var adapter = new PlcSerialStream(stream);
                var session = new SessionHandle(adapter);
                try
                {
                    PlcSerialCallbacks callbacks = adapter.Callbacks;
                    PlcNative.Check(PlcNative.CreateSerial(ref options, ref callbacks, out ulong value));
                    session.Value = value;
                    session.SetHandle(new IntPtr(1));
                    return session;
                }
                catch { session.Dispose(); throw; }
            }

            protected override bool ReleaseHandle()
            {
                try
                {
                    PlcStatus status = PlcNative.Destroy(Value);
                    return status == PlcStatus.Ok || status == PlcStatus.InvalidHandle;
                }
                catch { return false; }
                finally { GC.KeepAlive(adapter); }
            }
        }
    }
}
