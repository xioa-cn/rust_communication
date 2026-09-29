namespace RsCommunication
{
    public abstract class PlcResult
    {
        public bool IsSuccess { get; }
        public int ErrorCode { get; }
        public string Message { get; }

        private protected PlcResult(bool isSuccess, int errorCode, string message)
        {
            IsSuccess = isSuccess;
            ErrorCode = errorCode;
            Message = message;
        }
    }

    public sealed class PlcResult<T> : PlcResult
    {
        public T Content { get; }

        private PlcResult(bool isSuccess, T content, int errorCode, string message)
            : base(isSuccess, errorCode, message)
        {
            Content = content;
        }

        internal static PlcResult<T> Success(T content) => new PlcResult<T>(true, content, 0, "Success");

        internal static PlcResult<T> Failure(int errorCode, string message) =>
            new PlcResult<T>(false, default(T), errorCode, message);
    }
}
