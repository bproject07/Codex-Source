pub(super) const QUEUED_LOCK_SOURCE: &str = r#"
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

public sealed class CodexRawQueuedFileLock : IDisposable
{
    private const uint GenericRead = 0x80000000;
    private const uint GenericWrite = 0x40000000;
    private const uint ShareRead = 0x00000001;
    private const uint ShareWrite = 0x00000002;
    private const uint ShareDelete = 0x00000004;
    private const uint OpenAlways = 4;
    private const uint FileAttributeNormal = 0x00000080;
    private const uint FileFlagOverlapped = 0x40000000;
    private const uint LockfileExclusiveLock = 0x00000002;
    private const int ErrorIoPending = 997;
    private const int ErrorNotFound = 1168;
    private const uint WaitObject0 = 0;
    private const uint WaitTimeout = 258;
    private const uint BoundedCleanupWait = 5000;

    [StructLayout(LayoutKind.Sequential)]
    private struct Overlapped
    {
        public UIntPtr Internal;
        public UIntPtr InternalHigh;
        public uint Offset;
        public uint OffsetHigh;
        public IntPtr Event;
    }

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern SafeFileHandle CreateFile(
        string fileName,
        uint desiredAccess,
        uint shareMode,
        IntPtr securityAttributes,
        uint creationDisposition,
        uint flagsAndAttributes,
        IntPtr templateFile);

    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool LockFileEx(
        SafeFileHandle file,
        uint flags,
        uint reserved,
        uint bytesLow,
        uint bytesHigh,
        IntPtr overlapped);

    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool UnlockFileEx(
        SafeFileHandle file,
        uint reserved,
        uint bytesLow,
        uint bytesHigh,
        IntPtr overlapped);

    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetOverlappedResult(
        SafeFileHandle file,
        IntPtr overlapped,
        out uint transferred,
        [MarshalAs(UnmanagedType.Bool)] bool wait);

    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool CancelIoEx(
        SafeFileHandle file,
        IntPtr overlapped);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr CreateEvent(
        IntPtr eventAttributes,
        [MarshalAs(UnmanagedType.Bool)] bool manualReset,
        [MarshalAs(UnmanagedType.Bool)] bool initialState,
        string name);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern uint WaitForSingleObject(IntPtr handle, uint milliseconds);

    [DllImport("kernel32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool CloseHandle(IntPtr handle);

    private SafeFileHandle file;
    private IntPtr completion;
    private IntPtr overlapped;
    private bool pending;
    private bool owned;
    private bool disposed;

    private CodexRawQueuedFileLock(
        SafeFileHandle file,
        IntPtr completion,
        IntPtr overlapped,
        bool pending,
        bool owned)
    {
        this.file = file;
        this.completion = completion;
        this.overlapped = overlapped;
        this.pending = pending;
        this.owned = owned;
    }

    public static CodexRawQueuedFileLock Arm(string path)
    {
        SafeFileHandle file = CreateFile(
            path,
            GenericRead | GenericWrite,
            ShareRead | ShareWrite | ShareDelete,
            IntPtr.Zero,
            OpenAlways,
            FileAttributeNormal | FileFlagOverlapped,
            IntPtr.Zero);
        if (file.IsInvalid)
        {
            throw new Win32Exception(Marshal.GetLastWin32Error());
        }

        IntPtr completion = CreateEvent(IntPtr.Zero, true, false, null);
        if (completion == IntPtr.Zero)
        {
            int error = Marshal.GetLastWin32Error();
            file.Dispose();
            throw new Win32Exception(error);
        }

        IntPtr overlapped = Marshal.AllocHGlobal(Marshal.SizeOf(typeof(Overlapped)));
        try
        {
            var state = new Overlapped { Event = completion };
            Marshal.StructureToPtr(state, overlapped, false);
            bool owned = LockFileEx(
                file,
                LockfileExclusiveLock,
                0,
                1,
                0,
                overlapped);
            if (!owned)
            {
                int error = Marshal.GetLastWin32Error();
                if (error != ErrorIoPending)
                {
                    throw new Win32Exception(error);
                }
            }
            return new CodexRawQueuedFileLock(
                file,
                completion,
                overlapped,
                !owned,
                owned);
        }
        catch
        {
            Marshal.FreeHGlobal(overlapped);
            CloseHandle(completion);
            file.Dispose();
            throw;
        }
    }

    public void WaitUntilOwned(int timeoutMilliseconds)
    {
        if (owned)
        {
            return;
        }
        if (!pending)
        {
            throw new InvalidOperationException("Updater lock request is not armed.");
        }

        uint wait = WaitForSingleObject(completion, (uint)timeoutMilliseconds);
        if (wait == WaitTimeout)
        {
            throw new TimeoutException(
                "Timed out acquiring the exact updater byte-range lock.");
        }
        if (wait != WaitObject0)
        {
            throw new Win32Exception(Marshal.GetLastWin32Error());
        }

        uint transferred;
        pending = false;
        if (!GetOverlappedResult(file, overlapped, out transferred, false))
        {
            throw new Win32Exception(Marshal.GetLastWin32Error());
        }
        owned = true;
    }

    public void Dispose()
    {
        if (disposed)
        {
            return;
        }
        disposed = true;

        if (owned)
        {
            UnlockFileEx(file, 0, 1, 0, overlapped);
            owned = false;
        }
        if (pending)
        {
            bool canceled = CancelIoEx(file, overlapped);
            int cancelError = canceled ? 0 : Marshal.GetLastWin32Error();
            if (canceled || cancelError == ErrorNotFound)
            {
                uint wait = WaitForSingleObject(completion, BoundedCleanupWait);
                pending = wait != WaitObject0;
            }
        }

        file.Dispose();
        if (!pending)
        {
            Marshal.FreeHGlobal(overlapped);
            CloseHandle(completion);
        }
    }
}
"#;
