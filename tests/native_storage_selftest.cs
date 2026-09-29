using System;
using System.Collections.Generic;
using System.IO;
using System.Text;
using System.Web.Script.Serialization;

internal static class StorageSelfTest
{
    private static int checks;
    private static void Check(bool condition, string message)
    {
        if (!condition) throw new Exception(message);
        checks++;
    }
    public static int Main()
    {
        string temporary = Path.Combine(Path.GetTempPath(), "local-image-storage-test-" + Guid.NewGuid().ToString("N"));
        string install = Path.Combine(temporary, "Program Files", "Local Image");
        string appData = Path.Combine(temporary, "Another User", "AppData", "Local");
        Directory.CreateDirectory(install);
        try
        {
            var json = new JavaScriptSerializer();
            string fresh = LocalImageStorage.ResolveProfileRoot(appData, null);
            Check(fresh == Path.Combine(appData, "Local Image"), "Fresh profile does not use Local Image AppData");
            Check(LocalImageStorage.ResolveProfileRoot(appData + "-second-user", null) != fresh, "Two users share a profile");
            var plan = new Dictionary<string, object> { { "schema", 1 }, { "setup_mode", "portable" },
                { "model_directory", Path.Combine(temporary, "Models café 水彩") },
                { "managed_ai_directory", Path.Combine(temporary, "Portable AI") } };
            File.WriteAllText(Path.Combine(install, "installation-defaults.json"), json.Serialize(plan), new UTF8Encoding(false));
            LocalImageStorage.InitializeProfile(fresh, install);
            var config = json.Deserialize<Dictionary<string, object>>(File.ReadAllText(Path.Combine(fresh, "config.json")));
            Check((string)config["model_directory"] == (string)plan["model_directory"], "Unicode model path corrupted");
            Check((string)config["managed_ai_directory"] == (string)plan["managed_ai_directory"], "Portable destination missing");
            Check((string)config["setup_mode"] == "portable", "Initial setup preference missing");
            string before = File.ReadAllText(Path.Combine(fresh, "config.json"));
            plan["model_directory"] = Path.Combine(temporary, "Different Models");
            File.WriteAllText(Path.Combine(install, "installation-defaults.json"), json.Serialize(plan));
            LocalImageStorage.InitializeProfile(fresh, install);
            Check(File.ReadAllText(Path.Combine(fresh, "config.json")) == before, "Update replaced user settings");
            string legacyBase = appData + "-legacy";
            string legacy = Path.Combine(legacyBase, "Local Remove");
            Directory.CreateDirectory(Path.Combine(legacy, "state"));
            Check(LocalImageStorage.ResolveProfileRoot(legacyBase, null) == legacy, "Legacy recovery not retained");
            Directory.CreateDirectory(Path.Combine(legacyBase, "Local Image", "state"));
            Check(LocalImageStorage.ResolveProfileRoot(legacyBase, null) == Path.Combine(legacyBase, "Local Image"), "Current profile lost priority");
            Check(LocalImageStorage.ResolveProfileRoot(appData, Path.Combine(temporary, "Explicit Profile")) == Path.Combine(temporary, "Explicit Profile"), "Explicit test profile ignored");
            bool rejected = false;
            try { LocalImageStorage.StoragePath(Path.Combine(install, "models"), install); } catch (InvalidOperationException) { rejected = true; }
            Check(rejected, "Mutable storage accepted under application directory");
            rejected = false;
            try { LocalImageStorage.StoragePath(@"\\server\models", install); } catch (InvalidOperationException) { rejected = true; }
            Check(rejected, "Network storage unexpectedly accepted");
            Check(LocalImageStorage.StoragePath(@"D:\", install) == @"D:\", "Drive-root separator lost");
            foreach (var invalid in new[] { @"D:relative", @"\without-drive", "relative" })
            {
                rejected = false;
                try { LocalImageStorage.StoragePath(invalid, install); } catch (InvalidOperationException) { rejected = true; }
                Check(rejected, "Drive-relative path accepted");
            }
            LocalImageStorage.CheckWritable(Path.Combine(temporary, "Not created", "Model folder"));
            Check(!Directory.Exists(Path.Combine(temporary, "Not created")), "Storage probe created selected folders");
            Check(Directory.GetFiles(install).Length == 1, "Profile initialization changed installation files");
            Console.WriteLine(json.Serialize(new { ok = true, checks = checks, legacy_preserved = true,
                unicode_paths = true, per_user_profiles = true, installer_defaults_once = true }));
            return 0;
        }
        finally
        {
            string resolved = Path.GetFullPath(temporary);
            if (Path.GetDirectoryName(resolved) == Path.GetTempPath().TrimEnd(Path.DirectorySeparatorChar)
                && Path.GetFileName(resolved).StartsWith("local-image-storage-test-")) Directory.Delete(resolved, true);
        }
    }
}
