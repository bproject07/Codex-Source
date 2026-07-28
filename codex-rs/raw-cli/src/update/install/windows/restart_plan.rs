pub(super) const RESTART_PLAN_SOURCE: &str = r#"
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using ComFileTime = System.Runtime.InteropServices.ComTypes.FILETIME;

public sealed class CodexRawRestartPlan : IDisposable
{
    private const int ErrorSuccess = 0;

    private enum Operation
    {
        None,
        Shutdown,
        Restart
    }

    private delegate void RmWriteStatusCallback(uint percentageComplete);

    [StructLayout(LayoutKind.Sequential)]
    private struct RmUniqueProcess
    {
        public int ProcessId;
        public ComFileTime ProcessStartTime;
    }

    [DllImport("rstrtmgr.dll", CharSet = CharSet.Unicode)]
    private static extern int RmStartSession(
        out uint sessionHandle,
        int sessionFlags,
        StringBuilder sessionKey);

    [DllImport("rstrtmgr.dll")]
    private static extern int RmEndSession(uint sessionHandle);

    [DllImport("rstrtmgr.dll", CharSet = CharSet.Unicode)]
    private static extern int RmRegisterResources(
        uint sessionHandle,
        uint fileCount,
        string[] fileNames,
        uint applicationCount,
        [In] RmUniqueProcess[] applications,
        uint serviceCount,
        string[] serviceNames);

    [DllImport("rstrtmgr.dll")]
    private static extern int RmShutdown(
        uint sessionHandle,
        uint actionFlags,
        RmWriteStatusCallback statusCallback);

    [DllImport("rstrtmgr.dll")]
    private static extern int RmRestart(
        uint sessionHandle,
        uint restartFlags,
        RmWriteStatusCallback statusCallback);

    private uint session;
    private bool hasSession;
    private readonly int processCount;
    private Task<int> active;
    private Operation activeOperation;
    private bool restartCompleted;
    private bool disposed;

    private CodexRawRestartPlan(uint session, bool hasSession, int processCount)
    {
        this.session = session;
        this.hasSession = hasSession;
        this.processCount = processCount;
        this.activeOperation = Operation.None;
    }

    public static CodexRawRestartPlan Create(long[] identities)
    {
        if (identities == null || identities.Length % 3 != 0)
        {
            throw new ArgumentException(
                "Invalid exact Restart Manager process identity list.");
        }
        int count = identities.Length / 3;
        var exact = new RmUniqueProcess[count];
        for (int index = 0; index < count; index++)
        {
            long processId = identities[index * 3];
            long low = identities[index * 3 + 1];
            long high = identities[index * 3 + 2];
            if (processId < 0 || processId > UInt32.MaxValue ||
                low < 0 || low > UInt32.MaxValue ||
                high < 0 || high > UInt32.MaxValue)
            {
                throw new ArgumentException(
                    "Invalid exact Restart Manager process identity value.");
            }
            exact[index].ProcessId = unchecked((int)(uint)processId);
            exact[index].ProcessStartTime.dwLowDateTime =
                unchecked((int)(uint)low);
            exact[index].ProcessStartTime.dwHighDateTime =
                unchecked((int)(uint)high);
        }
        if (exact.Length == 0)
        {
            return new CodexRawRestartPlan(0, false, 0);
        }

        uint session;
        var key = new StringBuilder(33);
        Check(RmStartSession(out session, 0, key));
        try
        {
            Check(RmRegisterResources(
                session,
                0,
                null,
                (uint)exact.Length,
                exact,
                0,
                null));
            return new CodexRawRestartPlan(session, true, exact.Length);
        }
        catch
        {
            RmEndSession(session);
            throw;
        }
    }

    public int ProcessCount
    {
        get { return processCount; }
    }

    public void Shutdown(int timeoutMilliseconds)
    {
        EnsureUsable();
        if (!hasSession)
        {
            return;
        }
        Start(Operation.Shutdown, () => RmShutdown(session, 0, null));
        Check(WaitActive(timeoutMilliseconds, "graceful shutdown"));
    }

    public void Restart(int timeoutMilliseconds)
    {
        EnsureUsable();
        if (!hasSession || restartCompleted)
        {
            return;
        }

        if (active != null)
        {
            Operation prior = activeOperation;
            int priorResult = WaitActive(
                timeoutMilliseconds,
                "the preceding Restart Manager operation");
            if (prior == Operation.Restart)
            {
                Check(priorResult);
                restartCompleted = true;
                return;
            }
        }

        Start(Operation.Restart, () => RmRestart(session, 0, null));
        Check(WaitActive(timeoutMilliseconds, "restart"));
        restartCompleted = true;
    }

    private void Start(Operation operation, Func<int> call)
    {
        if (active != null)
        {
            throw new InvalidOperationException(
                "A Restart Manager operation is already active.");
        }
        activeOperation = operation;
        active = Task.Factory.StartNew(
            call,
            CancellationToken.None,
            TaskCreationOptions.DenyChildAttach,
            TaskScheduler.Default);
    }

    private int WaitActive(int timeoutMilliseconds, string operation)
    {
        if (!active.Wait(timeoutMilliseconds))
        {
            throw new TimeoutException(
                "Restart Manager " + operation + " exceeded its time limit.");
        }
        int result = active.Result;
        active = null;
        activeOperation = Operation.None;
        return result;
    }

    private void EnsureUsable()
    {
        if (disposed)
        {
            throw new ObjectDisposedException("CodexRawRestartPlan");
        }
    }

    public void Dispose()
    {
        if (disposed)
        {
            return;
        }
        disposed = true;
        if (active != null && active.IsCompleted)
        {
            active = null;
            activeOperation = Operation.None;
        }
        if (hasSession && active == null)
        {
            RmEndSession(session);
            hasSession = false;
        }
    }

    private static void Check(int result)
    {
        if (result != ErrorSuccess)
        {
            throw new Win32Exception(result);
        }
    }
}
"#;
