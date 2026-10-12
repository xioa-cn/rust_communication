using System;
using System.IO.Ports;
using RsCommunication;

namespace Rs_TestDemo
{
    public class Program
    {
        public static void Main(string[] args)
        {
            string portName = args.Length > 0 ? args[0] : "COM3";
            const int baudRate = 9600;
            const byte unitId = 1;
            const string registerAddress = "HR0";
            const int registerCount = 2;
            bool enableWrite = args.Length > 1 && args[1] == "--write";

            Console.WriteLine($"Modbus RTU：{portName}，{baudRate}，8N1，站号 {unitId}");
            Console.WriteLine("请按设备配置修改端口、波特率、校验位、停止位、站号和寄存器地址。");

            try
            {
                using (var serialPort = new SerialPort(portName, baudRate, Parity.None, 8, StopBits.One))
                {
                    serialPort.Handshake = Handshake.None;
                    serialPort.ReadTimeout = 2000;
                    serialPort.WriteTimeout = 2000;
                    serialPort.Open();

                    using (var modbus = Modbus.Rtu(serialPort.BaseStream,
                        unitId: unitId, baudRate: (uint)baudRate, byteOrder: PlcByteOrder.ABCD))
                    {
                        var connectResult = modbus.Connect();
                        if (!CheckResult("创建 RTU 会话", connectResult)) return;
                        Console.WriteLine("RTU 会话已连接，设备是否在线还需通过读取确认。");

                        var readResult = modbus.Read<ushort>(registerAddress);
                        if (!CheckResult("读取单个保持寄存器", readResult)) return;
                        Console.WriteLine($"{registerAddress} = {readResult.Content}");

                        var batchResult = modbus.Read<ushort>(registerAddress, registerCount);
                        if (!CheckResult("读取连续保持寄存器", batchResult)) return;
                        Console.WriteLine($"从 {registerAddress} 开始的 {registerCount} 个寄存器：" +
                            string.Join(", ", batchResult.Content));

                        if (!enableWrite)
                        {
                            Console.WriteLine("默认只读。确认目标寄存器可安全写入后，使用参数 COM3 --write 启用写入示例。");
                            return;
                        }

                        const ushort writeValue = 123;
                        Console.WriteLine($"即将把 {registerAddress} 写为 {writeValue}，此操作会修改设备数据。");
                        var writeResult = modbus.Write<ushort>(registerAddress, writeValue);
                        if (!CheckResult("写入保持寄存器", writeResult)) return;

                        var verifyResult = modbus.Read<ushort>(registerAddress);
                        if (!CheckResult("写入后回读", verifyResult)) return;
                        Console.WriteLine($"写入后 {registerAddress} = {verifyResult.Content}");
                        if (verifyResult.Content != writeValue)
                        {
                            Console.WriteLine("回读值与写入值不一致，请检查设备逻辑是否覆盖了该寄存器。");
                            Environment.ExitCode = 1;
                        }
                    }
                }
            }
            catch (Exception exception)
            {
                Console.WriteLine($"RTU 示例运行失败：{exception.Message}");
                Console.WriteLine("请检查串口是否存在或被占用，以及输出目录中的 rs_appliaction.dll 是否可加载。");
                Environment.ExitCode = 1;
            }
        }

        private static bool CheckResult(string operation, PlcResult result)
        {
            if (result.IsSuccess) return true;

            Console.WriteLine($"{operation}失败：[{result.ErrorCode}] {result.Message}");
            Console.WriteLine("请检查接线、串口参数、站号和地址；传输错误后需重新创建 RTU 客户端。");
            Environment.ExitCode = 1;
            return false;
        }
    }
}
