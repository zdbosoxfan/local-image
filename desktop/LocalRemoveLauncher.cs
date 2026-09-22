using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Linq;
using System.Net.Http;
using System.Security.Cryptography;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading.Tasks;
using System.Web.Script.Serialization;
using System.Windows.Forms;
using Microsoft.Web.WebView2.Core;
using Microsoft.Web.WebView2.WinForms;

// Paths come only from native pickers, OS launch arguments, or WebView2 File objects.
// The browser never receives the launcher credential or an arbitrary-path API.
internal static class LocalRemoveLauncher
{
    internal const string ApiBase = "http://127.0.0.1:51247";
    internal static readonly string InstallDirectory = AppDomain.CurrentDomain.BaseDirectory;
    internal static readonly string DataDirectory = Path.GetFullPath(Environment.GetEnvironmentVariable("LOCAL_REMOVE_DATA_DIR") ??
        Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Local Remove"));
    internal static readonly JavaScriptSerializer Json = new JavaScriptSerializer { MaxJsonLength = 16 * 1024 * 1024 };
    internal static readonly HttpClient Http = new HttpClient(new HttpClientHandler { UseProxy = false, AllowAutoRedirect = false }) { Timeout = TimeSpan.FromMinutes(3) };
    internal static string LauncherKey;
    internal static int ExitCode;
    private static System.Threading.Mutex desktopMutex;

    [STAThread]
    private static int Main(string[] args)
    {
        string output = Option(args, "--output");
        try
        {
            if (args.Contains("--self-test")) { RunSelfTest(output); return 0; }
            if (args.Contains("--shutdown-backend"))
            {
                ShutdownBackend().GetAwaiter().GetResult(); return 0;
            }
            if (args.Contains("--configure"))
            {
                Application.EnableVisualStyles();
                ConfigureAi(null).GetAwaiter().GetResult();
                return 0;
            }
            string[] paths = ReadPaths(args);
            if (args.Contains("--no-open"))
            {
                EnsureBackend().GetAwaiter().GetResult();
                var result = paths.Length > 0 ? RegisterPaths(paths).GetAwaiter().GetResult() : null;
                WriteResult(output, new { ok = true, url = CollectionUrl(result), result = result });
                return 0;
            }
            desktopMutex = new System.Threading.Mutex(false, "Local\\LocalRemoveDesktop");
            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            Application.Run(new LocalRemoveWindow(paths, args.Contains("--probe-webview"), output));
            return ExitCode;
        }
        catch (Exception error)
        {
            WriteError(error);
            if (args.Contains("--no-open") || args.Contains("--self-test") || args.Contains("--probe-webview")) WriteResult(output, new { ok = false, error = error.Message });
            else MessageBox.Show(error.Message, "Local Remove could not start", MessageBoxButtons.OK, MessageBoxIcon.Error);
            return 1;
        }
        finally { if (desktopMutex != null) desktopMutex.Dispose(); }
    }
    internal static string Option(string[] args, string key)
    {
        int position = Array.IndexOf(args, key);
        return position >= 0 && position + 1 < args.Length ? args[position + 1] : null;
    }
    internal static string[] ReadPaths(string[] args)
    {
        var paths = new List<string>();
        for (int i = 0; i < args.Length; i++)
        {
            if (args[i] == "--output") { i++; continue; }
            if (args[i].StartsWith("--", StringComparison.Ordinal)) continue;
            paths.Add(Path.GetFullPath(args[i]));
        }
        return paths.Distinct(StringComparer.OrdinalIgnoreCase).ToArray();
    }
    internal static bool TrustedPage(string address)
    {
        Uri uri;
        return Uri.TryCreate(address, UriKind.Absolute, out uri)
            && uri.Scheme == "http" && uri.Host == "127.0.0.1" && uri.Port == 51247
            && uri.AbsolutePath == "/remove" && String.IsNullOrEmpty(uri.UserInfo);
    }
    internal static bool TrustedDownload(string address)
    {
        Uri uri;
        return Uri.TryCreate(address, UriKind.Absolute, out uri)
            && uri.Scheme == "http" && uri.Host == "127.0.0.1" && uri.Port == 51247
            && String.IsNullOrEmpty(uri.UserInfo)
            && Regex.IsMatch(uri.AbsolutePath, "^/api/local-remove/session/[0-9a-fA-F-]{36}/(?:download|download-project)$");
    }
    internal static async Task EnsureBackend()
    {
        Directory.CreateDirectory(DataDirectory);
        if (!await BackendReady())
        {
            string backend = Path.Combine(InstallDirectory, "backend", "LocalRemoveBackend.exe");
            if (!File.Exists(backend)) throw new FileNotFoundException("Local Remove is missing an application file. Reinstall Local Remove.", backend);
            Process.Start(new ProcessStartInfo(backend)
            { UseShellExecute = false, CreateNoWindow = true, WindowStyle = ProcessWindowStyle.Hidden,
                WorkingDirectory = Path.GetDirectoryName(backend) });
        }
        DateTime deadline = DateTime.UtcNow.AddMinutes(3);
        while (!await BackendReady())
        {
            if (DateTime.UtcNow >= deadline) throw new InvalidOperationException("Local Remove could not start. Check the connector logs.");
            await Task.Delay(1000);
        }
        LauncherKey = File.ReadAllText(Path.Combine(DataDirectory, "state", "launcher.key"), Encoding.ASCII).Trim();
        if (LauncherKey.Length < 32) throw new InvalidOperationException("The Local Remove launcher credential is invalid.");
    }
    private static async Task<bool> BackendReady()
    {
        try
        {
            using (var client = new HttpClient(new HttpClientHandler { UseProxy = false, AllowAutoRedirect = false }))
            {
                client.Timeout = TimeSpan.FromSeconds(3);
                using (var response = await client.GetAsync(ApiBase + "/api/local-remove/runtime").ConfigureAwait(false))
                {
                    if (!response.IsSuccessStatusCode) return false;
                    var result = Json.Deserialize<Dictionary<string, object>>(await response.Content.ReadAsStringAsync().ConfigureAwait(false));
                    return result != null && Name(result, "application", "") == "local-remove"
                        && Name(result, "version", "") == "0.2.0"
                        && String.Equals(StringValue(result, "data_root", "").TrimEnd('\\'), DataDirectory.TrimEnd('\\'), StringComparison.OrdinalIgnoreCase);
                }
            }
        }
        catch (HttpRequestException) { return false; }
        catch (TaskCanceledException) { return false; }
        catch (ArgumentException) { return false; }
    }
    internal static async Task ConfigureAi(IWin32Window owner)
    {
        using (var dialog = new LocalRemoveSettings())
        {
            if (dialog.ShowDialog(owner) != DialogResult.OK) return;
        }
        if (await BackendReady().ConfigureAwait(false))
        {
            LauncherKey = File.ReadAllText(Path.Combine(DataDirectory, "state", "launcher.key"), Encoding.ASCII).Trim();
            await Api("/api/local-remove/reload-config", new Dictionary<string, object>()).ConfigureAwait(false);
        }
    }
    private static async Task ShutdownBackend()
    {
        if (!await BackendReady()) return;
        LauncherKey = File.ReadAllText(Path.Combine(DataDirectory, "state", "launcher.key"), Encoding.ASCII).Trim();
        await Api("/api/local-remove/shutdown", new Dictionary<string, object>());
        for (int attempt = 0; attempt < 20 && await BackendReady(); attempt++) await Task.Delay(500);
        if (await BackendReady()) throw new InvalidOperationException("Local Remove is still working. Wait for it to finish, then retry.");
    }
    internal static async Task<Dictionary<string, object>> RegisterPaths(string[] paths)
    {
        if (paths.Length == 0) return null;
        string[] projects = paths.Where(IsProjectPath).ToArray();
        if (projects.Length > 0)
        {
            if (paths.Length != 1) throw new InvalidOperationException("Open one Local Remove project at a time. Do not mix projects with images or folders.");
            if (!File.Exists(projects[0])) throw new FileNotFoundException("The selected Local Remove project no longer exists.");
            return await Api("/api/local-remove/open-project", new { path = projects[0] });
        }
        string[] folders = paths.Where(Directory.Exists).ToArray();
        if (folders.Length > 0)
        {
            if (paths.Length != 1) throw new InvalidOperationException("Open one folder at a time, or select a group of image files.");
            return await Api("/api/local-remove/register-folder", new { path = folders[0] });
        }
        if (paths.Any(path => !File.Exists(path))) throw new FileNotFoundException("One of the selected images no longer exists.");
        return await Api("/api/local-remove/open-files", new { paths = paths });
    }
    internal static async Task<Dictionary<string, object>> Api(string path, object payload)
    {
        return await Request(HttpMethod.Post, path, payload);
    }
    internal static async Task<Dictionary<string, object>> ReadSession(string sessionId)
    {
        return await Request(HttpMethod.Get, "/api/local-remove/session/" + Uri.EscapeDataString(sessionId), null);
    }
    private static async Task<Dictionary<string, object>> Request(HttpMethod method, string path, object payload)
    {
        using (var request = new HttpRequestMessage(method, ApiBase + path))
        {
            request.Headers.Add("x-local-launcher", LauncherKey);
            if (payload != null) request.Content = new StringContent(Json.Serialize(payload), Encoding.UTF8, "application/json");
            using (var response = await Http.SendAsync(request))
            {
                string body = await response.Content.ReadAsStringAsync();
                Dictionary<string, object> decoded = null;
                try { decoded = Json.Deserialize<Dictionary<string, object>>(body); } catch (ArgumentException) { }
                if (!response.IsSuccessStatusCode)
                {
                    object detail;
                    string message = decoded != null && decoded.TryGetValue("detail", out detail) ? Convert.ToString(detail) : "The Local Remove command could not be completed.";
                    throw new InvalidOperationException(message);
                }
                if (decoded == null) throw new InvalidOperationException("The Local Remove backend returned an invalid response.");
                return decoded;
            }
        }
    }
    internal static string CollectionUrl(Dictionary<string, object> response)
    {
        if (response == null) return ApiBase + "/remove";
        object rawCollection, rawSession, rawId;
        if (response.TryGetValue("collection", out rawCollection))
        {
            var collection = rawCollection as Dictionary<string, object>;
            if (collection != null && collection.TryGetValue("id", out rawId))
                return ApiBase + "/remove?collection=" + Uri.EscapeDataString(Convert.ToString(rawId));
        }
        if (response.TryGetValue("session", out rawSession))
        {
            var session = rawSession as Dictionary<string, object>;
            if (session != null && session.TryGetValue("id", out rawId))
                return ApiBase + "/remove?session=" + Uri.EscapeDataString(Convert.ToString(rawId));
        }
        throw new InvalidOperationException("The backend did not return an image collection or project session.");
    }
    internal static bool IsProjectPath(string path)
    {
        return String.Equals(Path.GetExtension(path), ".lremove", StringComparison.OrdinalIgnoreCase);
    }
    internal static Dictionary<string, object> ProjectSavePayload(Dictionary<string, object> message)
    {
        object rawSession, rawRevision;
        Guid sessionId;
        if (!message.TryGetValue("session_id", out rawSession) || !(rawSession is string) || !Guid.TryParse((string)rawSession, out sessionId))
            throw new InvalidOperationException("Choose an open image before saving a project.");
        if (!message.TryGetValue("revision", out rawRevision) || (!(rawRevision is int) && !(rawRevision is long)))
            throw new InvalidOperationException("The project revision is missing or invalid.");
        long revision = Convert.ToInt64(rawRevision);
        if (revision < 0 || revision > Int32.MaxValue) throw new InvalidOperationException("The project revision is invalid.");
        // Deliberately exclude any page-provided path, expected_hash, or unsupported draft data.
        return new Dictionary<string, object> { { "session_id", sessionId.ToString("D") }, { "revision", revision } };
    }
    internal static bool Flag(Dictionary<string, object> value, string key)
    {
        object raw;
        return value != null && value.TryGetValue(key, out raw) && raw is bool && (bool)raw;
    }
    internal static string Name(Dictionary<string, object> value, string key, string fallback)
    {
        object raw;
        return value != null && value.TryGetValue(key, out raw) && raw is string && !String.IsNullOrWhiteSpace((string)raw)
            ? Path.GetFileName((string)raw) : fallback;
    }
    internal static string StringValue(Dictionary<string, object> value, string key, string fallback)
    {
        object raw;
        return value != null && value.TryGetValue(key, out raw) && raw is string ? (string)raw : fallback;
    }
    internal static string HashFile(string path)
    {
        using (var stream = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read))
        using (var hash = SHA256.Create()) return BitConverter.ToString(hash.ComputeHash(stream)).Replace("-", "").ToLowerInvariant();
    }
    internal static void WriteResult(string path, object value)
    {
        string result = Json.Serialize(value);
        if (!String.IsNullOrEmpty(path)) File.WriteAllText(Path.GetFullPath(path), result, new UTF8Encoding(false));
        Console.WriteLine(result);
    }
    internal static void WriteError(Exception error)
    {
        try
        {
            string logs = Path.Combine(DataDirectory, "logs");
            Directory.CreateDirectory(logs);
            File.WriteAllText(Path.Combine(logs, "local-remove-launcher-error.txt"), DateTime.Now + Environment.NewLine + error.ToString());
        }
        catch { }
    }
    private static void RunSelfTest(string output)
    {
        var pathSetting = new Dictionary<string, object> { { "model_directory", @"C:\Sample User\AI Models" } };
        if (StringValue(pathSetting, "model_directory", "") != @"C:\Sample User\AI Models")
            throw new InvalidOperationException("The installed path settings test failed.");
        if (!TrustedPage(ApiBase + "/remove?collection=test") || TrustedPage("https://127.0.0.1:51247/remove")
            || TrustedPage("http://localhost:51247/remove") || TrustedPage("http://127.0.0.1:51248/remove")
            || TrustedPage("http://127.0.0.1:51247/remove-other") || TrustedPage("http://user@127.0.0.1:51247/remove")
            || TrustedPage("https://example.com/remove")) throw new InvalidOperationException("The trusted-page boundary test failed.");
        if (!TrustedDownload(ApiBase + "/api/local-remove/session/e2419d61-d78b-4f8a-b138-8e65386aa3e9/download?ext=tif")
            || !TrustedDownload(ApiBase + "/api/local-remove/session/e2419d61-d78b-4f8a-b138-8e65386aa3e9/download-project")
            || TrustedDownload(ApiBase + "/api/local-remove/session/e2419d61-d78b-4f8a-b138-8e65386aa3e9/preview")
            || TrustedDownload(ApiBase + "/api/local-remove/session/e2419d61-d78b-4f8a-b138-8e65386aa3e9/download-project/extra")
            || TrustedDownload("https://example.com/api/local-remove/session/e2419d61-d78b-4f8a-b138-8e65386aa3e9/download"))
            throw new InvalidOperationException("The download boundary test failed.");
        var savePayload = ProjectSavePayload(new Dictionary<string, object>
        {
            { "session_id", "e2419d61-d78b-4f8a-b138-8e65386aa3e9" }, { "revision", 7 },
            { "path", "C:\\untrusted-page-path.lremove" }, { "expected_hash", "untrusted" }, { "draft", new object() }
        });
        if (savePayload.Count != 2 || Convert.ToInt64(savePayload["revision"]) != 7)
            throw new InvalidOperationException("The project-save payload allowlist test failed.");
        bool badRevisionRejected = false, badSessionRejected = false, mixedProjectsRejected = false;
        try { ProjectSavePayload(new Dictionary<string, object> { { "session_id", "e2419d61-d78b-4f8a-b138-8e65386aa3e9" }, { "revision", true } }); }
        catch (InvalidOperationException) { badRevisionRejected = true; }
        try { ProjectSavePayload(new Dictionary<string, object> { { "session_id", "../other-session" }, { "revision", 0 } }); }
        catch (InvalidOperationException) { badSessionRejected = true; }
        try { RegisterPaths(new[] { "photo.jpg", "project.lremove" }).GetAwaiter().GetResult(); }
        catch (InvalidOperationException) { mixedProjectsRejected = true; }
        if (!badRevisionRejected || !badSessionRejected || !mixedProjectsRejected || !IsProjectPath("edit.LREMOVE") || IsProjectPath("edit.lremove.jpg"))
            throw new InvalidOperationException("The project argument validation tests failed.");
        var projectResponse = new Dictionary<string, object> { { "collection", null }, { "session", new Dictionary<string, object> { { "id", "project-session" } } } };
        if (CollectionUrl(projectResponse) != ApiBase + "/remove?session=project-session")
            throw new InvalidOperationException("The project navigation test failed.");
        var gate = new CloseRequestGate();
        string firstClose = gate.Begin();
        if (gate.Begin() != firstClose || gate.Complete("wrong-id", true) || gate.Approved || gate.PendingId != firstClose)
            throw new InvalidOperationException("The close request correlation test failed.");
        if (!gate.Complete(firstClose, false) || gate.Approved || gate.PendingId != null)
            throw new InvalidOperationException("The cancelled close preservation test failed.");
        string nextClose = gate.Begin();
        if (nextClose == firstClose || gate.Complete(firstClose, true) || gate.Approved || !gate.Complete(nextClose, true) || !gate.Approved)
            throw new InvalidOperationException("The explicit close approval test failed.");
        WriteResult(output, new { ok = true, trusted_origin_checks = 12, project_boundary_checks = 7, close_handshake_checks = 7,
            bridge_version = 2, projects = true, closeRequests = true, runtime = CoreWebView2Environment.GetAvailableBrowserVersionString(), architecture = Environment.Is64BitProcess ? "x64" : "x86" });
    }
}

