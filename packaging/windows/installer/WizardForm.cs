using System.Windows.Forms;

namespace NovaFlashStudio.Setup;

/// <summary>Base class for wizard pages.</summary>
internal abstract class WizardPage : UserControl
{
    public abstract string Title { get; }
    public virtual string Subtitle => "";

    /// <summary>Validate before leaving the page; returns error text or null.</summary>
    public virtual string? ValidateInput() => null;

    /// <summary>Called when the page becomes current.</summary>
    public virtual void OnEnter() { }

    protected Label MakeLabel(string text, float size = 9.5f, bool bold = false)
    {
        return new Label
        {
            Text = text,
            AutoSize = true,
            Font = new Font("Segoe UI", size, bold ? FontStyle.Bold : FontStyle.Regular),
            ForeColor = Color.Gainsboro,
        };
    }

    protected void AddControl(Control control, int top)
    {
        control.Top = top;
        control.Left = 24;
        control.Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right;
        Controls.Add(control);
    }
}

/// <summary>Page 1: welcome / product info.</summary>
internal sealed class WelcomePage : WizardPage
{
    public override string Title => "Willkommen";
    public override string Subtitle => "NØVA Flash Studio installieren";

    public WelcomePage()
    {
        var logo = MessageBoxCompat.LoadLogo();
        if (logo is not null)
        {
            var picture = new PictureBox
            {
                Image = logo,
                SizeMode = PictureBoxSizeMode.Zoom,
                Width = 96,
                Height = 96,
                Top = 30,
                Left = 24,
            };
            Controls.Add(picture);
        }

        AddControl(MakeLabel("NØVA Flash Studio", 16f, bold: true), 140);
        AddControl(MakeLabel($"Version {AssemblyVersion()}", 10f), 172);
        AddControl(MakeLabel(
            "Der Assistent installiert NØVA Flash Studio auf diesem Computer.\n" +
            "Standard ist die Installation nur für diesen Benutzer\n" +
            "(%LOCALAPPDATA%, keine Admin-Rechte). Optional ist eine\n" +
            "Installation für alle Benutzer (UAC-Abfrage) oder ein\n" +
            "eigener Installationsordner möglich.\n\n" +
            "Ihre Projekte und Einstellungen werden niemals gelöscht.", 9.5f), 210);
    }

    private static string AssemblyVersion()
    {
        var attr = typeof(Installer).Assembly
            .GetCustomAttributes(typeof(System.Reflection.AssemblyInformationalVersionAttribute), false);
        if (attr.Length > 0)
        {
            var v = ((System.Reflection.AssemblyInformationalVersionAttribute)attr[0]).InformationalVersion;
            if (!string.IsNullOrWhiteSpace(v)) return v;
        }
        return "0.0.0";
    }
}

/// <summary>Page 2: license agreement.</summary>
internal sealed class LicensePage : WizardPage
{
    public override string Title => "Lizenzvertrag";
    public override string Subtitle => "Bitte lesen Sie die Lizenzbedingungen";

    private readonly RadioButton _accept = new() { Text = "Ich akzeptiere den Lizenzvertrag", AutoSize = true };
    private readonly RadioButton _decline = new() { Text = "Ich akzeptiere nicht", AutoSize = true };
    private readonly Button _next;

    public bool Accepted => _accept.Checked;

