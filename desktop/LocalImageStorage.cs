using System;
using System.Collections.Generic;
using System.IO;
using System.Text;
using System.Web.Script.Serialization;

// Program Files contains immutable application files. Each launching user owns
// their profile, and imports the installer's optional storage choices once.
internal static class LocalImageStorage
{
    internal static string ResolveProfileRoot(string localAppData, string explicitRoot)
    {
        if (!String.IsNullOrWhiteSpace(explicitRoot)) return Path.GetFullPath(explicitRoot);
        string current = Path.Combine(localAppData, "Local Image");
        string legacy = Path.Combine(localAppData, "Local Remove");
        return HasProfile(legacy) && !HasProfile(current) ? legacy : current;
    }
    private static bool HasProfile(string path)
    {
        return File.Exists(Path.Combine(path, "config.json")) || Directory.Exists(Path.Combine(path, "state"));
    }
    internal static string StoragePath(string value, string installDirectory)
    {
        if (String.IsNullOrWhiteSpace(value)) return "";
        if (value.Length < 3 || !Char.IsLetter(value[0]) || value[1] != ':' || value[2] != '\\' || value.IndexOf('\0') >= 0)
            throw new InvalidOperationException("Choose a local absolute folder for AI storage.");
        string full = Path.GetFullPath(value);
        string root = Path.GetPathRoot(full);
        if (full.Length > root.Length) full = full.TrimEnd(Path.DirectorySeparatorChar);
        string app = Path.GetFullPath(installDirectory).TrimEnd(Path.DirectorySeparatorChar);
        if (String.Equals(full, app, StringComparison.OrdinalIgnoreCase)
            || full.StartsWith(app + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase))
            throw new InvalidOperationException("Store models and ComfyUI outside the application installation folder.");
        return full;
    }
    internal static void CheckWritable(string path)
    {
        if (String.IsNullOrEmpty(path)) return;
        foreach (var protectedFolder in new[] { Environment.SpecialFolder.ProgramFiles,
            Environment.SpecialFolder.ProgramFilesX86, Environment.SpecialFolder.Windows })
        {
            string folder = Environment.GetFolderPath(protectedFolder).TrimEnd(Path.DirectorySeparatorChar);
            if (!String.IsNullOrEmpty(folder) && (String.Equals(path, folder, StringComparison.OrdinalIgnoreCase)
                || path.StartsWith(folder + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase)))
                throw new InvalidOperationException("Choose AI storage outside Program Files and Windows, such as AppData or another data drive.");
        }
        string parent = path;
        while (!Directory.Exists(parent))
        {
            string next = Path.GetDirectoryName(parent);
            if (String.IsNullOrEmpty(next) || next == parent) throw new InvalidOperationException("The selected storage drive is unavailable.");
            parent = next;
        }
        string probe = Path.Combine(parent, ".local-image-write-check-" + Guid.NewGuid().ToString("N"));
        try
        {
            using (var stream = new FileStream(probe, FileMode.CreateNew, FileAccess.Write, FileShare.None)) { }
        }
        catch (UnauthorizedAccessException) { throw new InvalidOperationException("Local Image cannot write to this folder. Choose a folder in AppData or on an accessible data drive."); }
        finally { if (File.Exists(probe)) File.Delete(probe); }
    }
    internal static void InitializeProfile(string profile, string installDirectory)
    {
        Directory.CreateDirectory(profile);
        string target = Path.Combine(profile, "config.json");
        // Existing profiles are never replaced by installer choices on update.
        if (File.Exists(target)) return;
        var settings = new Dictionary<string, object>();
        string plan = Path.Combine(installDirectory, "installation-defaults.json");
        if (File.Exists(plan))
        {
            var info = new FileInfo(plan);
            if (info.Length > 65536) throw new InvalidOperationException("The installation settings file is invalid. Reinstall Local Image.");
            var defaults = new JavaScriptSerializer().Deserialize<Dictionary<string, object>>(File.ReadAllText(plan, Encoding.UTF8));
            object schema;
            if (defaults == null || !defaults.TryGetValue("schema", out schema) || !(schema is int) || (int)schema != 1)
                throw new InvalidOperationException("The installation settings format is unsupported. Reinstall Local Image.");
            foreach (string key in new[] { "model_directory", "managed_ai_directory" })
            {
                object raw;
                if (defaults.TryGetValue(key, out raw) && raw is string)
                {
                    string path = StoragePath((string)raw, installDirectory);
                    if (path.Length > 0) settings[key] = path;
                }
            }
            object mode;
            string setup = defaults.TryGetValue("setup_mode", out mode) && mode is string ? (string)mode : "discover";
            if (Array.IndexOf(new[] { "discover", "portable", "later" }, setup) < 0)
                throw new InvalidOperationException("The installation setup choice is invalid.");
            settings["setup_mode"] = setup;
        }
        else settings["setup_mode"] = "discover";
        settings["comfy_port"] = 8188;
        settings["installation_defaults_applied"] = true;
        string temporary = target + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try
        {
            File.WriteAllText(temporary, new JavaScriptSerializer().Serialize(settings), new UTF8Encoding(false));
            if (!File.Exists(target)) File.Move(temporary, target);
        }
        finally { if (File.Exists(temporary)) File.Delete(temporary); }
    }
}
