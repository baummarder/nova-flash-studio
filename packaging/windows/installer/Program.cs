using System.Reflection;
using NovaFlashStudio.Setup;

namespace NovaFlashStudio.Setup;

internal static class Program
{
    private static int Main(string[] args)
    {
        if (args.Length == 0)
        {
            ApplicationConfiguration.Initialize();
            Application.Run(new WizardForm());
            return 0;
        }

        var command = args[0];
        try
        {
            return command switch
            {
                "--version" or "-V" => Version(),
                "--help" or "-h" => Help(),
                "--smoke-test" => SmokeTest(),
                "--install" => HeadlessInstall(args.Skip(1).ToArray()),
                "--uninstall" => HeadlessUninstall(args.Skip(1).ToArray()),
                _ => Unknown(command),
            };
        }
        catch (InstallException ex)
        {
            Cli.Error($"FEHLER: {ex.Message}");
            return 2;
        }
        catch (Exception ex)
        {
            Cli.Error($"FEHLER: {ex.GetType().Name}: {ex.Message}");
            return 1;
        }
    }

    private static int Version()
    {
        var info = Assembly.GetExecutingAssembly()
            .GetCustomAttribute<AssemblyInformationalVersionAttribute>()?.InformationalVersion;
        Cli.WriteLine($"{EditionInfo.ProductName} Setup {info ?? "0.0.0"}");
        return 0;
    }

    private static int Help()
    {
        Cli.WriteLine(
            $"""
            {EditionInfo.ProductName} Setup

            USAGE:
                {EditionInfo.SetupExeName}                 Interaktiver Assistent (Standard)
                {EditionInfo.SetupExeName} --version      Versionsinformationen
                {EditionInfo.SetupExeName} --smoke-test   Payload-Prüfsumme und Inhalt testen
                {EditionInfo.SetupExeName} --install DIR  Headless-Installation
                {EditionInfo.SetupExeName} --uninstall    Installation entfernen

            INSTALL-OPTIONEN:
                --all-users          Systemweite Installation (HKLM, UAC/Admin)
                --per-user           Installation nur für diesen Benutzer (Standard, %LOCALAPPDATA%)
                --desktop            Desktop-Verknüpfung
                --no-desktop         Keine Desktop-Verknüpfung
                --start-menu         Startmenü-Verknüpfung
                --no-start-menu      Keine Startmenü-Verknüpfung
                --assoc-nova         .nova-Dateizuordnung registrieren
                --assoc-swf          .swf-Dateizuordnung registrieren
                --no-assoc           Keine Dateizuordnungen
                --migrate            Vorhandene abweichende Installation entfernen (Migration)
                --quiet              Keine Fortschrittsausgabe
                --notify             Ergebnis in einem Dialog anzeigen (für elevated Runs)

            UNINSTALL-OPTIONEN:
                --quiet              Ohne Bestätigung abfragen

            Benutzerprojekte und -einstellungen unter %APPDATA% werden niemals
            entfernt.
            """);
        return 0;
    }

    private static int Unknown(string command)
    {
        Cli.Error($"Unbekannter Befehl: {command}");
        Cli.Error("Verwenden Sie --help für die Syntax.");
        return 2;
    }

    private static int SmokeTest()
    {
        var hash = Installer.PayloadSha256();
        if (hash is null)
        {
            Cli.Error("FEHLER: payload.zip ist nicht in der Setup-Exe eingebettet.");
            return 2;
        }
        Cli.WriteLine($"payload-sha256: {hash}");

        using var stream = System.Reflection.Assembly.GetExecutingAssembly()
            .GetManifestResourceStream("payload.zip")!;
        using var archive = new System.IO.Compression.ZipArchive(stream, System.IO.Compression.ZipArchiveMode.Read);
        var exeEntries = archive.Entries
            .Where(e => e.FullName.EndsWith(".exe", StringComparison.OrdinalIgnoreCase))
            .ToList();
        Cli.WriteLine($"payload-eintraege: {archive.Entries.Count}");
        Cli.WriteLine($"exe-eintraege: {exeEntries.Count}");
        foreach (var entry in exeEntries.Take(5))
        {
            Cli.WriteLine($"  {entry.FullName} ({entry.Length} bytes)");
        }
        if (exeEntries.Count == 0)
        {
            Cli.Error("FEHLER: Die Payload enthält keine ausführbare Datei.");
            return 2;
        }
        var eula = Installer.ReadEulaText();
        Cli.WriteLine($"eula: {(eula is null ? "FEHLT" : $"{eula.Length} Zeichen")}");
        Cli.WriteLine("smoke-test: OK");
        return 0;
    }