    public LicensePage(Button nextButton)
    {
        _next = nextButton;
        var text = Installer.ReadEulaText() ?? "(EULA-Text nicht gefunden)";
        var box = new TextBox
        {
            Multiline = true,
            ReadOnly = true,
            ScrollBars = ScrollBars.Vertical,
            Dock = DockStyle.Fill,
            BackColor = Color.White,
            ForeColor = Color.Black,
            Font = new Font("Consolas", 9f),
            Text = text,
        };
        Controls.Add(box);

        var bottom = new Panel { Dock = DockStyle.Bottom, Height = 44, BackColor = Color.Transparent };
        _accept.Left = 8; _accept.Top = 10; _accept.ForeColor = Color.Gainsboro;
        _decline.Left = 280; _decline.Top = 10; _decline.ForeColor = Color.Gainsboro;
        bottom.Controls.Add(_accept);
        bottom.Controls.Add(_decline);
        Controls.Add(bottom);
        box.BringToFront();
        bottom.BringToFront();

        _accept.CheckedChanged += (_, _) => _next.Enabled = _accept.Checked;
        _decline.CheckedChanged += (_, _) => _next.Enabled = false;
        _accept.Checked = true;
        _next.Enabled = true;
    }

    public override void OnEnter()
    {
        _next.Enabled = _accept.Checked;
    }
}

/// <summary>Page 3: install scope and directory.</summary>
internal sealed class ScopePage : WizardPage
{
    public override string Title => "Installationsort";
    public override string Subtitle => "Für wen und wohin installieren?";

    private readonly RadioButton _perUser = new() { AutoSize = true, Text = "Nur für mich (empfohlen)" };
    private readonly RadioButton _allUsers = new() { AutoSize = true, Text = "Für alle Benutzer (Administratorrechte erforderlich)" };
    private readonly TextBox _dir = new() { Width = 420 };
    private readonly Button _browse = new() { Text = "Durchsuchen …", Width = 110 };
    private readonly Label _adminHint = new() { AutoSize = true, ForeColor = Color.Orange };
    private readonly Label _existing = new() { AutoSize = true, ForeColor = Color.SkyBlue, MaximumSize = new Size(620, 0) };
    private readonly CheckBox _migrate = new()
    {
        AutoSize = true,
        ForeColor = Color.Gainsboro,
        Text = "Vorhandene Installation migrieren (wird zuerst entfernt; Projekte und Einstellungen bleiben erhalten)",
    };

    public InstallScope Scope => _allUsers.Checked ? InstallScope.AllUsers : InstallScope.PerUser;
    public string InstallDir => _dir.Text.Trim();
    /// <summary>True when the user confirmed migrating a conflicting install.</summary>
    public bool MigrationRequested => _migrate.Visible && _migrate.Checked;

    private readonly List<Installer.ExistingInstall> _existingInstalls;

    public ScopePage(IReadOnlyList<Installer.ExistingInstall> existing)
    {
        _existingInstalls = existing.ToList();
        int top = 24;
        _perUser.Top = top; _perUser.Left = 24; _perUser.ForeColor = Color.Gainsboro;
        _allUsers.Top = top + 28; _allUsers.Left = 24; _allUsers.ForeColor = Color.Gainsboro;
        Controls.Add(_perUser); Controls.Add(_allUsers);

        var dirLabel = MakeLabel("Installationsordner:", 9.5f, bold: true);
        dirLabel.Top = top + 70; dirLabel.Left = 24;
        Controls.Add(dirLabel);

        _dir.Top = top + 94; _dir.Left = 24;
        _dir.ForeColor = Color.Black;
        _dir.BackColor = Color.White;
        Controls.Add(_dir);

        _browse.Top = _dir.Top; _browse.Left = _dir.Right + 8;
        _browse.FlatStyle = FlatStyle.System;
        Controls.Add(_browse);

        _adminHint.Top = top + 128; _adminHint.Left = 24;
        _adminHint.Text = _allUsers.Checked
            ? "Die Installation für alle Benutzer erfordert Administratorrechte: " +
              "Beim Start der Installation erscheint der normale Windows-UAC-Dialog. " +
              "Bei Ablehnung wird nichts installiert."
            : "Die Installation nur für diesen Benutzer benötigt keine " +
              "Administratorrechte und installiert nach %LOCALAPPDATA%\\Programs.";
        Controls.Add(_adminHint);

        _existing.Top = top + 156; _existing.Left = 24;
        Controls.Add(_existing);

        _migrate.Top = top + 190; _migrate.Left = 24; _migrate.Width = 620;
        _migrate.Visible = false;
        Controls.Add(_migrate);

        // Default: normal per-user installation, no UAC required. If an
        // existing installation is found, pre-select its scope and directory
        // so the wizard upgrades it instead of creating a parallel install.
        var sameDirUser = _existingInstalls.FirstOrDefault(e => e.Scope == "user");
        var sameDirMachine = _existingInstalls.FirstOrDefault(e => e.Scope == "machine");
        if (sameDirUser is not null)
        {
            _perUser.Checked = true;
            _dir.Text = sameDirUser.InstallDir;
        }
        else if (sameDirMachine is not null)
        {
            _allUsers.Checked = true;
            _dir.Text = sameDirMachine.InstallDir;
        }
        else
        {
            _dir.Text = Installer.DefaultInstallDir(InstallScope.PerUser);
            _perUser.Checked = true;
        }

        _perUser.CheckedChanged += (_, _) => OnScopeChanged();
        _allUsers.CheckedChanged += (_, _) => OnScopeChanged();
        _browse.Click += (_, _) => Browse();
        RefreshExistingInfo();
    }

