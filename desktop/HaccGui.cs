using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Text;
using System.Threading.Tasks;
using System.Windows.Forms;

namespace HaccDesktop {
    static class Program {
        [STAThread]
        static void Main(string[] args) {
            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            Application.Run(new MainForm(args));
        }
    }

    sealed class MainForm : Form {
        readonly TabControl tabs = new TabControl();
        readonly ListBox inputs = new ListBox();
        readonly TextBox archiveOut = new TextBox();
        readonly TextBox archiveIn = new TextBox();
        readonly TextBox extractOut = new TextBox();
        readonly RadioButton gpu = new RadioButton();
        readonly RadioButton cpu = new RadioButton();
        readonly Button packButton = new Button();
        readonly Button extractButton = new Button();
        readonly ProgressBar progress = new ProgressBar();
        readonly Label status = new Label();
        readonly Color accent = Color.FromArgb(39, 86, 184);

        public MainForm(string[] args) {
            Text = "HACC 1.0 — Archiver";
            Width = 760;
            Height = 645;
            MinimumSize = new Size(690, 610);
            StartPosition = FormStartPosition.CenterScreen;
            Font = new Font("Segoe UI", 9.5f);
            BackColor = Color.White;

            var title = new Panel { Dock = DockStyle.Top, Height = 78, BackColor = accent };
            title.Controls.Add(new Label {
                Text = "HACC", ForeColor = Color.White, Font = new Font("Segoe UI", 25, FontStyle.Bold),
                Location = new Point(22, 8), AutoSize = true
            });
            title.Controls.Add(new Label {
                Text = "Fast GPU compression  •  Version 1.0", ForeColor = Color.White,
                Font = new Font("Segoe UI", 10), Location = new Point(25, 49), AutoSize = true
            });
            Controls.Add(title);

            var footer = new Panel { Dock = DockStyle.Bottom, Height = 62, BackColor = Color.FromArgb(246, 248, 252) };
            progress.Location = new Point(20, 9);
            progress.Width = 700;
            progress.Anchor = AnchorStyles.Left | AnchorStyles.Right | AnchorStyles.Top;
            progress.Visible = false;
            footer.Controls.Add(progress);
            status.Text = "Ready";
            status.Location = new Point(20, 36);
            status.AutoSize = true;
            footer.Controls.Add(status);
            Controls.Add(footer);

            tabs.Dock = DockStyle.Fill;
            tabs.TabPages.Add(CreatePackTab());
            tabs.TabPages.Add(CreateExtractTab());
            Controls.Add(tabs);
            tabs.BringToFront();

            if (args.Length >= 2 && args[0].Equals("extract", StringComparison.OrdinalIgnoreCase)) {
                tabs.SelectedIndex = 1;
                archiveIn.Text = args[1];
                SuggestExtractOutput();
            } else if (args.Length >= 2 && args[0].Equals("pack", StringComparison.OrdinalIgnoreCase)) {
                AddInputs(SubArray(args, 1));
            } else if (args.Length == 1 && File.Exists(args[0]) &&
                       args[0].EndsWith(".hacc", StringComparison.OrdinalIgnoreCase)) {
                tabs.SelectedIndex = 1;
                archiveIn.Text = args[0];
                SuggestExtractOutput();
            }
        }

        static string[] SubArray(string[] values, int start) {
            var result = new string[values.Length - start];
            Array.Copy(values, start, result, 0, result.Length);
            return result;
        }

