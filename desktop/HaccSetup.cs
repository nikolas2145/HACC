using System;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Threading;
using System.Windows.Forms;
using Microsoft.Win32;

namespace HaccSetup {
    static class Program {
        const string Version = "1.0.0";
        const string ProductKey = @"Software\HACC";
        const string Classes = @"Software\Classes\";
        static readonly string InstallDir = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Programs", "HACC");

        [DllImport("shell32.dll")]
        static extern void SHChangeNotify(uint eventId, uint flags, IntPtr item1, IntPtr item2);

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        static extern bool MoveFileEx(string existing, string replacement, int flags);

        [STAThread]
        static void Main(string[] args) {
            if (args.Length >= 2 && args[0] == "--extract-payload") {
                ExtractPayload(args[1]);
                return;
            }
            if (args.Length >= 1 && args[0] == "--uninstall-worker") {
                UninstallWorker();
                return;
            }
            if (args.Length >= 1 && args[0] == "--uninstall") {
                if (MessageBox.Show("Uninstall HACC 1.0 and remove its File Explorer menu entries?",
                    "HACC", MessageBoxButtons.YesNo, MessageBoxIcon.Question) == DialogResult.Yes) {
                    string temporary = Path.Combine(Path.GetTempPath(), "HaccUninstall-" + Guid.NewGuid().ToString("N") + ".exe");
                    File.Copy(Assembly.GetExecutingAssembly().Location, temporary);
                    Process.Start(new ProcessStartInfo(temporary, "--uninstall-worker") { UseShellExecute = false });
                }
                return;
            }
            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            Application.Run(new SetupForm());
        }

        public static string Target { get { return InstallDir; } }

        static void CopyResource(string name, string destination) {
            using (Stream source = Assembly.GetExecutingAssembly().GetManifestResourceStream(name)) {
                if (source == null) throw new InvalidOperationException("Installer payload is missing " + name);
                using (FileStream target = File.Create(destination)) source.CopyTo(target);
            }
        }

        static void ExtractPayload(string directory) {
            Directory.CreateDirectory(directory);
            CopyResource("Hacc.Cli", Path.Combine(directory, "hacc.exe"));
            CopyResource("Hacc.Gui", Path.Combine(directory, "HaccGui.exe"));
        }

        static void SetDefault(string path, string value) {
            using (RegistryKey key = Registry.CurrentUser.CreateSubKey(path)) key.SetValue(null, value);
        }