    private string SelectedScopeName => Scope == InstallScope.AllUsers ? "machine" : "user";

    private static bool SameDir(string a, string b)
    {
        try
        {
            var pa = Path.GetFullPath(a).TrimEnd(Path.DirectorySeparatorChar);
            var pb = Path.GetFullPath(b).TrimEnd(Path.DirectorySeparatorChar);
            return string.Equals(pa, pb, StringComparison.OrdinalIgnoreCase);
        }
        catch (Exception) { return false; }
    }

    /// <summary>Existing installs that match the currently selected scope+dir (upgrade targets).</summary>
    private List<Installer.ExistingInstall> UpgradeTargets() =>
        _existingInstalls.Where(e =>
            string.Equals(e.Scope, SelectedScopeName, StringComparison.OrdinalIgnoreCase)
            && SameDir(e.InstallDir, _dir.Text)).ToList();

    /// <summary>Existing installs that conflict with the current selection (migration candidates).</summary>
    private List<Installer.ExistingInstall> Conflicts() =>
        _existingInstalls.Where(e =>
            !string.Equals(e.Scope, SelectedScopeName, StringComparison.OrdinalIgnoreCase)
            || !SameDir(e.InstallDir, _dir.Text)).ToList();

    private void RefreshExistingInfo()
    {
        var targets = UpgradeTargets();
        var conflicts = Conflicts();
        if (targets.Count > 0)
        {
            var t = targets[0];
            _existing.Text = $"Vorhandene Installation {t.Version ?? "?"} gefunden in " +
                $"{t.InstallDir} — diese wird aktualisiert. " +
                "Projekte und Einstellungen bleiben erhalten.";
            _migrate.Visible = false;
        }
        else if (conflicts.Count > 0)
        {
            var c = conflicts[0];
            _existing.Text = $"Es existiert eine Installation {c.Version ?? "?"} in " +
                $"{c.InstallDir} [{(c.Scope == "machine" ? "alle Benutzer" : "nur dieser Benutzer")}]. " +
                "Der gewählte Bereich/Ordner weicht ab — ohne Bestätigung wird nichts " +
                "installiert (keine parallele oder stillschweigende Migration).";
            _migrate.Visible = true;
        }
        else
        {
            _existing.Text = "";
            _migrate.Visible = false;
        }
    }

    private void Browse()
    {
        using var dialog = new FolderBrowserDialog
        {
            Description = "Installationsordner für NØVA Flash Studio wählen",
            SelectedPath = _dir.Text,
            ShowNewFolderButton = true,
        };
        if (dialog.ShowDialog(this) == DialogResult.OK)
        {
            _dir.Text = dialog.SelectedPath;
        }
    }