    private sealed class HeadlessOptions
    {
        public string? InstallDir;
        public InstallScope Scope = InstallScope.PerUser;
        public bool Desktop = true;
        public bool StartMenu = true;
        public bool AssociateNova;
        public bool AssociateSwf;
        public bool Quiet;
        public bool Notify;
        public bool Migrate;
    }

    private static int HeadlessInstall(string[] args)
    {
        var parsed = ParseHeadless(args, requireDir: true);
        if (parsed is null) return 2;
        var dir = Path.GetFullPath(parsed.InstallDir!);
        var options = new InstallOptions
        {
            InstallDir = dir,
            Scope = parsed.Scope,
            DesktopShortcut = parsed.Desktop,
            StartMenuShortcut = parsed.StartMenu,
            AssociateNova = parsed.AssociateNova,
            AssociateSwf = parsed.AssociateSwf,
            MigrateExisting = parsed.Migrate,
        };
        Installer.Install(options, (p, m) => { if (!parsed.Quiet) Cli.WriteLine($"[{p,3}%] {m}"); });
        Cli.WriteLine($"Installiert: {dir}");
        if (parsed.Notify)
        {
            MessageBoxCompat.ShowInfo(
                $"{EditionInfo.ProductName} wurde installiert.\n\n{dir}");
        }
        return 0;
    }

    private static int HeadlessUninstall(string[] args)
    {
        var quiet = args.Contains("--quiet");
        var notify = args.Contains("--notify");
        var performed = Installer.Uninstall(
            new UninstallOptions { Quiet = quiet, Notify = notify },
            (p, m) => { if (!quiet) Cli.WriteLine($"[{p,3}%] {m}"); });
        if (!performed)
        {
            // Delegated to an elevated child via UAC; it reports its own result.
            return 0;
        }
        Cli.WriteLine("Deinstallation abgeschlossen.");
        if (notify)
        {
            MessageBoxCompat.ShowInfo($"{EditionInfo.ProductName} wurde deinstalliert.");
        }
        return 0;
    }

    private static HeadlessOptions? ParseHeadless(string[] args, bool requireDir)
    {
        var result = new HeadlessOptions();
        for (var i = 0; i < args.Length; i++)
        {
            switch (args[i])
            {
                case "--all-users": result.Scope = InstallScope.AllUsers; break;
                case "--per-user": result.Scope = InstallScope.PerUser; break;
                case "--notify": result.Notify = true; break;
                case "--migrate": result.Migrate = true; break;
                case "--desktop": result.Desktop = true; break;
                case "--no-desktop": result.Desktop = false; break;
                case "--start-menu": result.StartMenu = true; break;
                case "--no-start-menu": result.StartMenu = false; break;
                case "--assoc-nova": result.AssociateNova = true; break;
                case "--assoc-swf": result.AssociateSwf = true; break;
                case "--no-assoc": result.AssociateNova = result.AssociateSwf = false; break;
                case "--quiet": result.Quiet = true; break;
                default:
                    if (!args[i].StartsWith("--", StringComparison.Ordinal)
                        && result.InstallDir is null)
                    {
                        result.InstallDir = args[i];
                    }
                    else
                    {
                        Cli.Error($"Unbekannte Option: {args[i]}");
                        return null;
                    }
                    break;
            }
        }
        if (requireDir && result.InstallDir is null)
        {
            Cli.Error("FEHLER: --install benötigt ein Installationsverzeichnis.");
            return null;
        }
        return result;
    }
}
