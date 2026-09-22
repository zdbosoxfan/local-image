using System;
using System.Collections.Generic;
using System.Drawing;
using System.IO;
using System.Text;
using System.Windows.Forms;

internal sealed class LocalRemoveSettings : Form
{
    private readonly NumericUpDown port;
    private readonly TextBox models;

    internal LocalRemoveSettings()
    {
        Text = "Local Remove - AI connection";
        StartPosition = FormStartPosition.CenterScreen;
        ClientSize = new Size(570, 285);
        FormBorderStyle = FormBorderStyle.FixedDialog;
        MaximizeBox = false; MinimizeBox = false;
        Font = new Font("Segoe UI", 10);
        var explanation = new Label { Text = "AI Remove connects to ComfyUI running on this PC.\nQuick Heal works without this connection.",
            Location = new Point(20, 20), Size = new Size(530, 50) };
        Controls.Add(explanation);
        Controls.Add(new Label { Text = "ComfyUI port", Location = new Point(20, 86), AutoSize = true });
        port = new NumericUpDown { Location = new Point(190, 82), Width = 110, Minimum = 1, Maximum = 65535, Value = 8188 };
        Controls.Add(port);
        Controls.Add(new Label { Text = "FLUX model folder", Location = new Point(20, 132), AutoSize = true });
        models = new TextBox { Location = new Point(20, 159), Width = 425 };
        Controls.Add(models);
        var browse = new Button { Text = "Browse...", Location = new Point(454, 157), Width = 96, Height = 30 };
        browse.Click += delegate {
            using (var picker = new FolderBrowserDialog { Description = "Choose the model folder containing diffusion_models, text_encoders, and vae.", ShowNewFolderButton = false })
                if (picker.ShowDialog(this) == DialogResult.OK) models.Text = picker.SelectedPath;
        };
        Controls.Add(browse);
        var cancel = new Button { Text = "Cancel", Location = new Point(340, 226), Size = new Size(100, 34), DialogResult = DialogResult.Cancel };
        var save = new Button { Text = "Save", Location = new Point(450, 226), Size = new Size(100, 34) };
        save.Click += Save;
        Controls.Add(cancel); Controls.Add(save); CancelButton = cancel; AcceptButton = save;
        string file = Path.Combine(LocalRemoveLauncher.DataDirectory, "config.json");
        if (File.Exists(file))
        {
            try {
                var settings = LocalRemoveLauncher.Json.Deserialize<Dictionary<string, object>>(File.ReadAllText(file));
                object value;
                if (settings.TryGetValue("comfy_port", out value)) port.Value = Math.Max(1, Math.Min(65535, Convert.ToInt32(value)));
                models.Text = LocalRemoveLauncher.StringValue(settings, "model_directory", "");
            } catch (Exception error) { LocalRemoveLauncher.WriteError(error); }
        }
    }

    private void Save(object sender, EventArgs args)
    {
        try
        {
            if ((int)port.Value == 51247) throw new InvalidOperationException("This port is used by Local Remove. Enter the port shown by ComfyUI.");
            string folder = models.Text.Trim();
            if (folder.Length > 0 && !Directory.Exists(folder)) throw new InvalidOperationException("Choose an existing model folder, or leave it blank to use Quick Heal only.");
            Directory.CreateDirectory(LocalRemoveLauncher.DataDirectory);
            string target = Path.Combine(LocalRemoveLauncher.DataDirectory, "config.json");
            // Preserve installation and setup fields maintained by the guided setup service.
            var settings = File.Exists(target)
                ? LocalRemoveLauncher.Json.Deserialize<Dictionary<string, object>>(File.ReadAllText(target))
                : new Dictionary<string, object>();
            if (settings == null) settings = new Dictionary<string, object>();
            settings["comfy_port"] = (int)port.Value;
            settings["model_directory"] = folder;
            string temporary = target + "." + Guid.NewGuid().ToString("N") + ".tmp";
            try
            {
                File.WriteAllText(temporary, LocalRemoveLauncher.Json.Serialize(settings), new UTF8Encoding(false));
                if (File.Exists(target)) File.Replace(temporary, target, null); else File.Move(temporary, target);
            }
            finally { if (File.Exists(temporary)) File.Delete(temporary); }
            DialogResult = DialogResult.OK; Close();
        }
        catch (Exception error) { MessageBox.Show(this, error.Message, "AI connection", MessageBoxButtons.OK, MessageBoxIcon.Information); }
    }
}