    private void OnScopeChanged()
    {
        // Keep the directory in sync with the scope unless the user customized it.
        var isDefault = _dir.Text.Equals(
            Installer.DefaultInstallDir(InstallScope.PerUser), StringComparison.OrdinalIgnoreCase)
            || _dir.Text.Equals(
                Installer.DefaultInstallDir(InstallScope.AllUsers), StringComparison.OrdinalIgnoreCase);
        if (isDefault || string.IsNullOrWhiteSpace(_dir.Text))
        {
            _dir.Text = Installer.DefaultInstallDir(Scope);
        }
        _adminHint.Visible = _allUsers.Checked;
        RefreshExistingInfo();
    }

    public override void OnEnter()
    {
        _adminHint.Visible = true;
        _adminHint.Text = _allUsers.Checked
            ? "Die Installation für alle Benutzer erfordert Administratorrechte: " +
              "Beim Start der Installation erscheint der normale Windows-UAC-Dialog. " +
              "Bei Ablehnung wird nichts installiert."
            : "Die Installation nur für diesen Benutzer benötigt keine " +
              "Administratorrechte und installiert nach %LOCALAPPDATA%\\Programs.";
        RefreshExistingInfo();
    }

    public override string? ValidateInput()
    {
        if (string.IsNullOrWhiteSpace(_dir.Text))
            return "Bitte einen Installationsordner angeben.";
        try { Path.GetFullPath(_dir.Text); }
        catch (Exception) { return "Der Installationsordner ist ungültig."; }
        if (Conflicts().Count > 0 && !_migrate.Checked)
        {
            return "Eine Installation im abweichenden Bereich/Ordner existiert. " +
                "Bestätigen Sie die Migration oder wählen Sie den passenden " +
                "Bereich und Ordner für ein Upgrade.";
        }
        return null;
    }
}

/// <summary>Page 4: shortcuts and file associations.</summary>
internal sealed class OptionsPage : WizardPage
{
    public override string Title => "Optionen";
    public override string Subtitle => "Verknüpfungen und Dateizuordnungen";

    private readonly CheckBox _desktop = new() { AutoSize = true, Text = "Desktop-Verknüpfung erstellen" };
    private readonly CheckBox _startMenu = new() { AutoSize = true, Text = "Startmenü-Verknüpfung erstellen" };
    private readonly CheckBox _nova = new() { AutoSize = true, Text = ".nova-Projektdateien mit NØVA Flash Studio öffnen" };
    private readonly CheckBox _swf = new() { AutoSize = true, Text = ".swf-Dateien mit NØVA Flash Studio öffnen" };

    public bool DesktopShortcut => _desktop.Checked;
    public bool StartMenuShortcut => _startMenu.Checked;
    public bool AssociateNova => _nova.Checked;
    public bool AssociateSwf => _swf.Checked;

    public OptionsPage()
    {
        int top = 32;
        foreach (var cb in new[] { _desktop, _startMenu, _nova, _swf })
        {
            cb.Top = top; cb.Left = 24; cb.ForeColor = Color.Gainsboro;
            Controls.Add(cb);
            top += 34;
        }
        _desktop.Checked = true;
        _startMenu.Checked = true;
        _nova.Checked = true;
        _swf.Checked = false;

        var note = MakeLabel(
            "Hinweis zu Dateizuordnungen: Windows 10/11 entscheidet die tatsächliche " +
            "Standardanwendung in den Systemsettings. Dieser Assistent registriert " +
            "nur den Öffnen-Befehl für NØVA Flash Studio und überschreibt keine " +
            "bestehenden Zuordnungen stillschweigend — beim Deinstallieren werden " +
            "nur Einträge entfernt, die noch auf diese Installation zeigen. " +
            "Ihre Projekte und Einstellungen unter %APPDATA% bleiben immer erhalten.", 9f);
        note.Top = top + 16; note.Left = 24;
        Controls.Add(note);
    }
}