// An OS-window close is approved only by a matching response from the trusted editor.
// There is deliberately no timeout path that grants approval.
internal sealed class CloseRequestGate
{
    internal string PendingId { get; private set; }
    internal bool Approved { get; private set; }
    internal string Begin()
    {
        if (PendingId == null) PendingId = "close-" + Guid.NewGuid().ToString("N");
        return PendingId;
    }
    internal bool Complete(string id, bool approved)
    {
        if (PendingId == null || !String.Equals(PendingId, id, StringComparison.Ordinal)) return false;
        PendingId = null; Approved = approved; return true;
    }
}

internal sealed class LocalRemoveWindow : Form
{
    private readonly WebView2 view;
    private readonly System.Windows.Forms.Timer heartbeat = new System.Windows.Forms.Timer { Interval = 10000 };
    private readonly Label startup;
    private readonly string[] paths;
    private readonly bool probe;
    private readonly string output;
    private bool bridgeBusy;
    private bool trustedEditorReady;
    private readonly CloseRequestGate closeGate = new CloseRequestGate();
    private readonly HashSet<ulong> ignoredNavigations = new HashSet<ulong>();
    internal LocalRemoveWindow(string[] initialPaths, bool hiddenProbe, string resultPath)
    {
        paths = initialPaths; probe = hiddenProbe; output = resultPath;
        Text = "Local Remove";
        Icon = Icon.ExtractAssociatedIcon(System.Reflection.Assembly.GetExecutingAssembly().Location);
        BackColor = Color.FromArgb(31, 32, 34); MinimumSize = new Size(800, 560); Size = new Size(1380, 940);
        StartPosition = FormStartPosition.CenterScreen; AutoScaleMode = AutoScaleMode.Dpi;
        if (probe) { Opacity = 0; ShowInTaskbar = false; }
        view = new WebView2 { Dock = DockStyle.Fill, DefaultBackgroundColor = BackColor, AllowExternalDrop = true };
        startup = new Label { Dock = DockStyle.Fill, Text = "Starting Local Remove…", TextAlign = ContentAlignment.MiddleCenter,
            ForeColor = Color.Gainsboro, Font = new Font("Segoe UI", 12), BackColor = BackColor };
        Controls.Add(view); Controls.Add(startup);
        Shown += async delegate { await Initialize(); };
        FormClosing += OnFormClosing;
        heartbeat.Tick += async delegate {
            try { await LocalRemoveLauncher.Api("/api/local-remove/heartbeat", new Dictionary<string, object>()); }
            catch (Exception error) { LocalRemoveLauncher.WriteError(error); }
        };
        FormClosed += delegate { heartbeat.Stop(); heartbeat.Dispose(); };
    }
    private async Task Initialize()
    {
        try
        {
            await LocalRemoveLauncher.EnsureBackend();
            await LocalRemoveLauncher.Api("/api/local-remove/heartbeat", new Dictionary<string, object>());
            heartbeat.Start();
            var collection = paths.Length > 0 ? await LocalRemoveLauncher.RegisterPaths(paths) : null;
            string profile = Path.Combine(LocalRemoveLauncher.DataDirectory, probe ? "WebView2-Probe" : "WebView2");
            var environment = await CoreWebView2Environment.CreateAsync(null, profile);
            await view.EnsureCoreWebView2Async(environment);
            var core = view.CoreWebView2;
            core.Settings.AreDefaultContextMenusEnabled = false; core.Settings.IsStatusBarEnabled = false;
            core.Settings.IsZoomControlEnabled = false; core.Settings.AreBrowserAcceleratorKeysEnabled = false;
            core.Settings.AreHostObjectsAllowed = false; core.Settings.IsWebMessageEnabled = true;
            core.NavigationStarting += OnNavigation;
            core.NewWindowRequested += delegate(object sender, CoreWebView2NewWindowRequestedEventArgs e) { e.Handled = true; if (e.IsUserInitiated) OpenExternal(e.Uri); };
            core.WebMessageReceived += OnWebMessage;
            core.DownloadStarting += OnDownload;
            core.PermissionRequested += delegate(object sender, CoreWebView2PermissionRequestedEventArgs e) { e.State = CoreWebView2PermissionState.Deny; };
            core.NavigationCompleted += delegate(object sender, CoreWebView2NavigationCompletedEventArgs e)
            {
                if (ignoredNavigations.Remove(e.NavigationId)) return;
                if (!e.IsSuccess) { FinishError(new InvalidOperationException("Local Remove could not load its editor (" + e.WebErrorStatus + ").")); return; }
                startup.Visible = false;
                if (probe)
                {
                    LocalRemoveLauncher.WriteResult(output, new { ok = true, runtime = environment.BrowserVersionString, source = core.Source, title = core.DocumentTitle, bridge = "v2:ready/openFiles/openFolder/openProject/saveProject/drop/closeReady" });
                    Close();
                }
            };
            core.Navigate(LocalRemoveLauncher.CollectionUrl(collection));
        }
        catch (Exception error) { FinishError(error); }
    }
    private void FinishError(Exception error)
    {
        LocalRemoveLauncher.ExitCode = 1; LocalRemoveLauncher.WriteError(error);
        if (probe) { LocalRemoveLauncher.WriteResult(output, new { ok = false, error = error.Message }); Close(); }
        else { startup.Text = "Local Remove could not start.\n\n" + error.Message; startup.Visible = true; startup.BringToFront(); }
    }
    private void OnFormClosing(object sender, FormClosingEventArgs e)
    {
        // A failed startup has never given the editor control of an image.
        if (probe || closeGate.Approved || !trustedEditorReady) return;
        e.Cancel = true;
        if (closeGate.PendingId != null) return;
        if (view.IsDisposed || view.CoreWebView2 == null || !LocalRemoveLauncher.TrustedPage(view.CoreWebView2.Source))
        {
            MessageBox.Show(this, "The editor is not available to review your open edits. The window has been kept open so no layers are discarded.",
                "Local Remove", MessageBoxButtons.OK, MessageBoxIcon.Information);
            return;
        }
        string id = closeGate.Begin();
        try
        {
            view.CoreWebView2.PostWebMessageAsJson(LocalRemoveLauncher.Json.Serialize(new
            { type = "local-remove-native", action = "requestClose", id = id }));
        }
        catch (Exception error)
        {
            closeGate.Complete(id, false); LocalRemoveLauncher.WriteError(error);
            MessageBox.Show(this, "The editor could not review your open edits. The window has been kept open.", "Local Remove", MessageBoxButtons.OK, MessageBoxIcon.Information);
        }
    }
    private void OnNavigation(object sender, CoreWebView2NavigationStartingEventArgs e)
    {
        if (LocalRemoveLauncher.TrustedPage(e.Uri)) return;
        ignoredNavigations.Add(e.NavigationId);
        if (LocalRemoveLauncher.TrustedPage(view.CoreWebView2.Source) && LocalRemoveLauncher.TrustedDownload(e.Uri)) return;
        e.Cancel = true;
        if (e.IsUserInitiated) OpenExternal(e.Uri);
    }
    private async void OnDownload(object sender, CoreWebView2DownloadStartingEventArgs e)
    {
        e.Handled = true;
        if (probe || !LocalRemoveLauncher.TrustedPage(view.CoreWebView2.Source) || !LocalRemoveLauncher.TrustedDownload(e.DownloadOperation.Uri))
        { e.Cancel = true; return; }
        using (e.GetDeferral())
        {
            // Native modal dialogs must open after WebView2's callback returns.
            await Task.Yield();
            try
            {
                if (IsDisposed) { e.Cancel = true; return; }
                bool projectDownload = new Uri(e.DownloadOperation.Uri).AbsolutePath.EndsWith("/download-project", StringComparison.Ordinal);
                using (var picker = new SaveFileDialog { Title = projectDownload ? "Export editable project" : "Export image", FileName = Path.GetFileName(e.ResultFilePath),
                    Filter = projectDownload ? "Local Remove project (*.lremove)|*.lremove" : "Image file|*.*", DefaultExt = projectDownload ? "lremove" : "", AddExtension = true, OverwritePrompt = true, RestoreDirectory = true })
                {
                    if (picker.ShowDialog(this) == DialogResult.OK) e.ResultFilePath = picker.FileName;
                    else e.Cancel = true;
                }
            }
            catch (Exception error) { e.Cancel = true; LocalRemoveLauncher.WriteError(error); }
        }
    }
    private static void OpenExternal(string address)
    {
        Uri uri;
        if (!Uri.TryCreate(address, UriKind.Absolute, out uri) || uri.Scheme != "https" || !String.IsNullOrEmpty(uri.UserInfo)) return;
        try { Process.Start(new ProcessStartInfo(uri.AbsoluteUri) { UseShellExecute = true }); }
        catch (Exception error) { LocalRemoveLauncher.WriteError(error); }
    }
    private void Reply(object id, object result, string error)
    {
        if (view.IsDisposed || view.CoreWebView2 == null || !LocalRemoveLauncher.TrustedPage(view.CoreWebView2.Source)) return;
        view.CoreWebView2.PostWebMessageAsJson(LocalRemoveLauncher.Json.Serialize(new { type = "local-remove-native", id = id, result = result, error = error }));
    }
    private async void OnWebMessage(object sender, CoreWebView2WebMessageReceivedEventArgs e)
    {
        if (!LocalRemoveLauncher.TrustedPage(e.Source) || !LocalRemoveLauncher.TrustedPage(view.CoreWebView2.Source)) return;
        object id = null; bool ownsBusy = false;
        try
        {
            string raw = e.WebMessageAsJson;
            if (raw == null || raw.Length > 4096) return;
            var message = LocalRemoveLauncher.Json.Deserialize<Dictionary<string, object>>(raw);
            object action;
            if (message == null || !message.TryGetValue("id", out id) || !(id is string) || ((string)id).Length > 128
                || !message.TryGetValue("action", out action) || !(action is string)) return;
            string verb = (string)action;
            if (verb == "ready") { trustedEditorReady = true; Reply(id, new { native = true, version = 2, projects = true, closeRequests = true }, null); return; }
            if (verb == "closeReady")
            {
                object approved;
                if (!message.TryGetValue("approved", out approved) || !(approved is bool)) throw new InvalidOperationException("The close decision is missing.");
                bool decision = (bool)approved && !bridgeBusy;
                if (closeGate.Complete((string)id, decision) && decision)
                    BeginInvoke(new Action(delegate { if (!IsDisposed) Close(); }));
                return;
            }
            if (verb != "openFiles" && verb != "openFolder" && verb != "openProject" && verb != "saveProject" && verb != "drop" && verb != "configureAi") throw new InvalidOperationException("Unknown desktop action.");
            if (bridgeBusy) throw new InvalidOperationException("Finish opening the current selection first.");
            bridgeBusy = true; ownsBusy = true;
            if (verb == "configureAi")
            {
                await Task.Yield();
                await LocalRemoveLauncher.ConfigureAi(this);
                Reply(id, new { ok = true }, null); return;
            }
            if (verb == "saveProject")
            {
                var saved = await SaveProject(message);
                Reply(id, saved, null); return;
            }
            string[] selected = new string[0];
            if (verb == "openFiles")
            {
                await Task.Yield();
                if (IsDisposed) return;
                using (var picker = new OpenFileDialog { Title = "Open images or a Local Remove project", Multiselect = true, CheckFileExists = true,
                    Filter = "Images and Local Remove projects|*.jpg;*.jpeg;*.png;*.tif;*.tiff;*.webp;*.lremove|Images|*.jpg;*.jpeg;*.png;*.tif;*.tiff;*.webp|Local Remove project (*.lremove)|*.lremove", RestoreDirectory = true })
                    if (picker.ShowDialog(this) == DialogResult.OK) selected = picker.FileNames;
            }
            else if (verb == "openProject")
            {
                await Task.Yield();
                if (IsDisposed) return;
                using (var picker = new OpenFileDialog { Title = "Open editable project", Multiselect = false, CheckFileExists = true,
                    Filter = "Local Remove project (*.lremove)|*.lremove", DefaultExt = "lremove", RestoreDirectory = true })
                    if (picker.ShowDialog(this) == DialogResult.OK) selected = new[] { picker.FileName };
            }
            else if (verb == "openFolder")
            {
                await Task.Yield();
                if (IsDisposed) return;
                using (var picker = new FolderBrowserDialog { Description = "Choose a folder of images to edit in Local Remove.", ShowNewFolderButton = false })
                    if (picker.ShowDialog(this) == DialogResult.OK) selected = new[] { picker.SelectedPath };
            }
            else
            {
                // Chromium-created File wrappers supply OS paths; page-provided strings are ignored.
                if (e.AdditionalObjects == null || e.AdditionalObjects.Count == 0) throw new InvalidOperationException("Drop image files from File Explorer, or use Open Folder.");
                selected = e.AdditionalObjects.OfType<CoreWebView2File>().Select(file => file.Path)
                    .Where(path => !String.IsNullOrWhiteSpace(path)).Distinct(StringComparer.OrdinalIgnoreCase).ToArray();
                if (selected.Length == 0) throw new InvalidOperationException("The dropped item is not a local image file.");
            }
            var result = selected.Length == 0 ? null : await LocalRemoveLauncher.RegisterPaths(selected);
            Reply(id, result, null);
        }
        catch (Exception error) { Reply(id, null, error.Message); }
        finally { if (ownsBusy) bridgeBusy = false; }
    }
    private async Task<Dictionary<string, object>> SaveProject(Dictionary<string, object> message)
    {
        var payload = LocalRemoveLauncher.ProjectSavePayload(message);
        var session = await LocalRemoveLauncher.ReadSession((string)payload["session_id"]);
        bool choosePath = LocalRemoveLauncher.Flag(message, "saveAs") || !LocalRemoveLauncher.Flag(session, "has_project_path");
        if (choosePath)
        {
            await Task.Yield();
            if (IsDisposed) return null;
            string sourceName = LocalRemoveLauncher.Name(session, "name", "Untitled");
            string suggested = LocalRemoveLauncher.Name(session, "project_name", Path.GetFileNameWithoutExtension(sourceName) + ".lremove");
            using (var picker = new SaveFileDialog { Title = "Save editable project", FileName = Path.ChangeExtension(suggested, ".lremove"),
                Filter = "Local Remove project (*.lremove)|*.lremove", DefaultExt = "lremove", AddExtension = true, OverwritePrompt = true, RestoreDirectory = true })
            {
                if (picker.ShowDialog(this) != DialogResult.OK) return null;
                string destination = Path.GetFullPath(picker.FileName);
                if (!LocalRemoveLauncher.IsProjectPath(destination)) throw new InvalidOperationException("Save editable projects with the .lremove extension.");
                payload["path"] = destination;
                if (File.Exists(destination)) payload["expected_hash"] = await Task.Run(delegate { return LocalRemoveLauncher.HashFile(destination); });
            }
        }
        // A known path is resolved and conflict-checked privately by the backend.
        return await LocalRemoveLauncher.Api("/api/local-remove/save-project", payload);
    }
}
