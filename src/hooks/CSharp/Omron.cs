using System;

namespace RsCommunication
{
    public enum OmronProtocol { FinsTcp, FinsUdp }

    public sealed class OmronOptions : PlcConnectionOptions
    {
        public OmronProtocol Protocol { get; set; } = OmronProtocol.FinsTcp;
        public PlcByteOrder ByteOrder { get; set; } = PlcByteOrder.CDAB;
        public byte DestinationNetwork { get; set; }
        public byte DestinationNode { get; set; }
        public byte DestinationUnit { get; set; }
        public byte SourceNetwork { get; set; }
        public byte SourceNode { get; set; }
        public byte SourceUnit { get; set; }
        public byte GatewayCount { get; set; } = 2;
        public OmronOptions() : base(9600) { }
    }

    public sealed class Omron : PlcClient
    {
        public Omron(string ip, int port = 9600, OmronProtocol protocol = OmronProtocol.FinsTcp)
            : this(ip, new OmronOptions { Port = port, Protocol = protocol }) { }

        public Omron(string ip, OmronOptions options) : base(ip, Configure(options)) { }

        private static PlcOptions Configure(OmronOptions configuration)
        {
            if (configuration == null) throw new ArgumentNullException(nameof(configuration));
            PlcProtocol protocol;
            switch (configuration.Protocol)
            {
                case OmronProtocol.FinsTcp: protocol = PlcProtocol.FinsTcp; break;
                case OmronProtocol.FinsUdp: protocol = PlcProtocol.FinsUdp; break;
                default: throw new ArgumentOutOfRangeException(nameof(configuration.Protocol));
            }
            PlcOptions options = configuration.NativeOptions(protocol);
            options.ByteOrder = configuration.ByteOrder;
            options.FinsDestinationNetwork = configuration.DestinationNetwork;
            options.FinsDestinationNode = configuration.DestinationNode;
            options.FinsDestinationUnit = configuration.DestinationUnit;
            options.FinsSourceNetwork = configuration.SourceNetwork;
            options.FinsSourceNode = configuration.SourceNode;
            options.FinsSourceUnit = configuration.SourceUnit;
            options.FinsGatewayCount = configuration.GatewayCount;
            return options;
        }
    }
}