/// <summary>Page 5: progress.</summary>
internal sealed class ProgressPage : WizardPage
{
    public override string Title => "Installation";
    public override string Subtitle => "Bitte warten …";

    private readonly ProgressBar _bar = new() { Left = 24, Top = 20, Width = 620, Height = 24, Style = ProgressBarStyle.Continuous };
    private readonly Label _status = new() { Left = 24, Top = 58, Width = 620, Height = 60, ForeColor = Color.Gainsboro };

    public ProgressPage()
    {
        Controls.Add(_status);
        Controls.Add(_bar);
    }

    public void Report(int percent, string message)
    {
        if (IsDisposed) return;
        if (InvokeRequired) { BeginInvoke(() => Report(percent, message)); return; }
        if (percent >= 0) _bar.Value = Math.Clamp(percent, 0, 100);
        _status.Text = message;
    }
}

/// <summary>Page 6: finish.</summary>
internal sealed class FinishPage : WizardPage
{
    public override string Title => "Fertig";
    public override string Subtitle => "Installation abgeschlossen";

    private readonly CheckBox _launch = new()
    {
        AutoSize = true,
        Text = "NØVA Flash Studio jetzt starten",
        ForeColor = Color.Gainsboro,
        Top = 60,
        Left = 24,
        Checked = true,
    };

    public bool Launch => _launch.Checked;
    public string? InstallDir { get; set; }

    public FinishPage()
    {
        AddControl(MakeLabel("NØVA Flash Studio wurde erfolgreich installiert.", 11f, bold: true), 24);
        AddControl(MakeLabel(
            "Sie können das Programm über das Startmenü oder die Desktop-Verknüpfung starten.\n" +
            "Die Deinstallation finden Sie in den Windows-Einstellungen unter „Apps“.",
            9.5f), 56);
        Controls.Add(_launch);
    }
}

/// <summary>The setup wizard form.</summary>
internal sealed class WizardForm : Form
{
    private static readonly Color Bg = Color.FromArgb(28, 30, 36);
    private static readonly Color PanelBg = Color.FromArgb(38, 41, 48);
    private static readonly Color Accent = Color.FromArgb(64, 156, 255);

    private readonly List<WizardPage> _pages = new();
    private int _index;
    private readonly Label _step = new() { AutoSize = true, ForeColor = Color.Gray, Font = new Font("Segoe UI", 8.5f) };
    private readonly Label _title = new() { AutoSize = true, ForeColor = Color.White, Font = new Font("Segoe UI", 13f, FontStyle.Bold) };
    private readonly Label _subtitle = new() { AutoSize = true, ForeColor = Color.Silver, Font = new Font("Segoe UI", 9f) };
    private readonly Panel _content = new() { Dock = DockStyle.Fill, BackColor = Bg, Padding = new Padding(0, 64, 0, 56) };
    private readonly Panel _footer = new() { Dock = DockStyle.Bottom, Height = 56, BackColor = PanelBg };
    private readonly Button _back = new() { Text = "Zurück", Width = 96, Height = 30 };
    private readonly Button _next = new() { Text = "Weiter", Width = 110, Height = 30, BackColor = Accent, ForeColor = Color.White, FlatStyle = FlatStyle.Flat };
    private readonly Button _cancel = new() { Text = "Abbrechen", Width = 96, Height = 30 };

    private readonly List<Installer.ExistingInstall> _existingInstalls;
    private InstallOptions? _installOptions;
    private bool _installing;
    private CancellationTokenSource? _cts;