        TabPage CreatePackTab() {
            var page = new TabPage("Pack") { BackColor = Color.White, Padding = new Padding(20) };
            page.Controls.Add(new Label { Text = "Files and folders", Location = new Point(20, 20), AutoSize = true, Font = new Font(Font, FontStyle.Bold) });
            inputs.Location = new Point(20, 48);
            inputs.Size = new Size(580, 168);
            inputs.SelectionMode = SelectionMode.MultiExtended;
            inputs.AllowDrop = true;
            inputs.DragEnter += delegate(object sender, DragEventArgs e) {
                e.Effect = e.Data.GetDataPresent(DataFormats.FileDrop) ? DragDropEffects.Copy : DragDropEffects.None;
            };
            inputs.DragDrop += delegate(object sender, DragEventArgs e) {
                AddInputs((string[])e.Data.GetData(DataFormats.FileDrop));
            };
            page.Controls.Add(inputs);
            var addFiles = MakeButton("Add files", 610, 48, 120);
            addFiles.Click += delegate { ChooseFiles(); };
            page.Controls.Add(addFiles);
            var addFolder = MakeButton("Add folder", 610, 88, 120);
            addFolder.Click += delegate { ChooseFolder(); };
            page.Controls.Add(addFolder);
            var remove = MakeButton("Remove", 610, 128, 120);
            remove.Click += delegate {
                while (inputs.SelectedIndices.Count > 0) inputs.Items.RemoveAt(inputs.SelectedIndices[0]);
            };
            page.Controls.Add(remove);

            page.Controls.Add(new Label { Text = "Output archive", Location = new Point(20, 233), AutoSize = true });
            archiveOut.Location = new Point(20, 258);
            archiveOut.Width = 580;
            page.Controls.Add(archiveOut);
            var browse = MakeButton("Browse…", 610, 255, 120);
            browse.Click += delegate {
                using (var dialog = new SaveFileDialog()) {
                    dialog.Filter = "HACC archive (*.hacc)|*.hacc";
                    dialog.DefaultExt = "hacc";
                    dialog.FileName = Path.GetFileName(archiveOut.Text);
                    if (dialog.ShowDialog(this) == DialogResult.OK) archiveOut.Text = dialog.FileName;
                }
            };
            page.Controls.Add(browse);

            gpu.Text = "GPU — fast NVIDIA compression";
            gpu.Location = new Point(20, 305);
            gpu.AutoSize = true;
            gpu.Checked = true;
            page.Controls.Add(gpu);
            cpu.Text = "CPU — no GPU required";
            cpu.Location = new Point(20, 334);
            cpu.AutoSize = true;
            page.Controls.Add(cpu);
            packButton.Text = "Create archive";
            packButton.Location = new Point(20, 376);
            packButton.Size = new Size(175, 38);
            StylePrimary(packButton);
            packButton.Click += async delegate { await PackAsync(); };
            page.Controls.Add(packButton);
            page.SizeChanged += delegate {
                int right = page.ClientSize.Width - 140;
                int fieldWidth = right - 30;
                inputs.Width = fieldWidth;
                archiveOut.Width = fieldWidth;
                addFiles.Left = right;
                addFolder.Left = right;
                remove.Left = right;
                browse.Left = right;
            };
            return page;
        }

        TabPage CreateExtractTab() {
            var page = new TabPage("Extract") { BackColor = Color.White, Padding = new Padding(20) };
            page.Controls.Add(new Label { Text = ".hacc archive", Location = new Point(20, 35), AutoSize = true });
            archiveIn.Location = new Point(20, 63);
            archiveIn.Width = 580;
            archiveIn.Leave += delegate { if (extractOut.TextLength == 0) SuggestExtractOutput(); };
            page.Controls.Add(archiveIn);
            var chooseArchive = MakeButton("Browse…", 610, 60, 120);
            chooseArchive.Click += delegate {
                using (var dialog = new OpenFileDialog()) {
                    dialog.Filter = "HACC archive (*.hacc)|*.hacc";
                    if (dialog.ShowDialog(this) == DialogResult.OK) {
                        archiveIn.Text = dialog.FileName;
                        SuggestExtractOutput();
                    }
                }
            };
            page.Controls.Add(chooseArchive);

            page.Controls.Add(new Label { Text = "Destination folder", Location = new Point(20, 115), AutoSize = true });
            extractOut.Location = new Point(20, 143);
            extractOut.Width = 580;
            page.Controls.Add(extractOut);
            var chooseOutput = MakeButton("Browse…", 610, 140, 120);
            chooseOutput.Click += delegate {
                using (var dialog = new FolderBrowserDialog()) {
                    dialog.Description = "Choose the destination folder";
                    if (dialog.ShowDialog(this) == DialogResult.OK) extractOut.Text = dialog.SelectedPath;
                }
            };
            page.Controls.Add(chooseOutput);

            page.Controls.Add(new Label {
                Text = "CRC32 is verified for every block during extraction.",
                Location = new Point(20, 198), AutoSize = true, ForeColor = Color.DimGray
            });
            extractButton.Text = "Extract archive";
            extractButton.Location = new Point(20, 247);
            extractButton.Size = new Size(175, 38);
            StylePrimary(extractButton);
            extractButton.Click += async delegate { await ExtractAsync(); };
            page.Controls.Add(extractButton);
            page.SizeChanged += delegate {
                int right = page.ClientSize.Width - 140;
                int fieldWidth = right - 30;
                archiveIn.Width = fieldWidth;
                extractOut.Width = fieldWidth;
                chooseArchive.Left = right;
                chooseOutput.Left = right;
            };
            return page;
        }

        Button MakeButton(string text, int x, int y, int width) {
            return new Button { Text = text, Location = new Point(x, y), Size = new Size(width, 33) };
        }

        void StylePrimary(Button button) {
            button.BackColor = accent;
            button.ForeColor = Color.White;
            button.FlatStyle = FlatStyle.Flat;
            button.FlatAppearance.BorderSize = 0;
            button.Font = new Font(Font, FontStyle.Bold);
        }

        void AddInputs(string[] paths) {
            foreach (string path in paths) {
                if ((File.Exists(path) || Directory.Exists(path)) && !inputs.Items.Contains(path)) inputs.Items.Add(path);
            }
            if (inputs.Items.Count > 0 && archiveOut.TextLength == 0) {
                string first = inputs.Items[0].ToString();
                string name = Directory.Exists(first) ? Path.GetFileName(first.TrimEnd('\\')) : Path.GetFileNameWithoutExtension(first);
                if (inputs.Items.Count > 1) name = "Selection";
                archiveOut.Text = Path.Combine(Path.GetDirectoryName(first), name + ".hacc");
            }
        }

