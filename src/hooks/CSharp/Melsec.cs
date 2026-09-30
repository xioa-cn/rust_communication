using System;

namespace RsCommunication
{
    public enum MelsecProtocol
    {
        McBinaryTcp, McAsciiTcp, McBinaryUdp, McAsciiUdp, A1eBinaryTcp, A1eAsciiTcp, McRBinaryTcp
    }

    public sealed class MelsecOptions : PlcConnectionOptions
    {
        public MelsecProtocol Protocol { get; set; } = MelsecProtocol.McBinaryTcp;
        public byte Network { get; set; }
        public byte Pc { get; set; } = 255;
        public ushort Io { get; set; } = 0x03ff;
        public byte Station { get; set; }
        public ushort MonitoringTimer { get; set; } = 16;
        public MelsecOptions() : base(6000) { }
    }

    public sealed class Melsec : PlcClient
    {
        public Melsec(string ip, int port = 6000, MelsecProtocol protocol = MelsecProtocol.McBinaryTcp)
            : this(ip, new MelsecOptions { Port = port, Protocol = protocol }) { }

        public Melsec(string ip, MelsecOptions options) : base(ip, Configure(options)) { }

        private static PlcOptions Configure(MelsecOptions configuration)
        {
            if (configuration == null) throw new ArgumentNullException(nameof(configuration));
            PlcProtocol protocol;
            switch (configuration.Protocol)
            {
                case MelsecProtocol.McBinaryTcp: protocol = PlcProtocol.McBinaryTcp; break;
                case MelsecProtocol.McAsciiTcp: protocol = PlcProtocol.McAsciiTcp; break;
                case MelsecProtocol.McBinaryUdp: protocol = PlcProtocol.McBinaryUdp; break;
                case MelsecProtocol.McAsciiUdp: protocol = PlcProtocol.McAsciiUdp; break;
                case MelsecProtocol.A1eBinaryTcp: protocol = PlcProtocol.A1eBinaryTcp; break;
                case MelsecProtocol.A1eAsciiTcp: protocol = PlcProtocol.A1eAsciiTcp; break;
                case MelsecProtocol.McRBinaryTcp: protocol = PlcProtocol.McRBinaryTcp; break;
                default: throw new ArgumentOutOfRangeException(nameof(configuration.Protocol));
            }
            PlcOptions options = configuration.NativeOptions(protocol);
            options.MelsecNetwork = configuration.Network;
            options.MelsecPc = configuration.Pc;
            options.MelsecIo = configuration.Io;
            options.MelsecStation = configuration.Station;
            options.MonitoringTimer = configuration.MonitoringTimer;
            return options;
        }
    }
}