    public WizardForm()
    {
        Text = "NØVA Flash Studio – Setup";
        StartPosition = FormStartPosition.CenterScreen;
        ClientSize = new Size(720, 540);
        FormBorderStyle = FormBorderStyle.FixedDialog;
        MaximizeBox = false;
        MinimizeBox = false;
        BackColor = Bg;
        ForeColor = Color.Gainsboro;
        Font = new Font("Segoe UI", 9f);
        Icon = LoadIcon();

        _existingInstalls = Installer.FindExistingInstalls();

        var header = new Panel { Dock = DockStyle.Top, Height = 64, BackColor = PanelBg };
        _title.Location = new Point(24, 12);
        _subtitle.Location = new Point(24, 38);
        header.Controls.Add(_title);
        header.Controls.Add(_subtitle);

        _step.AutoSize = true;
        _step.Anchor = AnchorStyles.Top | AnchorStyles.Right;
        _step.Location = new Point(600, 26);
        header.Controls.Add(_step);

        _back.Anchor = AnchorStyles.Bottom | AnchorStyles.Right;
        _back.Location = new Point(340, 14);
        _back.Click += (_, _) => Navigate(-1);

        _next.Anchor = AnchorStyles.Bottom | AnchorStyles.Right;
        _next.Location = new Point(450, 14);
        _next.Click += async (_, _) => await NavigateNext();

        _cancel.Anchor = AnchorStyles.Bottom | AnchorStyles.Right;
        _cancel.Location = new Point(574, 14);
        _cancel.Click += (_, _) =>
        {
            if (_index == _pages.Count - 1) MaybeLaunch();
            Close();
        };

        _footer.Controls.Add(_back);
        _footer.Controls.Add(_next);
        _footer.Controls.Add(_cancel);

        Controls.Add(_content);
        Controls.Add(_footer);
        Controls.Add(header);

        BuildPages();
        ShowPage(0);
    }

    private static Icon? LoadIcon()
    {
        try
        {
            using var stream = System.Reflection.Assembly.GetExecutingAssembly()
                .GetManifestResourceStream("logo.png");
            if (stream is null) return null;
            using var bmp = new Bitmap(stream);
            var hIcon = bmp.GetHicon();
            var icon = Icon.FromHandle(hIcon);
            var clone = (Icon)icon.Clone();
            DestroyIcon(hIcon);
            return clone;
        }
        catch { return null; }
    }

    [System.Runtime.InteropServices.DllImport("user32.dll")]
    private static extern bool DestroyIcon(IntPtr handle);

    private void BuildPages()
    {
        _pages.Add(new WelcomePage());
        _pages.Add(new LicensePage(_next));
        _pages.Add(new ScopePage(_existingInstalls));
        _pages.Add(new OptionsPage());
        _pages.Add(new ProgressPage());
        _pages.Add(new FinishPage());
    }

    private void ShowPage(int index)
    {
        _index = index;
        var page = _pages[index];
        _title.Text = page.Title;
        _subtitle.Text = page.Subtitle;
        _step.Text = $"Schritt {index + 1} von {_pages.Count}";

        _content.Controls.Clear();
        page.Dock = DockStyle.Fill;
        page.Visible = true;
        _content.Controls.Add(page);

        _back.Visible = index > 0 && index < _pages.Count - 1;
        _back.Enabled = !_installing;
        _next.Text = index == _pages.Count - 2 ? "Installieren" : "Weiter";
        _next.Visible = index < _pages.Count - 1;
        _next.Enabled = index != 1 || ((LicensePage)page).Accepted;
        _cancel.Text = index == _pages.Count - 1 ? "Schließen" : "Abbrechen";
        _cancel.Enabled = true;
        page.OnEnter();
    }

    private async Task NavigateNext()
    {
        var page = _pages[_index];
        if (page is ScopePage scope)
        {
            var error = scope.ValidateInput();
            if (error is not null) { MessageBoxCompat.ShowError(error); return; }
        }
        if (_index == 3 && page is OptionsPage options)
        {
            var scopePage = (ScopePage)_pages[2];
            _installOptions = new InstallOptions
            {
                InstallDir = scopePage.InstallDir,
                Scope = scopePage.Scope,
                DesktopShortcut = options.DesktopShortcut,
                StartMenuShortcut = options.StartMenuShortcut,
                AssociateNova = options.AssociateNova,
                AssociateSwf = options.AssociateSwf,
            };
        }

        if (_index == _pages.Count - 2)
        {
            await RunInstall();
            return;
        }
        ShowPage(_index + 1);
    }

