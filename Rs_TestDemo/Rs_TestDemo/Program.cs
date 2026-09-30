using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using RsCommunication;

namespace Rs_TestDemo
{
    public class Program
    {
        public static void Main(string[] args)
        {
            Console.WriteLine("Hello, World!");

            OmronCip omronCip = new OmronCip("127.0.0.1");

            var cipConnectResult = omronCip.Connect();

            Console.WriteLine(cipConnectResult.IsSuccess ? "Connect Success" : "Connect Fail");

            var read = omronCip.Read<short>("A");

            Console.WriteLine(read.IsSuccess ? "Read Success" : "Read Fail");

            Console.WriteLine(read.Content);


            return;

            ModbusOptions modbusOptions = new ModbusOptions();

            modbusOptions.ByteOrder = PlcByteOrder.CDAB;

            Modbus modbus = new Modbus("127.0.0.1");

            var connectResult = modbus.Connect();
            Console.WriteLine(connectResult.IsSuccess ? "Connect Success" : "Connect Fail");

            Stopwatch sp = new Stopwatch();

            sp.Start();

            var i = modbus.Read<short>("1");
            sp.Stop();
            Console.WriteLine("sp milliseconds:" + sp.ElapsedMilliseconds);
// Console.WriteLine("value:" + i.Content);

            List<Task> task = new List<Task>();


            sp.Restart();
            foreach (var item in Enumerable.Range(0, 30000))
            {
                var i0 = modbus.Write<short>(item.ToString(), (short)item);
                var i1 = modbus.Read<short>(item.ToString(), 100);

                if (!i1.IsSuccess)
                {
                    Console.WriteLine(i1.Message);
                    continue;
                }

                if (i1.Content[0] != item)
                {
                    Console.WriteLine("写入失败！");
                }
            }

            sp.Stop();
            Console.WriteLine("single sp milliseconds:" + sp.ElapsedMilliseconds);

            foreach (var ran in Enumerable.Range(0, 5))
            {
                task.Add(Task.Run(() =>
                {
                    Stopwatch sp1 = new Stopwatch();
                    sp.Start();
                    foreach (var item in Enumerable.Range(0, 30000))
                    {
                        var i0 = modbus.Write<short>(item.ToString(), (short)item);
                        var i1 = modbus.Read<short>(item.ToString(), 100);

                        if (!i1.IsSuccess)
                        {
                            Console.WriteLine(i1.Message);
                            continue;
                        }

                        if (i1.Content[0] != item)
                        {
                            Console.WriteLine("写入失败！");
                        }
                    }

                    sp1.Stop();
                    Thread.Sleep(1);
                    Console.WriteLine("task sp milliseconds:" + sp1.ElapsedMilliseconds);
                }));
            }

            Task.WaitAll(task.ToArray());
        }
    }
}
