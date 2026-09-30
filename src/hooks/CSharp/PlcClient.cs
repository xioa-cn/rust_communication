using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

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

    #nullable enable
    internal sealed class PlcOperationGate
    {
        [ThreadStatic] private static Waiter? cachedWaiter;
        private readonly object sync = new object();
        private Waiter? head;
        private Waiter? tail;
        private int ownerThread;
        private int depth;

        internal int WaitingCount
        {
            get
            {
                lock (sync)
                {
                    int count = 0;
                    for (Waiter? waiter = head; waiter != null; waiter = waiter.Next) count++;
                    return count;
                }
            }
        }

        internal void Enter()
        {
            int thread = Environment.CurrentManagedThreadId;
            Waiter waiter;
            lock (sync)
            {
                if (ownerThread == thread)
                {
                    depth++;
                    return;
                }
                if (ownerThread == 0)
                {
                    ownerThread = thread;
                    depth = 1;
                    return;
                }
                waiter = cachedWaiter ?? new Waiter(thread);
                cachedWaiter = null;
                waiter.Reset();
                if (tail == null) head = waiter;
                else tail.Next = waiter;
                tail = waiter;
            }
            try
            {
                waiter.Wait();
                cachedWaiter = waiter;
            }
            catch
            {
                Cancel(waiter);
                throw;
            }
        }

        internal void Exit()
        {
            Waiter? next = null;
            bool interrupted = EnterUninterruptibly(sync);
            try
            {
                if (ownerThread != Environment.CurrentManagedThreadId)
                    throw new SynchronizationLockException("The current thread does not own the PLC operation gate.");
                if (--depth == 0) next = Advance();
            }
            finally { Monitor.Exit(sync); }
            if (next != null) interrupted |= next.Signal();
            if (interrupted) throw new ThreadInterruptedException();
        }

        private static bool EnterUninterruptibly(object target)
        {
            bool interrupted = false;
            while (true)
            {
                try
                {
                    Monitor.Enter(target);
                    return interrupted;
                }
                catch (ThreadInterruptedException) { interrupted = true; }
            }
        }

        private Waiter? Advance()
        {
            Waiter? next = head;
            if (next == null)
            {
                ownerThread = 0;
                depth = 0;
                return null;
            }
            head = next.Next;
            if (head == null) tail = null;
            next.Next = null;
            ownerThread = next.Thread;
            depth = 1;
            return next;
        }

        private void Cancel(Waiter interrupted)
        {
            Waiter? next = null;
            EnterUninterruptibly(sync);
            try
            {
                if (ownerThread == interrupted.Thread)
                {
                    next = Advance();
                }
                else
                {
                    Waiter? previous = null;
                    for (Waiter? current = head; current != null; current = current.Next)
                    {
                        if (ReferenceEquals(current, interrupted))
                        {
                            if (previous == null) head = current.Next;
                            else previous.Next = current.Next;
                            if (ReferenceEquals(tail, current)) tail = previous;
                            current.Next = null;
                            break;
                        }
                        previous = current;
                    }
                }
            }
            finally { Monitor.Exit(sync); }
            next?.Signal();
        }

        private sealed class Waiter
        {
            internal readonly int Thread;
            internal Waiter? Next;
            private bool ready;

            internal Waiter(int thread) { Thread = thread; }

            internal void Reset() { ready = false; }

            internal void Wait()
            {
                lock (this)
                    while (!ready) Monitor.Wait(this);
            }

            internal bool Signal()
            {
                bool interrupted = EnterUninterruptibly(this);
                try
                {
                    ready = true;
                    Monitor.Pulse(this);
                }
                finally { Monitor.Exit(this); }
                return interrupted;
            }
        }
    }
    #nullable restore

    public abstract class PlcClient : IDisposable
    {
        private static readonly Encoding Utf8 = new UTF8Encoding(false, true);
        private readonly PlcOperationGate gate = new PlcOperationGate();
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

        public PlcResult<T> Read<T>(string address) where T : struct =>
            UseResult(address, static (handle, state) =>
            {
                T value = default;
                PlcStatus status = ReadValues(handle, state, MemoryMarshal.CreateSpan(ref value, 1));
                return status == PlcStatus.Ok ? PlcResult<T>.Success(value) : NativeFailure<T>(status);
            });

        public PlcResult<T[]> Read<T>(string address, int count) where T : struct =>
            UseResult((address, count), static (handle, state) =>
            {
                ByteLength(state.count, TypeInfo<T>().Width);
                T[] values = new T[state.count];
                PlcStatus status = ReadValues(handle, state.address, values.AsSpan());
                return status == PlcStatus.Ok ? PlcResult<T[]>.Success(values) : NativeFailure<T[]>(status);
            });

        public PlcResult<int> ReadInto<T>(string address, T[] destination) where T : struct =>
            UseResult((address, destination), static (handle, state) =>
            {
                if (state.destination == null) throw new ArgumentNullException(nameof(destination));
                PlcStatus status = ReadValues(handle, state.address, state.destination.AsSpan());
                return status == PlcStatus.Ok
                    ? PlcResult<int>.Success(state.destination.Length)
                    : NativeFailure<int>(status);
            });

        public PlcResult<int> ReadInto<T>(string address, T[] destination, int offset, int count) where T : struct =>
            UseResult((address, destination, offset, count), static (handle, state) =>
            {
                if (state.destination == null) throw new ArgumentNullException(nameof(destination));
                PlcStatus status = ReadValues(handle, state.address, state.destination.AsSpan(state.offset, state.count));
                return status == PlcStatus.Ok ? PlcResult<int>.Success(state.count) : NativeFailure<int>(status);
            });

        public PlcResult<int> ReadInto<T>(string address, Span<T> destination) where T : struct =>
            TransferValues<T>(address, destination, default, false);

        private static PlcStatus ReadValues<T>(ulong handle, string address, Span<T> values) where T : struct
        {
            var (dataType, width) = TypeInfo<T>();
            ByteLength(values.Length, width);
            Span<byte> buffer = MemoryMarshal.AsBytes(values);
            PlcStatus status = PlcNative.Read(handle, address, dataType, (uint)values.Length, buffer, out uint written);
            if (status != PlcStatus.Ok) return status;
            if (written != buffer.Length) throw new InvalidDataException("PLC returned an unexpected byte count.");
            if (typeof(T) == typeof(bool))
                foreach (byte value in buffer)
                    if (value > 1) throw new InvalidDataException("PLC returned an invalid Boolean value.");
            ConvertEndian(buffer, width);
            return PlcStatus.Ok;
        }

        public PlcResult<T> Write<T>(string address, T value) where T : struct =>
            UseResult((address, value), static (handle, state) =>
            {
                T local = state.value;
                PlcStatus status = WriteValues<T>(handle, state.address, MemoryMarshal.CreateReadOnlySpan(ref local, 1));
                return status == PlcStatus.Ok ? PlcResult<T>.Success(state.value) : NativeFailure<T>(status);
            });

        public PlcResult<int> WriteAll<T>(string address, params T[] values) where T : struct =>
            UseResult((address, values), static (handle, state) =>
            {
                if (state.values == null) throw new ArgumentNullException(nameof(values));
                PlcStatus status = WriteValues<T>(handle, state.address, state.values.AsSpan());
                return status == PlcStatus.Ok
                    ? PlcResult<int>.Success(state.values.Length)
                    : NativeFailure<int>(status);
            });

        public PlcResult<int> WriteAll<T>(string address, T[] values, int offset, int count) where T : struct =>
            UseResult((address, values, offset, count), static (handle, state) =>
            {
                if (state.values == null) throw new ArgumentNullException(nameof(values));
                PlcStatus status = WriteValues<T>(handle, state.address, state.values.AsSpan(state.offset, state.count));
                return status == PlcStatus.Ok ? PlcResult<int>.Success(state.count) : NativeFailure<int>(status);
            });

        public PlcResult<int> WriteAll<T>(string address, ReadOnlySpan<T> values) where T : struct =>
            TransferValues<T>(address, default, values, true);

        private PlcResult<int> TransferValues<T>(string address, Span<T> destination, ReadOnlySpan<T> source, bool write) where T : struct
        {
            try
            {
                gate.Enter();
                try
                {
                    if (session.IsClosed) throw new ObjectDisposedException(GetType().Name);
                    bool retained = false;
                    try
                    {
                        session.DangerousAddRef(ref retained);
                        PlcStatus status = write
                            ? WriteValues<T>(session.Value, address, source)
                            : ReadValues<T>(session.Value, address, destination);
                        return status == PlcStatus.Ok
                            ? PlcResult<int>.Success(write ? source.Length : destination.Length)
                            : NativeFailure<int>(status);
                    }
                    finally
                    {
                        if (retained) session.DangerousRelease();
                    }
                }
                finally { gate.Exit(); }
            }
            catch (ObjectDisposedException exception)
            {
                return PlcResult<int>.Failure((int)PlcStatus.InvalidHandle, exception.Message);
            }
            catch (ArgumentException exception)
            {
                return PlcResult<int>.Failure((int)PlcStatus.InvalidArgument, exception.Message);
            }
            catch (NotSupportedException exception)
            {
                return PlcResult<int>.Failure((int)PlcStatus.NotSupported, exception.Message);
            }
            catch (InvalidDataException exception)
            {
                return PlcResult<int>.Failure((int)PlcStatus.OperationFailed, exception.Message);
            }
        }

        private static PlcStatus WriteValues<T>(ulong handle, string address, ReadOnlySpan<T> values) where T : struct
        {
            var (dataType, width) = TypeInfo<T>();
            ByteLength(values.Length, width);
            ReadOnlySpan<byte> buffer = MemoryMarshal.AsBytes(values);
            if (BitConverter.IsLittleEndian || width == 1)
                return PlcNative.Write(handle, address, dataType, (uint)values.Length, buffer);
            byte[] converted = buffer.ToArray();
            ConvertEndian(converted, width);
            return PlcNative.Write(handle, address, dataType, (uint)values.Length, converted.AsSpan());
        }

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
            gate.Enter();
            try { session.Dispose(); }
            finally { gate.Exit(); }
        }

        private TResult Use<TResult>(Func<ulong, TResult> action) =>
            Use(action, static (handle, operation) => operation(handle));

        private TResult Use<TState, TResult>(TState state, Func<ulong, TState, TResult> action)
        {
            gate.Enter();
            try
            {
                if (session.IsClosed) throw new ObjectDisposedException(GetType().Name);
                bool retained = false;
                try
                {
                    session.DangerousAddRef(ref retained);
                    return action(session.Value, state);
                }
                finally
                {
                    if (retained) session.DangerousRelease();
                }
            }
            finally { gate.Exit(); }
        }

        private PlcResult Execute(Func<ulong, PlcStatus> operation) => UseResult(handle =>
        {
            PlcStatus status = operation(handle);
            return status == PlcStatus.Ok ? PlcResult<bool>.Success(true) : NativeFailure<bool>(status);
        });

        private PlcResult<T> UseResult<T>(Func<ulong, PlcResult<T>> operation) =>
            UseResult(operation, static (handle, action) => action(handle));

        private PlcResult<T> UseResult<TState, T>(TState state, Func<ulong, TState, PlcResult<T>> operation)
        {
            try { return Use(state, operation); }
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

        private static void ConvertEndian(Span<byte> buffer, int width)
        {
            if (BitConverter.IsLittleEndian || width == 1) return;
            for (int offset = 0; offset < buffer.Length; offset += width)
                buffer.Slice(offset, width).Reverse();
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