        void ChooseFiles() {
            using (var dialog = new OpenFileDialog()) {
                dialog.Multiselect = true;
                dialog.Filter = "All files (*.*)|*.*";
                if (dialog.ShowDialog(this) == DialogResult.OK) AddInputs(dialog.FileNames);
            }
        }

        void ChooseFolder() {
            using (var dialog = new FolderBrowserDialog()) {
                dialog.Description = "Choose a folder to pack";
                if (dialog.ShowDialog(this) == DialogResult.OK) AddInputs(new[] { dialog.SelectedPath });
            }
        }

        void SuggestExtractOutput() {
            if (File.Exists(archiveIn.Text)) {
                extractOut.Text = Path.Combine(Path.GetDirectoryName(archiveIn.Text),
                    Path.GetFileNameWithoutExtension(archiveIn.Text));
            }
        }

        async Task PackAsync() {
            if (inputs.Items.Count == 0) { MessageBox.Show(this, "Add a file or folder."); return; }
            if (String.IsNullOrWhiteSpace(archiveOut.Text)) { MessageBox.Show(this, "Choose an output archive."); return; }
            if (File.Exists(archiveOut.Text) && MessageBox.Show(this, "The archive already exists. Overwrite it?",
                "HACC", MessageBoxButtons.YesNo, MessageBoxIcon.Warning) != DialogResult.Yes) return;
            var arguments = new List<string> { "a", gpu.Checked ? "-g" : "-l3", archiveOut.Text };
            foreach (object item in inputs.Items) arguments.Add(item.ToString());
            await RunAsync(arguments);
        }

        async Task ExtractAsync() {
            if (!File.Exists(archiveIn.Text)) { MessageBox.Show(this, "Choose a valid .hacc archive."); return; }
            if (String.IsNullOrWhiteSpace(extractOut.Text)) { MessageBox.Show(this, "Choose a destination folder."); return; }
            if (Directory.Exists(extractOut.Text) && Directory.GetFileSystemEntries(extractOut.Text).Length > 0 &&
                MessageBox.Show(this, "The destination folder is not empty. Continue?", "HACC",
                    MessageBoxButtons.YesNo, MessageBoxIcon.Warning) != DialogResult.Yes) return;
            await RunAsync(new List<string> { "x", archiveIn.Text, extractOut.Text });
        }

        async Task RunAsync(List<string> arguments) {
            packButton.Enabled = false;
            extractButton.Enabled = false;
            progress.Visible = true;
            progress.Style = ProgressBarStyle.Marquee;
            status.Text = "Working…";
            try {
                string result = await Task.Run(() => RunCli(arguments));
                status.Text = "Done";
                MessageBox.Show(this, result, "HACC", MessageBoxButtons.OK, MessageBoxIcon.Information);
            } catch (Exception error) {
                status.Text = "Operation failed";
                MessageBox.Show(this, error.Message, "HACC", MessageBoxButtons.OK, MessageBoxIcon.Error);
            } finally {
                packButton.Enabled = true;
                extractButton.Enabled = true;
                progress.Visible = false;
            }
        }

        static string RunCli(List<string> args) {
            string binary = Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "hacc.exe");
            if (!File.Exists(binary)) throw new FileNotFoundException("hacc.exe was not found next to the GUI.", binary);
            var start = new ProcessStartInfo(binary) {
                Arguments = JoinArguments(args), UseShellExecute = false, CreateNoWindow = true,
                RedirectStandardOutput = true, RedirectStandardError = true,
                StandardOutputEncoding = Encoding.UTF8, StandardErrorEncoding = Encoding.UTF8
            };
            using (var process = Process.Start(start)) {
                string output = process.StandardOutput.ReadToEnd();
                string error = process.StandardError.ReadToEnd();
                process.WaitForExit();
                if (process.ExitCode != 0) throw new InvalidOperationException(String.IsNullOrWhiteSpace(error) ? output : error);
                return output.Trim();
            }
        }

        static string JoinArguments(List<string> args) {
            var result = new StringBuilder();
            foreach (string arg in args) {
                if (result.Length > 0) result.Append(' ');
                result.Append(Quote(arg));
            }
            return result.ToString();
        }

        static string Quote(string text) {
            var result = new StringBuilder("\"");
            int backslashes = 0;
            foreach (char c in text) {
                if (c == '\\') { ++backslashes; continue; }
                if (c == '"') {
                    result.Append('\\', backslashes * 2 + 1);
                    result.Append('"');
                } else {
                    result.Append('\\', backslashes);
                    result.Append(c);
                }
                backslashes = 0;
            }
            result.Append('\\', backslashes * 2);
            result.Append('"');
            return result.ToString();
        }
    }
}