    private void Navigate(int delta)
    {
        if (_installing) return;
        ShowPage(Math.Clamp(_index + delta, 0, _pages.Count - 1));
    }

    private async Task RunInstall()
    {
        if (_installOptions is null) return;

        // All-users installs require elevation: hand the chosen options to an
        // elevated instance via the standard UAC prompt, then close. If the
        // user denies UAC we stay on the options page — never silently fall
        // back to a per-user install.
        if (_installOptions.Scope == InstallScope.AllUsers && !Installer.IsAdministrator())
        {
            try
            {
                Installer.RelaunchElevated(BuildElevatedArgs(_installOptions));
                Close();
            }
            catch (InstallException ex)
            {
                MessageBoxCompat.ShowError(ex.Message);
            }
            return;
        }

        _installing = true;
        _back.Enabled = false;
        _next.Enabled = false;
        _cancel.Enabled = false;
        _cts = new CancellationTokenSource();
        var progressPage = (ProgressPage)_pages[4];
        ShowPage(4);

        var options = _installOptions;
        try
        {
            await Task.Run(() =>
            {
                Installer.Install(
                    options,
                    (percent, message) => progressPage.Report(percent, message),
                    _cts.Token);
            });
            var finish = (FinishPage)_pages[5];
            finish.InstallDir = options.InstallDir;
            ShowPage(5);
        }
        catch (OperationCanceledException)
        {
            MessageBoxCompat.ShowInfo("Installation wurde abgebrochen.");
            ShowPage(2);
        }
        catch (Exception ex)
        {
            MessageBoxCompat.ShowError($"Installation fehlgeschlagen:\n\n{ex.Message}");
            ShowPage(2);
        }
        finally
        {
            _installing = false;
            _next.Enabled = true;
            _back.Enabled = _index > 0 && _index < _pages.Count - 1;
            _cancel.Enabled = true;
        }
    }

    protected override void OnFormClosed(FormClosedEventArgs e)
    {
        _cts?.Cancel();
        base.OnFormClosed(e);
    }

    protected override void OnFormClosing(FormClosingEventArgs e)
    {
        if (_installing)
        {
            var result = MessageBox.Show(
                "Die Installation läuft noch. Trotzdem abbrechen?",
                "NØVA Flash Studio – Setup",
                MessageBoxButtons.YesNo,
                MessageBoxIcon.Warning);
            if (result == DialogResult.No)
            {
                e.Cancel = true;
                return;
            }
            _cts?.Cancel();
        }
        base.OnFormClosing(e);
    }

    private static string BuildElevatedArgs(InstallOptions o)
    {
        var parts = new List<string>
        {
            "--install", $"\"{o.InstallDir}\"",
            "--all-users",
            o.DesktopShortcut ? "--desktop" : "--no-desktop",
            o.StartMenuShortcut ? "--start-menu" : "--no-start-menu",
        };
        if (o.AssociateNova) parts.Add("--assoc-nova");
        if (o.AssociateSwf) parts.Add("--assoc-swf");
        if (!o.AssociateNova && !o.AssociateSwf) parts.Add("--no-assoc");
        parts.Add("--notify");
        return string.Join(" ", parts);
    }

    /// <summary>Launches the app after a successful finish page.</summary>
    public void MaybeLaunch()
    {
        if (_pages[5] is FinishPage finish && finish.Launch && finish.InstallDir is not null)
        {
            Installer.LaunchInstalledApp(finish.InstallDir);
        }
    }
}
