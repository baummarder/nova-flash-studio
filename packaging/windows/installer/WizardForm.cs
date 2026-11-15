using System.Windows.Forms;

namespace NovaFlashStudio.Setup;

/// <summary>Shared classic palette (2009 desktop look, matches the app).</summary>
internal static class WizardTheme
{
    public static readonly Color Bg = Color.FromArgb(38, 40, 43);
    public static readonly Color PanelBg = Color.FromArgb(48, 50, 54);
    public static readonly Color HeaderTop = Color.FromArgb(56, 59, 64);
    public static readonly Color HeaderBottom = Color.FromArgb(44, 46, 50);
    public static readonly Color EdgeHi = Color.FromArgb(96, 100, 107);
    public static readonly Color EdgeLo = Color.FromArgb(26, 28, 30);
    public static readonly Color Accent = Color.FromArgb(76, 155, 224);
    public static readonly Color TextMain = Color.FromArgb(216, 216, 216);
    public static readonly Color TextDim = Color.FromArgb(168, 171, 176);
    public static readonly Color TextFaint = Color.FromArgb(126, 130, 136);
    public static readonly Color Border = Color.FromArgb(88, 91, 97);
    public static readonly Color Warn = Color.FromArgb(230, 160, 60);
    public static readonly Color Info = Color.FromArgb(120, 180, 230);

    /// <summary>Classic etched group frame.</summary>
    public static GroupBox MakeGroup(string text)
    {
        return new GroupBox
        {
            Text = text,
            ForeColor = TextDim,
            Font = new Font("Segoe UI", 8.5f, FontStyle.Bold),
        };
    }
}

/// <summary>Base class for wizard pages. All visible text is resolved through
/// <see cref="InstallerText"/> so a mid-wizard language switch only needs
/// <see cref="RefreshText"/>.</summary>
internal abstract class WizardPage : UserControl
{
    public virtual string Title => "";
    public virtual string Subtitle => "";

    /// <summary>Validate before leaving the page; returns error text or null.</summary>
    public virtual string? ValidateInput() => null;

    /// <summary>Called when the page becomes current.</summary>
    public virtual void OnEnter() { }

    /// <summary>Re-apply localized strings after a language change.</summary>
    public virtual void RefreshText() { }

