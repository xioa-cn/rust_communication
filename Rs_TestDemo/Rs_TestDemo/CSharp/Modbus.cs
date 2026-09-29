using System;
using System.IO;

namespace RsCommunication
{
    public enum ModbusProtocol { Tcp, Udp }

    public sealed class ModbusOptions : PlcConnectionOptions
    {
        public ModbusProtocol Protocol { get; set; } = ModbusProtocol.Tcp;
        public byte UnitId { get; set; } = 1;
        public PlcByteOrder ByteOrder { get; set; } = PlcByteOrder.CDAB;
        public ModbusOptions() : base(502) { }
    }

    public sealed class Modbus : PlcClient
    {
        public Modbus(string ip, int port = 502, byte unitId = 1, ModbusProtocol protocol = ModbusProtocol.Tcp)
            : this(ip, new ModbusOptions { Port = port, UnitId = unitId, Protocol = protocol }) { }

        public Modbus(string ip, ModbusOptions options) : base(ip, Configure(options)) { }

        private Modbus(Stream stream, PlcOptions options) : base(stream, options) { }

        public static Modbus Rtu(Stream stream, byte unitId = 1, uint baudRate = 9600,
            PlcByteOrder byteOrder = PlcByteOrder.ABCD)
        {
            PlcOptions options = PlcNative.Options(PlcProtocol.ModbusRtu);
            options.UnitId = unitId;
            options.BaudRate = baudRate;
            options.ByteOrder = byteOrder;
            return new Modbus(stream, options);
        }

        public static Modbus Ascii(Stream stream, byte unitId = 1, PlcByteOrder byteOrder = PlcByteOrder.ABCD)
        {
            PlcOptions options = PlcNative.Options(PlcProtocol.ModbusAscii);
            options.UnitId = unitId;
            options.ByteOrder = byteOrder;
            return new Modbus(stream, options);
        }

        private static PlcOptions Configure(ModbusOptions configuration)
        {
            if (configuration == null) throw new ArgumentNullException(nameof(configuration));
            PlcProtocol protocol = configuration.Protocol switch
            {
                ModbusProtocol.Tcp => PlcProtocol.ModbusTcp,
                ModbusProtocol.Udp => PlcProtocol.ModbusUdp,
                _ => throw new ArgumentOutOfRangeException(nameof(configuration.Protocol))
            };
            PlcOptions options = configuration.NativeOptions(protocol);
            options.UnitId = configuration.UnitId;
            options.ByteOrder = configuration.ByteOrder;
            return options;
        }
    }
}
