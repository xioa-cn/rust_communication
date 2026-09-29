using System;

namespace RsCommunication
{
    public enum InovanceSeries : uint { AM = 0, AC = 1, AP = 2, EVO = 3, H5U = 4, H3U = 5, Easy = 6 }

    public sealed class InovanceOptions : PlcConnectionOptions
    {
        public InovanceSeries Series { get; set; } = InovanceSeries.AM;
        public byte UnitId { get; set; } = 1;
        public PlcByteOrder ByteOrder { get; set; } = PlcByteOrder.CDAB;
        public InovanceOptions() : base(502) { }
    }

    public sealed class Inovance : PlcClient
    {
        public Inovance(string ip, InovanceSeries series, int port = 502)
            : this(ip, new InovanceOptions { Port = port, Series = series }) { }

        public Inovance(string ip, InovanceOptions options) : base(ip, Configure(options)) { }

        private static PlcOptions Configure(InovanceOptions configuration)
        {
            if (configuration == null) throw new ArgumentNullException(nameof(configuration));
            PlcOptions options = configuration.NativeOptions(PlcProtocol.InovanceModbusTcp);
            options.InovanceType = (uint)configuration.Series;
            options.UnitId = configuration.UnitId;
            options.ByteOrder = configuration.ByteOrder;
            return options;
        }
    }
}