    protected Label MakeLabel(string text, float size = 9.5f, bool bold = false)
    {
        return new Label
        {
            Text = text,
            AutoSize = true,
            Font = new Font("Segoe UI", size, bold ? FontStyle.Bold : FontStyle.Regular),
            ForeColor = WizardTheme.TextMain,
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

/// <summary>First page (only when no language was persisted yet): pick the
/// setup language. Native language names are always shown.</summary>
internal sealed class LanguagePage : WizardPage
{
    private readonly ComboBox _combo = new() { DropDownStyle = ComboBoxStyle.DropDownList, Width = 220 };
    private readonly Label _prompt = new()
    {
        AutoSize = true,
        MaximumSize = new Size(560, 0),
        ForeColor = WizardTheme.TextDim,
    };

    public override string Title => InstallerText.T("wiz.lang.title");
    public override string Subtitle => InstallerText.T("wiz.lang.subtitle");

    public LanguagePage()
    {
        foreach (var (code, name) in InstallerText.Languages)
        {
            _combo.Items.Add(name);
        }
        _combo.SelectedIndex = Array.FindIndex(
            InstallerText.Languages, l => l.Code == InstallerText.Current);
        if (_combo.SelectedIndex < 0) _combo.SelectedIndex = 0;
        _combo.Top = 40;
        _combo.Left = 24;
        Controls.Add(_combo);

        var label = MakeLabel(InstallerText.T("wiz.lang.label"), 9.5f, bold: true);
        label.Top = 18;
        label.Left = 24;
        Controls.Add(label);

        _prompt.Top = 90;
        _prompt.Left = 24;
        _prompt.Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right;
        Controls.Add(_prompt);
        RefreshText();
    }

    /// <summary>Apply the combo selection; called by the wizard on Next.</summary>
    public string SelectedCode =>
        _combo.SelectedIndex >= 0
            ? InstallerText.Languages[_combo.SelectedIndex].Code
            : InstallerText.Current;

    /// <summary>Apply selection immediately so the next page is localized.</summary>
    public void ApplySelection()
    {
        InstallerText.SetLanguage(SelectedCode);
    }

    public override void RefreshText()
    {
        _prompt.Text = InstallerText.T("wiz.lang.prompt");
    }
}

/// <summary>Page: welcome / product info.</summary>
internal sealed class WelcomePage : WizardPage
{
    private readonly Label _version = new() { AutoSize = true };
    private readonly Label _body = new() { AutoSize = false };

    public override string Title => InstallerText.T("wiz.welcome.title");
    public override string Subtitle => InstallerText.T("wiz.welcome.subtitle", EditionInfo.ProductName);

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

        var name = MakeLabel(EditionInfo.ProductName, 16f, bold: true);
        name.Top = 140; name.Left = 24;
        Controls.Add(name);

        _version.Top = 172; _version.Left = 24;
        _version.Font = new Font("Segoe UI", 10f);
        _version.ForeColor = WizardTheme.TextDim;
        Controls.Add(_version);

        _body.Top = 210; _body.Left = 24; _body.Width = 640; _body.Height = 160;
        _body.Font = new Font("Segoe UI", 9.5f);
        _body.ForeColor = WizardTheme.TextMain;
        Controls.Add(_body);
        RefreshText();
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

    public override void RefreshText()
    {
        _version.Text = InstallerText.T("wiz.welcome.version", AssemblyVersion());
        _body.Text = InstallerText.T("wiz.welcome.body", EditionInfo.ProductName);
    }
}

/// <summary>Page: license agreement.</summary>
internal sealed class LicensePage : WizardPage
{
    private readonly RadioButton _accept = new() { AutoSize = true };
    private readonly RadioButton _decline = new() { AutoSize = true };
    private readonly Button _next;

    public override string Title => InstallerText.T("wiz.license.title");
    public override string Subtitle => InstallerText.T("wiz.license.subtitle");

    public bool Accepted => _accept.Checked;

    public LicensePage(Button nextButton)
    {
        _next = nextButton;
        var text = Installer.ReadEulaText() ?? InstallerText.T("wiz.license.missing");
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
        _accept.Left = 8; _accept.Top = 10; _accept.ForeColor = WizardTheme.TextMain;
        _decline.Left = 280; _decline.Top = 10; _decline.ForeColor = WizardTheme.TextMain;
        bottom.Controls.Add(_accept);
        bottom.Controls.Add(_decline);
        Controls.Add(bottom);
        box.BringToFront();
        bottom.BringToFront();

        _accept.CheckedChanged += (_, _) => _next.Enabled = _accept.Checked;
        _decline.CheckedChanged += (_, _) => _next.Enabled = false;
        _accept.Checked = true;
        _next.Enabled = true;
        RefreshText();
    }

    public override void OnEnter()
    {
        _next.Enabled = _accept.Checked;
    }

    public override void RefreshText()
    {
        _accept.Text = InstallerText.T("wiz.license.accept");
        _decline.Text = InstallerText.T("wiz.license.decline");
    }
}

/// <summary>Page: install scope and directory.</summary>
internal sealed class ScopePage : WizardPage
{
    private readonly RadioButton _perUser = new() { AutoSize = true };
    private readonly RadioButton _allUsers = new() { AutoSize = true };
    private readonly TextBox _dir = new() { Width = 420 };
    private readonly Button _browse = new() { Width = 110 };
    private readonly Label _dirLabel = new() { AutoSize = true };
    private readonly Label _adminHint = new() { AutoSize = true, ForeColor = WizardTheme.Warn };
    private readonly Label _existing = new() { AutoSize = true, ForeColor = WizardTheme.Info, MaximumSize = new Size(620, 0) };
    private readonly CheckBox _migrate = new()
    {
        AutoSize = true,
        ForeColor = WizardTheme.TextMain,
    };

    public override string Title => InstallerText.T("wiz.scope.title");
    public override string Subtitle => InstallerText.T("wiz.scope.subtitle");

    public InstallScope Scope => _allUsers.Checked ? InstallScope.AllUsers : InstallScope.PerUser;
    public string InstallDir => _dir.Text.Trim();
    /// <summary>True when the user confirmed migrating a conflicting install.</summary>
    public bool MigrationRequested => _migrate.Visible && _migrate.Checked;

    private readonly List<Installer.ExistingInstall> _existingInstalls;

    public ScopePage(IReadOnlyList<Installer.ExistingInstall> existing)
    {
        _existingInstalls = existing.ToList();
        int top = 24;
        _perUser.Top = top; _perUser.Left = 24; _perUser.ForeColor = WizardTheme.TextMain;
        _allUsers.Top = top + 28; _allUsers.Left = 24; _allUsers.ForeColor = WizardTheme.TextMain;
        Controls.Add(_perUser); Controls.Add(_allUsers);

        _dirLabel.Top = top + 70; _dirLabel.Left = 24;
        _dirLabel.Font = new Font("Segoe UI", 9.5f, FontStyle.Bold);
        _dirLabel.ForeColor = WizardTheme.TextMain;
        Controls.Add(_dirLabel);

        _dir.Top = top + 94; _dir.Left = 24;
        _dir.ForeColor = Color.Black;
        _dir.BackColor = Color.White; // classic light input
        Controls.Add(_dir);

        _browse.Top = _dir.Top; _browse.Left = _dir.Right + 8;
        _browse.FlatStyle = FlatStyle.System;
        Controls.Add(_browse);

        _adminHint.Top = top + 128; _adminHint.Left = 24;
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
        RefreshText();
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
            _existing.Text = InstallerText.T("wiz.scope.found", t.Version ?? "?", t.InstallDir);
            _migrate.Visible = false;
        }
        else if (conflicts.Count > 0)
        {
            var c = conflicts[0];
            _existing.Text = InstallerText.T(
                "wiz.scope.conflict",
                c.Version ?? "?",
                c.InstallDir,
                c.Scope == "machine"
                    ? InstallerText.T("wiz.scope.scopeall")
                    : InstallerText.T("wiz.scope.scopeuser"));
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
            Description = InstallerText.T("wiz.scope.browse", EditionInfo.ProductName),
            SelectedPath = _dir.Text,
            ShowNewFolderButton = true,
        };
        if (dialog.ShowDialog(this) == DialogResult.OK)
        {
            _dir.Text = dialog.SelectedPath;
            RefreshExistingInfo();
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
        RefreshExistingInfo();
    }

    public override void OnEnter()
    {
        RefreshExistingInfo();
        UpdateHint();
    }

    private void UpdateHint()
    {
        _adminHint.Text = _allUsers.Checked
            ? InstallerText.T("wiz.scope.hintall")
            : InstallerText.T("wiz.scope.hintuser");
        _adminHint.Visible = true;
    }

    public override void RefreshText()
    {
        _perUser.Text = InstallerText.T("wiz.scope.peruser");
        _allUsers.Text = InstallerText.T("wiz.scope.allusers");
        _dirLabel.Text = InstallerText.T("wiz.scope.dirlabel");
        _browse.Text = InstallerText.T("wiz.scope.browse");
        _migrate.Text = InstallerText.T("wiz.scope.migrate");
        UpdateHint();
        RefreshExistingInfo();
    }

    public override string? ValidateInput()
    {
        if (string.IsNullOrWhiteSpace(_dir.Text))
            return InstallerText.T("wiz.scope.errdir");
        try { Path.GetFullPath(_dir.Text); }
        catch (Exception) { return InstallerText.T("wiz.scope.errinvalid"); }
        if (Conflicts().Count > 0 && !_migrate.Checked)
        {
            return InstallerText.T("wiz.scope.errconflict");
        }
        return null;
    }
}

/// <summary>Page: shortcuts and file associations.</summary>
internal sealed class OptionsPage : WizardPage
{
    private readonly CheckBox _desktop = new() { AutoSize = true };
    private readonly CheckBox _startMenu = new() { AutoSize = true };
    private readonly CheckBox _nova = new() { AutoSize = true };
    private readonly CheckBox _swf = new() { AutoSize = true };
    private readonly Label _note = new() { AutoSize = false };

    public override string Title => InstallerText.T("wiz.options.title");
    public override string Subtitle => InstallerText.T("wiz.options.subtitle");

    public bool DesktopShortcut => _desktop.Checked;
    public bool StartMenuShortcut => _startMenu.Checked;
    public bool AssociateNova => _nova.Checked;
    public bool AssociateSwf => _swf.Checked;

    public OptionsPage()
    {
        int top = 32;
        foreach (var cb in new[] { _desktop, _startMenu, _nova, _swf })
        {
            cb.Top = top; cb.Left = 24; cb.ForeColor = WizardTheme.TextMain;
            Controls.Add(cb);
            top += 34;
        }
        _desktop.Checked = true;
        _startMenu.Checked = true;
        _nova.Checked = true;
        _swf.Checked = false;

        _note.Top = top + 16; _note.Left = 24; _note.Width = 650; _note.Height = 120;
        _note.Font = new Font("Segoe UI", 9f);
        _note.ForeColor = WizardTheme.TextFaint;
        Controls.Add(_note);
        RefreshText();
    }

    public override void RefreshText()
    {
        _desktop.Text = InstallerText.T("wiz.options.desktop");
        _startMenu.Text = InstallerText.T("wiz.options.startmenu");
        _nova.Text = InstallerText.T("wiz.options.nova", EditionInfo.ProductName);
        _swf.Text = InstallerText.T("wiz.options.swf", EditionInfo.ProductName);
        _note.Text = InstallerText.T("wiz.options.note", EditionInfo.ProductName);
    }
}

/// <summary>Page: summary of all choices before install starts.</summary>
internal sealed class SummaryPage : WizardPage
{
    private readonly ScopePage _scope;
    private readonly OptionsPage _options;
    private readonly Label _body = new() { AutoSize = false };

    public override string Title => InstallerText.T("wiz.summary.title");
    public override string Subtitle => InstallerText.T("wiz.summary.subtitle");

    public SummaryPage(ScopePage scope, OptionsPage options)
    {
        _scope = scope;
        _options = options;
        _body.Top = 28; _body.Left = 24; _body.Width = 660; _body.Height = 380;
        _body.Font = new Font("Segoe UI", 10f);
        _body.ForeColor = WizardTheme.TextMain;
        Controls.Add(_body);
        RefreshText();
    }

    private string OnOff(bool value) =>
        value ? "✓" : InstallerText.T("wiz.summary.none");

    public override void OnEnter() => RefreshText();

    public override void RefreshText()
    {
        var scopeText = _scope.Scope == InstallScope.AllUsers
            ? InstallerText.T("wiz.summary.scopeall")
            : InstallerText.T("wiz.summary.scopeuser");
        var assoc = new List<string>();
        if (_options.AssociateNova) assoc.Add(".nova");
        if (_options.AssociateSwf) assoc.Add(".swf");
        _body.Text =
            $"{InstallerText.T("wiz.summary.product")}  {EditionInfo.ProductName}\n" +
            $"{InstallerText.T("wiz.summary.version")}  {AssemblyVersion()}\n" +
            $"{InstallerText.T("wiz.summary.scope")}  {scopeText}\n" +
            $"{InstallerText.T("wiz.summary.dir")}  {_scope.InstallDir}\n\n" +
            $"{InstallerText.T("wiz.summary.desktop")}      {OnOff(_options.DesktopShortcut)}\n" +
            $"{InstallerText.T("wiz.summary.startmenu")}   {OnOff(_options.StartMenuShortcut)}\n" +
            $"{InstallerText.T("wiz.summary.assocnova")}   {(assoc.Contains(".nova") ? "✓" : InstallerText.T("wiz.summary.none"))}\n" +
            $"{InstallerText.T("wiz.summary.assocswf")}    {(assoc.Contains(".swf") ? "✓" : InstallerText.T("wiz.summary.none"))}\n\n" +
            InstallerText.T("wiz.summary.note");
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

/// <summary>Page: progress.</summary>
internal sealed class ProgressPage : WizardPage
{
    public override string Title => InstallerText.T("wiz.progress.title");
    public override string Subtitle => InstallerText.T("wiz.progress.subtitle");

    private readonly ProgressBar _bar = new() { Left = 24, Top = 20, Width = 620, Height = 24, Style = ProgressBarStyle.Continuous };
    private readonly Label _status = new() { Left = 24, Top = 58, Width = 620, Height = 60, ForeColor = WizardTheme.TextMain };

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

/// <summary>Page: finish.</summary>
internal sealed class FinishPage : WizardPage
{
    private readonly CheckBox _launch = new()
    {
        AutoSize = true,
        Top = 60,
        Left = 24,
        Checked = true,
    };

    private readonly Button _docs = new()
    {
        AutoSize = true,
        Top = 92,
        Left = 24,
        FlatStyle = FlatStyle.System,
    };

    private readonly Label _success = new() { AutoSize = false };
    private readonly Label _info = new() { AutoSize = false };

    public override string Title => InstallerText.T("wiz.finish.title");
    public override string Subtitle => InstallerText.T("wiz.finish.subtitle");

    public bool Launch => _launch.Checked;
    public string? InstallDir { get; set; }

    public FinishPage()
    {
        _success.Top = 24; _success.Left = 24; _success.Width = 640;
        _success.Font = new Font("Segoe UI", 11f, FontStyle.Bold);
        _success.ForeColor = WizardTheme.TextMain;
        Controls.Add(_success);

        _info.Top = 56; _info.Left = 24; _info.Width = 640; _info.Height = 60;
        _info.Font = new Font("Segoe UI", 9.5f);
        _info.ForeColor = WizardTheme.TextDim;
        Controls.Add(_info);

        _launch.ForeColor = WizardTheme.TextMain;
        Controls.Add(_launch);
        Controls.Add(_docs);
        _docs.Click += (_, _) => OpenDocs();
        RefreshText();
    }

    private void OpenDocs()
    {
        if (InstallDir is null) return;
        // The payload ships docs/*.md next to the executable.
        var docsDir = Path.Combine(InstallDir, "docs");
        var target = Directory.Exists(docsDir) ? docsDir : InstallDir;
        try
        {
            System.Diagnostics.Process.Start(
                new System.Diagnostics.ProcessStartInfo("explorer.exe", $"\"{target}\"")
                {
                    UseShellExecute = true,
                });
        }
        catch { /* explorer unavailable — nothing else to do */ }
    }

    public override void RefreshText()
    {
        _success.Text = InstallerText.T("wiz.finish.success", EditionInfo.ProductName);
        _info.Text = InstallerText.T("wiz.finish.info");
        _launch.Text = InstallerText.T("wiz.finish.launch", EditionInfo.ProductName);
        _docs.Text = InstallerText.T("wiz.finish.docs");
    }
}

/// <summary>The setup wizard form — classic 2009 desktop styling:
/// charcoal surfaces, subtle header gradient, beveled buttons and
/// etched group frames, matching the NØVA application theme.
/// M24: language page + header language selector + summary page +
/// TableLayoutPanel footer so navigation can never be clipped.</summary>
internal sealed class WizardForm : Form
{
    private static readonly Color Bg = WizardTheme.Bg;
    private static readonly Color PanelBg = WizardTheme.PanelBg;
    private static readonly Color HeaderTop = WizardTheme.HeaderTop;
    private static readonly Color HeaderBottom = WizardTheme.HeaderBottom;
    private static readonly Color EdgeHi = WizardTheme.EdgeHi;
    private static readonly Color Accent = WizardTheme.Accent;
    private static readonly Color TextFaint = WizardTheme.TextFaint;
    private static readonly Color Border = WizardTheme.Border;

    /// <summary>Header strip with a subtle top-down gradient.</summary>
    private sealed class HeaderPanel : Panel
    {
        protected override void OnPaint(PaintEventArgs e)
        {
            base.OnPaint(e);
            using var brush = new System.Drawing.Drawing2D.LinearGradientBrush(
                ClientRectangle,
                HeaderTop,
                HeaderBottom,
                System.Drawing.Drawing2D.LinearGradientMode.Vertical);
            e.Graphics.FillRectangle(brush, ClientRectangle);
            // Bottom hairline separating header from content.
            using var pen = new Pen(Border);
            e.Graphics.DrawLine(pen, 0, Height - 1, Width, Height - 1);
        }
    }

    private readonly List<WizardPage> _pages = new();
    private int _index;
    private LanguagePage? _languagePage;
    private ScopePage? _scopePage;
    private OptionsPage? _optionsPage;

    private readonly Label _step = new() { AutoSize = true, ForeColor = WizardTheme.TextFaint, Font = new Font("Segoe UI", 8.5f) };
    private readonly Label _title = new() { AutoSize = true, ForeColor = WizardTheme.TextMain, Font = new Font("Segoe UI", 13f, FontStyle.Bold) };
    private readonly Label _subtitle = new() { AutoSize = true, ForeColor = WizardTheme.TextDim, Font = new Font("Segoe UI", 9f) };
    private readonly ComboBox _langCombo = new() { DropDownStyle = ComboBoxStyle.DropDownList, Width = 120 };

    private readonly Panel _content = new() { Dock = DockStyle.Fill, BackColor = Bg, Padding = new Padding(8) };
    private readonly Panel _footer = new() { Dock = DockStyle.Bottom, Height = 52, BackColor = PanelBg };
    private readonly Button _back = new() { Text = "Zurück", Width = 96, Height = 30, FlatStyle = FlatStyle.System };
    private readonly Button _next = new() { Text = "Weiter", Width = 110, Height = 30, BackColor = Accent, ForeColor = Color.White, FlatStyle = FlatStyle.Flat };
    private readonly Button _cancel = new() { Text = "Abbrechen", Width = 96, Height = 30, FlatStyle = FlatStyle.System };

    private readonly List<Installer.ExistingInstall> _existingInstalls;
    private InstallOptions? _installOptions;
    private bool _installing;
    private CancellationTokenSource? _cts;

    public WizardForm()
    {
        InstallerText.Initialize();
        Text = InstallerText.T("wiz.setup.title", EditionInfo.ProductName);
        StartPosition = FormStartPosition.CenterScreen;
        ClientSize = new Size(720, 540);
        FormBorderStyle = FormBorderStyle.FixedDialog;
        MaximizeBox = false;
        MinimizeBox = false;
        BackColor = Bg;
        ForeColor = WizardTheme.TextMain;
        Font = new Font("Segoe UI", 9f);
        Icon = LoadIcon();

        _existingInstalls = Installer.FindExistingInstalls();

        var header = new HeaderPanel { Dock = DockStyle.Top, Height = 64 };
        _title.Location = new Point(24, 12);
        _subtitle.Location = new Point(24, 38);
        header.Controls.Add(_title);
        header.Controls.Add(_subtitle);

        // Language selector — always visible so the language can be changed
        // during the whole install (M24.2).
        _langCombo.Anchor = AnchorStyles.Top | AnchorStyles.Right;
        _langCombo.Location = new Point(470, 24);
        _langCombo.Font = new Font("Segoe UI", 8.5f);
        foreach (var (code, name) in InstallerText.Languages)
        {
            _langCombo.Items.Add(name);
        }
        _langCombo.SelectedIndex = Math.Max(0,
            Array.FindIndex(InstallerText.Languages, l => l.Code == InstallerText.Current));
        _langCombo.SelectedIndexChanged += (_, _) => OnLanguageChanged();
        header.Controls.Add(_langCombo);

        _step.AutoSize = true;
        _step.Anchor = AnchorStyles.Top | AnchorStyles.Right;
        _step.Location = new Point(330, 26);
        _step.ForeColor = TextFaint;
        header.Controls.Add(_step);

        // Footer: right-aligned button strip inside a docking-safe table.
        var footerBar = new TableLayoutPanel
        {
            Dock = DockStyle.Fill,
            ColumnCount = 1,
            RowCount = 1,
            Padding = new Padding(0, 11, 12, 11),
        };
        footerBar.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100f));
        var buttons = new FlowLayoutPanel
        {
            Dock = DockStyle.Fill,
            FlowDirection = FlowDirection.RightToLeft,
            WrapContents = false,
        };
        buttons.Controls.Add(_cancel);
        buttons.Controls.Add(_next);
        buttons.Controls.Add(_back);
        footerBar.Controls.Add(buttons, 0, 0);
        _footer.Controls.Add(footerBar);

        _back.Click += (_, _) => Navigate(-1);
        _next.Click += async (_, _) => await NavigateNext();
        _cancel.Click += (_, _) => OnCancel();

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
        // M24.2: first run without a persisted language starts on the
        // language page; afterwards the choice is restored automatically.
        if (!InstallerText.HasStoredLanguage())
        {
            _languagePage = new LanguagePage();
            _pages.Add(_languagePage);
        }
        _pages.Add(new WelcomePage());
        _pages.Add(new LicensePage(_next));
        _scopePage = new ScopePage(_existingInstalls);
        _pages.Add(_scopePage);
        _optionsPage = new OptionsPage();
        _pages.Add(_optionsPage);
        _pages.Add(new SummaryPage(_scopePage, _optionsPage));
        _pages.Add(new ProgressPage());
        _pages.Add(new FinishPage());
    }

    private int IndexOf<T>() where T : WizardPage =>
        _pages.FindIndex(p => p is T);

    private void OnLanguageChanged()
    {
        if (_langCombo.SelectedIndex < 0) return;
        var code = InstallerText.Languages[_langCombo.SelectedIndex].Code;
        InstallerText.SetLanguage(code);
        RefreshAllText();
    }

    /// <summary>Re-localize header, footer and every page (language switch).</summary>
    private void RefreshAllText()
    {
        Text = InstallerText.T("wiz.setup.title", EditionInfo.ProductName);
        foreach (var page in _pages) page.RefreshText();
        _back.Text = InstallerText.T("wiz.back");
        _cancel.Text = _index == _pages.Count - 1
            ? InstallerText.T("wiz.close")
            : InstallerText.T("wiz.cancel");
        _next.Text = _index == IndexOf<SummaryPage>()
            ? InstallerText.T("wiz.install")
            : InstallerText.T("wiz.next");
        ShowPage(_index);
    }

    private void ShowPage(int index)
    {
        _index = index;
        var page = _pages[index];
        _title.Text = page.Title;
        _subtitle.Text = page.Subtitle;
        _step.Text = InstallerText.T("wiz.step", index + 1, _pages.Count);

        _content.Controls.Clear();
        page.Dock = DockStyle.Fill;
        page.Visible = true;
        _content.Controls.Add(page);

        var isLast = index == _pages.Count - 1;
        var isProgressOrLater = index >= _pages.Count - 2;
        _back.Visible = index > 0 && !isProgressOrLater;
        _back.Enabled = !_installing;
        _next.Text = page is SummaryPage
            ? InstallerText.T("wiz.install")
            : InstallerText.T("wiz.next");
        _next.Visible = !isLast;
        _next.Enabled = page is not LicensePage license || license.Accepted;
        _cancel.Text = isLast ? InstallerText.T("wiz.close") : InstallerText.T("wiz.cancel");
        _cancel.Enabled = !_installing;
        page.OnEnter();
    }

    private async Task NavigateNext()
    {
        var page = _pages[_index];
        if (page is LanguagePage language)
        {
            // Apply immediately so every following page is localized.
            language.ApplySelection();
            InstallerText.SetLanguage(language.SelectedCode);
            RefreshAllText();
            ShowPage(_index + 1);
            return;
        }
        var error = page.ValidateInput();
        if (error is not null)
        {
            MessageBoxCompat.ShowError(error);
            return;
        }

        if (page is OptionsPage && _scopePage is not null)
        {
            _installOptions = new InstallOptions
            {
                InstallDir = _scopePage.InstallDir,
                Scope = _scopePage.Scope,
                DesktopShortcut = _optionsPage?.DesktopShortcut ?? true,
                StartMenuShortcut = _optionsPage?.StartMenuShortcut ?? true,
                AssociateNova = _optionsPage?.AssociateNova ?? true,
                AssociateSwf = _optionsPage?.AssociateSwf ?? false,
            };
        }

        if (page is SummaryPage)
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

    private void OnCancel()
    {
        if (_index == _pages.Count - 1)
        {
            MaybeLaunch();
        }
        Close();
    }

    private async Task RunInstall()
    {
        if (_installOptions is null) return;

        // All-users installs require elevation: hand the chosen options to an
        // elevated instance via the standard UAC prompt, then close. If the
        // user denies UAC we stay on the summary page — never silently fall
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
        var progressIndex = _pages.FindIndex(p => p is ProgressPage);
        var finishIndex = _pages.FindIndex(p => p is FinishPage);
        if (progressIndex < 0 || finishIndex < 0) return;
        var progressPage = (ProgressPage)_pages[progressIndex];
        ShowPage(progressIndex);

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
            var finish = (FinishPage)_pages[finishIndex];
            finish.InstallDir = options.InstallDir;
            ShowPage(finishIndex);
        }
        catch (OperationCanceledException)
        {
            MessageBoxCompat.ShowInfo(InstallerText.T("wiz.cancel.done"));
            var scopeIndex = IndexOf<ScopePage>();
            ShowPage(scopeIndex >= 0 ? scopeIndex : 0);
        }
        catch (Exception ex)
        {
            MessageBoxCompat.ShowError($"{InstallerText.T("wiz.error.title")}\n\n{ex.Message}");
            var scopeIndex = IndexOf<ScopePage>();
            ShowPage(scopeIndex >= 0 ? scopeIndex : 0);
        }
        finally
        {
            _installing = false;
            RefreshNavState();
        }
    }

    /// <summary>Recompute button visibility/enabled after install finishes.</summary>
    private void RefreshNavState()
    {
        var isLast = _index == _pages.Count - 1;
        var isProgressOrLater = _index >= _pages.Count - 2;
        _back.Visible = _index > 0 && !isProgressOrLater;
        _back.Enabled = !_installing;
        _next.Visible = !isLast;
        _next.Enabled = _pages[_index] is not LicensePage license || license.Accepted;
        _cancel.Enabled = !_installing;
        _cancel.Text = isLast ? InstallerText.T("wiz.close") : InstallerText.T("wiz.cancel");
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
                InstallerText.T("wiz.cancel.confirm"),
                Text,
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
        if (_pages.Find(p => p is FinishPage) is FinishPage finish
            && finish.Launch && finish.InstallDir is not null)
        {
            Installer.LaunchInstalledApp(finish.InstallDir);
        }
    }
}
