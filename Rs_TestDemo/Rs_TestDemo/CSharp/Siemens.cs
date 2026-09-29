using System;

namespace RsCommunication
{
    public enum S7Cpu : uint { S1200 = 0, S1500 = 1, S300 = 2, S400 = 3, S200 = 4, S200Smart = 5 }

    public sealed class SiemensOptions : PlcConnectionOptions
    {
        public S7Cpu Cpu { get; set; } = S7Cpu.S1200;
        public byte Rack { get; set; }
        public byte Slot { get; set; }
        public ushort? LocalTsap { get; set; }
        public ushort? RemoteTsap { get; set; }
        public SiemensOptions() : base(102) { }
    }

    public sealed class Siemens : PlcClient
    {
        public Siemens(string ip, int port = 102, S7Cpu cpu = S7Cpu.S1200)
            : this(ip, new SiemensOptions { Port = port, Cpu = cpu }) { }

        public Siemens(string ip, SiemensOptions options) : base(ip, Configure(options)) { }

        public PlcResult<string> ReadS7String(string address) => ReadText(address, PlcStringKind.S7String, 0);
        public PlcResult<string> WriteS7String(string address, string value) => WriteText(address, PlcStringKind.S7String, value);

        private static PlcOptions Configure(SiemensOptions configuration)
        {
            if (configuration == null) throw new ArgumentNullException(nameof(configuration));
            if (configuration.LocalTsap.HasValue != configuration.RemoteTsap.HasValue)
                throw new ArgumentException("LocalTsap and RemoteTsap must be specified together.", nameof(configuration));
            PlcOptions options = configuration.NativeOptions(PlcProtocol.S7);
            options.S7Type = (uint)configuration.Cpu;
            options.Rack = configuration.Rack;
            options.Slot = configuration.Slot;
            options.UseTsap = configuration.LocalTsap.HasValue ? 1u : 0u;
            options.LocalTsap = configuration.LocalTsap.GetValueOrDefault();
            options.RemoteTsap = configuration.RemoteTsap.GetValueOrDefault();
            return options;
        }
    }
}