        static void RegisterVerb(string parent, string name, string label, string command, string icon, bool multi) {
            using (RegistryKey key = Registry.CurrentUser.CreateSubKey(Classes + parent + @"\shell\" + name)) {
                key.SetValue(null, label);
                key.SetValue("Icon", icon);
                if (multi) key.SetValue("MultiSelectModel", "Player");
                using (RegistryKey executable = key.CreateSubKey("command")) executable.SetValue(null, command);
            }
        }

        static void NotifyExplorer() {
            SHChangeNotify(0x08000000, 0, IntPtr.Zero, IntPtr.Zero);
        }

        public static void Install() {
            Directory.CreateDirectory(InstallDir);
            ExtractPayload(InstallDir);
            File.Copy(Assembly.GetExecutingAssembly().Location,
                Path.Combine(InstallDir, "HaccUninstall.exe"), true);
            string gui = Path.Combine(InstallDir, "HaccGui.exe");
            string icon = "\"" + gui + "\",0";

            using (RegistryKey settings = Registry.CurrentUser.CreateSubKey(ProductKey)) {
                if (settings.GetValue("PreviousAssociation") == null) {
                    using (RegistryKey current = Registry.CurrentUser.OpenSubKey(Classes + ".hacc")) {
                        object previous = current == null ? null : current.GetValue(null);
                        settings.SetValue("PreviousAssociation", previous == null ? "<none>" : previous.ToString());
                    }
                }
            }
            SetDefault(Classes + ".hacc", "HACC.Archive");
            SetDefault(Classes + "HACC.Archive", "HACC archive");
            SetDefault(Classes + @"HACC.Archive\DefaultIcon", icon);
            RegisterVerb("HACC.Archive", "open", "Open with HACC",
                "\"" + gui + "\" extract \"%1\"", icon, false);
            RegisterVerb(@"SystemFileAssociations\.hacc", "HACC.Extract", "Extract with HACC…",
                "\"" + gui + "\" extract \"%1\"", icon, false);
            RegisterVerb("*", "HACC.Pack", "Pack with HACC…",
                "\"" + gui + "\" pack %*", icon, true);
            RegisterVerb("Directory", "HACC.Pack", "Pack with HACC…",
                "\"" + gui + "\" pack %*", icon, true);

            using (RegistryKey key = Registry.CurrentUser.CreateSubKey(
                @"Software\Microsoft\Windows\CurrentVersion\Uninstall\HACC")) {
                key.SetValue("DisplayName", "HACC 1.0");
                key.SetValue("DisplayVersion", Version);
                key.SetValue("Publisher", "HACC");
                key.SetValue("InstallLocation", InstallDir);
                key.SetValue("DisplayIcon", gui);
                key.SetValue("UninstallString", "\"" + Path.Combine(InstallDir, "HaccUninstall.exe") + "\" --uninstall");
                key.SetValue("NoModify", 1, RegistryValueKind.DWord);
                key.SetValue("NoRepair", 1, RegistryValueKind.DWord);
            }
            CreateStartMenuShortcut(gui);
            NotifyExplorer();
        }

        static string ShortcutPath() {
            string programs = Environment.GetFolderPath(Environment.SpecialFolder.Programs);
            return Path.Combine(programs, "HACC.lnk");
        }

        static void CreateStartMenuShortcut(string gui) {
            Type shellType = Type.GetTypeFromProgID("WScript.Shell");
            if (shellType == null) return;
            dynamic shell = Activator.CreateInstance(shellType);
            dynamic shortcut = shell.CreateShortcut(ShortcutPath());
            shortcut.TargetPath = gui;
            shortcut.WorkingDirectory = InstallDir;
            shortcut.Description = "HACC 1.0 — GPU archiver";
            shortcut.Save();
        }

        static void RemoveKey(string path) {
            Registry.CurrentUser.DeleteSubKeyTree(path, false);
        }

        static void UninstallWorker() {
            try {
                bool removed = false;
                for (int attempt = 0; attempt < 20 && !removed; ++attempt) {
                    Thread.Sleep(250);
                    try {
                        if (Directory.Exists(InstallDir)) Directory.Delete(InstallDir, true);
                        removed = true;
                    } catch (IOException) { }
                    catch (UnauthorizedAccessException) { }
                }
                if (!removed) throw new IOException("Close HACC and try uninstalling again.");

                RemoveKey(Classes + @"SystemFileAssociations\.hacc\shell\HACC.Extract");
                RemoveKey(Classes + @"*\shell\HACC.Pack");
                RemoveKey(Classes + @"Directory\shell\HACC.Pack");
                RemoveKey(Classes + "HACC.Archive");
                using (RegistryKey settings = Registry.CurrentUser.OpenSubKey(ProductKey)) {
                    string previous = settings == null ? "<none>" : settings.GetValue("PreviousAssociation", "<none>").ToString();
                    using (RegistryKey extension = Registry.CurrentUser.OpenSubKey(Classes + ".hacc", true)) {
                        if (extension != null && Convert.ToString(extension.GetValue(null)) == "HACC.Archive") {
                            if (previous == "<none>") extension.DeleteValue("", false);
                            else extension.SetValue(null, previous);
                        }
                    }
                }
                RemoveKey(ProductKey);
                RemoveKey(@"Software\Microsoft\Windows\CurrentVersion\Uninstall\HACC");
                if (File.Exists(ShortcutPath())) File.Delete(ShortcutPath());
                NotifyExplorer();
                MessageBox.Show("HACC has been uninstalled.", "HACC", MessageBoxButtons.OK, MessageBoxIcon.Information);
            } catch (Exception error) {
                MessageBox.Show(error.Message, "Uninstall HACC", MessageBoxButtons.OK, MessageBoxIcon.Error);
            } finally {
                MoveFileEx(Assembly.GetExecutingAssembly().Location, null, 4);
            }
        }

        sealed class SetupForm : Form {
            readonly Button installButton = new Button();
            readonly Label status = new Label();

            public SetupForm() {
                Text = "Install HACC 1.0";
                Width = 590;
                Height = 380;
                MinimumSize = MaximumSize = Size;
                StartPosition = FormStartPosition.CenterScreen;
                Font = new Font("Segoe UI", 10);
                BackColor = Color.White;

                var header = new Panel { Dock = DockStyle.Top, Height = 95, BackColor = Color.FromArgb(39, 86, 184) };
                header.Controls.Add(new Label {
                    Text = "HACC 1.0", ForeColor = Color.White,
                    Font = new Font("Segoe UI", 25, FontStyle.Bold),
                    Location = new Point(26, 19), AutoSize = true
                });
                Controls.Add(header);
                Controls.Add(new Label {
                    Text = "Fast file compression and extraction with NVIDIA GPU support.",
                    Location = new Point(28, 118), AutoSize = true
                });
                Controls.Add(new Label {
                    Text = "Install for the current user:\n" + InstallDir +
                           "\n\nAdds HACC to the Start menu and File Explorer:\n" +
                           "• Extract .hacc archives\n• Pack selected files or folders",
                    Location = new Point(28, 151), Size = new Size(520, 119)
                });
                installButton.Text = "Install";
                installButton.Location = new Point(28, 281);
                installButton.Size = new Size(160, 38);
                installButton.BackColor = Color.FromArgb(39, 86, 184);
                installButton.ForeColor = Color.White;
                installButton.FlatStyle = FlatStyle.Flat;
                installButton.FlatAppearance.BorderSize = 0;
                installButton.Click += delegate {
                    installButton.Enabled = false;
                    try {
                        Install();
                        status.Text = "Done. Launch HACC from the Start menu.";
                        MessageBox.Show(this, "HACC 1.0 has been installed.", "HACC",
                            MessageBoxButtons.OK, MessageBoxIcon.Information);
                    } catch (Exception error) {
                        status.Text = "Installation failed.";
                        MessageBox.Show(this, error.Message, "HACC", MessageBoxButtons.OK, MessageBoxIcon.Error);
                        installButton.Enabled = true;
                    }
                };
                Controls.Add(installButton);
                status.Location = new Point(203, 291);
                status.AutoSize = true;
                Controls.Add(status);
            }
        }
    }
}
