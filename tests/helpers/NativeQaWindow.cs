// Test-only, read-only window/UIA metadata observer. Never referenced or
// packaged by the application. There are no UI input/action commands.
// Every command is bound to one QA launch manifest, executable hash, PID and
// process creation time. UIA inspection, when requested, only reads properties.
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Web.Script.Serialization;
using System.Windows.Automation;

internal static class NativeQaWindow
{
    private static readonly JavaScriptSerializer Json = new JavaScriptSerializer { MaxJsonLength = 4 * 1024 * 1024 };
    private const uint GW_OWNER = 4;
    [StructLayout(LayoutKind.Sequential)] private struct Rect { public int Left, Top, Right, Bottom; }
    private delegate bool EnumWindowsCallback(IntPtr hwnd, IntPtr data);
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumWindowsCallback callback, IntPtr data);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll")] private static extern IntPtr GetWindow(IntPtr hwnd, uint command);
    [DllImport("user32.dll")] private static extern bool IsWindow(IntPtr hwnd);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] private static extern bool IsWindowEnabled(IntPtr hwnd);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int maximum);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern int GetClassName(IntPtr hwnd, StringBuilder text, int maximum);
    [DllImport("user32.dll")] private static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
    [DllImport("user32.dll")] private static extern bool GetClientRect(IntPtr hwnd, out Rect rect);
    [DllImport("user32.dll")] private static extern uint GetDpiForWindow(IntPtr hwnd);
    // Affects only this observer's coordinate interpretation, not global DPI,
    // the inspected application's DPI, or any window's position/size.
    [DllImport("user32.dll")] private static extern bool SetProcessDpiAwarenessContext(IntPtr context);
    private static string Text(IntPtr hwnd, bool className)
    {
        var value = new StringBuilder(512);
        if (className) GetClassName(hwnd, value, value.Capacity); else GetWindowText(hwnd, value, value.Capacity);
        return value.ToString();
    }
    private static string Hash(string path)
    {
        using (var hash = SHA256.Create()) using (var stream = File.OpenRead(path)) return BitConverter.ToString(hash.ComputeHash(stream)).Replace("-", "").ToLowerInvariant();
    }
    private static bool Within(string path, string directory)
    {
        string root = Path.GetFullPath(directory).TrimEnd(Path.DirectorySeparatorChar) + Path.DirectorySeparatorChar;
        return Path.GetFullPath(path).StartsWith(root, StringComparison.OrdinalIgnoreCase);
    }
    private static string Value(Dictionary<string, object> data, string name) { return Convert.ToString(data[name]); }
    private static bool Owned(IntPtr window, IntPtr root)
    {
        if (!IsWindow(window)) return false;
        for (int depth = 0; window != IntPtr.Zero && depth < 12; depth++, window = GetWindow(window, GW_OWNER)) if (window == root) return true;
        return false;
    }
    private static object WindowRecord(IntPtr window, IntPtr root)
    {
        uint pid; GetWindowThreadProcessId(window, out pid); Rect outer, client; GetWindowRect(window, out outer); GetClientRect(window, out client);
        uint dpi = 0; try { dpi = GetDpiForWindow(window); } catch (EntryPointNotFoundException) { }
        return new { hwnd = window.ToInt64().ToString(), owner = GetWindow(window, GW_OWNER).ToInt64().ToString(), pid = pid,
            title = Text(window, false), className = Text(window, true), root = window == root, visible = IsWindowVisible(window), enabled = IsWindowEnabled(window),
            outer = new { x = outer.Left, y = outer.Top, width = outer.Right - outer.Left, height = outer.Bottom - outer.Top },
            client = new { width = client.Right - client.Left, height = client.Bottom - client.Top }, dpi = dpi };
    }
    private static object[] Elements(AutomationElement root)
    {
        var results = new List<object>(); var pending = new Queue<Tuple<AutomationElement, int>>(); pending.Enqueue(Tuple.Create(root, 0));
        var walker = TreeWalker.ControlViewWalker;
        while (pending.Count > 0 && results.Count < 300)
        {
            var item = pending.Dequeue(); var element = item.Item1;
            try
            {
                var current = element.Current;
                results.Add(new { index = results.Count, depth = item.Item2, runtimeId = string.Join(",", element.GetRuntimeId()), automationId = current.AutomationId,
                    name = current.Name, controlType = current.ControlType.ProgrammaticName, enabled = current.IsEnabled, offscreen = current.IsOffscreen });
                if (item.Item2 < 10) for (var child = walker.GetFirstChild(element); child != null; child = walker.GetNextSibling(child)) pending.Enqueue(Tuple.Create(child, item.Item2 + 1));
            }
            catch (ElementNotAvailableException) { }
        }
        return results.ToArray();
    }
    [STAThread] private static int Main(string[] args)
    {
        Console.OutputEncoding = new UTF8Encoding(false);
        try
        {
            if (args.Length == 0 || args[0] == "capabilities")
            {
                Console.WriteLine(Json.Serialize(new { available = true, uiAutomation = typeof(AutomationElement).Assembly.FullName,
                    framework = Environment.Version.ToString(), is64Bit = Environment.Is64BitProcess, interactiveSession = Environment.UserInteractive,
                    commands = new[] { "capabilities", "windows", "inspect" }, readOnly = true,
                    desktopActionsExecuted = false, applicationLaunched = false })); return 0;
            }
            try { SetProcessDpiAwarenessContext(new IntPtr(-4)); } catch (EntryPointNotFoundException) { }
            if (args.Length < 2) throw new ArgumentException("Supply a QA launch manifest.");
            var workspace = Directory.GetCurrentDirectory(); var qaRoot = Path.Combine(workspace, "qa-artifacts");
            if (!File.Exists(Path.Combine(workspace, "backend", "local_remove.py")) || !Within(args[1], qaRoot)) throw new InvalidOperationException("Run from the repository root with a manifest under qa-artifacts.");
            var manifest = Json.Deserialize<Dictionary<string, object>>(File.ReadAllText(args[1], Encoding.UTF8));
            var executable = Path.GetFullPath(Value(manifest, "host")); var runRoot = Path.GetFullPath(Value(manifest, "runRoot"));
            if (!Within(executable, Path.Combine(workspace, "dist")) || !Within(runRoot, qaRoot)) throw new InvalidOperationException("Only a workspace-owned package and QA run are allowed.");
            var process = Process.GetProcessById(Convert.ToInt32(manifest["pid"]));
            if (!String.Equals(Path.GetFullPath(process.MainModule.FileName), executable, StringComparison.OrdinalIgnoreCase)
                || process.StartTime.ToUniversalTime().ToFileTimeUtc().ToString() != Value(manifest, "processStartFileTime")
                || Hash(executable) != Value(manifest, "hostSha256")) throw new InvalidOperationException("The QA process identity changed; no UI action was attempted.");
            process.Refresh(); var main = process.MainWindowHandle;
            if (main == IntPtr.Zero) throw new InvalidOperationException("The QA process has no targetable main window.");
            if (args[0] == "windows")
            {
                var windows = new List<object>(); EnumWindows((window, unused) => { if (Owned(window, main)) windows.Add(WindowRecord(window, main)); return true; }, IntPtr.Zero);
                Console.WriteLine(Json.Serialize(new { pid = process.Id, windows = windows, desktopActionsExecuted = false })); return 0;
            }
            if (args.Length < 3) throw new ArgumentException("Supply a window handle returned by windows.");
            var target = new IntPtr(Int64.Parse(args[2])); if (!Owned(target, main)) throw new InvalidOperationException("This window is not owned by the recorded QA process.");
            if (args[0] == "inspect")
            {
                Console.WriteLine(Json.Serialize(new { observedAtUtc = DateTime.UtcNow.ToString("o"), pid = process.Id,
                    processStartFileTime = Value(manifest, "processStartFileTime"), window = WindowRecord(target, main), elements = Elements(AutomationElement.FromHandle(target)), desktopActionsExecuted = false })); return 0;
            }
            throw new ArgumentException("Only read-only capabilities, windows and inspect commands are supported.");
        }
        catch (Exception error) { Console.Error.WriteLine(Json.Serialize(new { error = error.GetType().Name, message = error.Message, noAutomaticRetry = true })); return 1; }
    }
}
