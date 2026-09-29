using RsCommunication;

Console.WriteLine("Hello, World!");

Modbus modbus = new Modbus("127.0.0.1");

var connectResult = modbus.Connect();
Console.WriteLine(connectResult.IsSuccess ? "Connect Success" : "Connect Fail");

var i = modbus.Read<int>("1");

Console.WriteLine(i.Content);