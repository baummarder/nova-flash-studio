using System.Diagnostics;
using System.IO.Compression;
using System.Reflection;
using System.Security.Cryptography;
using System.Security.Principal;
using System.Text.Json;
using Microsoft.Win32;

namespace NovaFlashStudio.Setup;

/// <summary>Raised for expected, user-facing installer failures.</summary>
public sealed class InstallException : Exception
{
    public InstallException(string message) : base(message) { }
}

public enum InstallScope
{
    /// <summary>%LOCALAPPDATA%\Programs — no administrator rights required.</summary>
    PerUser,

    /// <summary>Program Files + HKLM — requires an elevated process.</summary>
    AllUsers,
}

public sealed class InstallOptions
{
    public required string InstallDir { get; init; }
    public InstallScope Scope { get; init; } = InstallScope.PerUser;
    public bool DesktopShortcut { get; init; } = true;
    public bool StartMenuShortcut { get; init; } = true;
    public bool AssociateNova { get; init; }
    public bool AssociateSwf { get; init; }
    /// <summary>
    /// Explicitly confirmed migration: remove conflicting installations in a
    /// different scope or directory before installing. User data is preserved.
    /// </summary>
    public bool MigrateExisting { get; init; }
}

public sealed class UninstallOptions
{
    public bool Quiet { get; init; }
    /// <summary>Show a completion dialog (used by elevated headless runs).</summary>
    public bool Notify { get; init; }
}

/// <summary>Progress callback: (percent 0-100 or -1, status message).</summary>
public delegate void ProgressReporter(int percent, string message);

/// <summary>A previously recorded artifact that uninstall may remove.</summary>
public sealed class AssociationRecord
{
    public required string Extension { get; set; }
    public required string ProgId { get; set; }
    public required string Hive { get; set; } // "HKCU" or "HKLM"
    /// <summary>Default value the extension had before we registered.</summary>
    public string? PreviousDefault { get; set; }
    /// <summary>True when the ProgId key already existed before installation.</summary>
    public bool ProgIdPreExisting { get; set; }
}

public sealed class InstallManifest
{
    public string Product { get; set; } = Installer.ProductName;
    public string Version { get; set; } = "";
    public string InstallDir { get; set; } = "";
    public string Scope { get; set; } = "";
    public string PayloadSha256 { get; set; } = "";
    public List<string> Files { get; set; } = new();
    public List<string> Shortcuts { get; set; } = new();
    public List<AssociationRecord> Associations { get; set; } = new();
    public string UninstallerFileName { get; set; } = Installer.UninstallerFileName;
}

/// <summary>Install/uninstall engine shared by the wizard and the CLI.</summary>
public static class Installer
{
    // Edition-aware identity (M19): values come from EditionInfo.cs, which
    // package-installer.ps1 regenerates per edition before dotnet publish.
    public static string ProductName => EditionInfo.ProductName;
    public static string AppExeName => EditionInfo.AppExeName;
    public static string UninstallerFileName => EditionInfo.UninstallerFileName;
    public const string ManifestFileName = "install-manifest.json";
    private static string UninstallKeyName => EditionInfo.UninstallKey;
    private static string UninstallRegPath =>
        @"Software\Microsoft\Windows\CurrentVersion\Uninstall\" + UninstallKeyName;

    private static readonly JsonSerializerOptions JsonOptions = new() { WriteIndented = true };

    // ------------------------------------------------------------------
    // Payload
    // ------------------------------------------------------------------

    /// <summary>SHA-256 of the embedded payload ZIP, or null when absent.</summary>
    public static string? PayloadSha256()
    {
        using var stream = OpenPayloadStream();
        if (stream is null) return null;
        return Convert.ToHexString(SHA256.HashData(stream)).ToLowerInvariant();
    }

    public static bool HasPayload => PayloadSha256() is not null;

    private static Stream? OpenPayloadStream()
    {
        var assembly = Assembly.GetExecutingAssembly();
        return assembly.GetManifestResourceStream("payload.zip");
    }

    private static string AppVersion()
    {
        var info = Assembly.GetExecutingAssembly()
            .GetCustomAttribute<AssemblyInformationalVersionAttribute>()?.InformationalVersion;
        return string.IsNullOrWhiteSpace(info) ? "0.0.0" : info;
    }

    public static string? ReadEulaText()
    {
        using var stream = Assembly.GetExecutingAssembly().GetManifestResourceStream("EULA.txt");
        if (stream is null) return null;
        using var reader = new StreamReader(stream);
        return reader.ReadToEnd();
    }

    // ------------------------------------------------------------------
    // Existing installation detection (upgrade handling)
    // ------------------------------------------------------------------

    public sealed class ExistingInstall
    {
        public required string InstallDir { get; init; }
        public string? Version { get; init; }
        public string Scope { get; init; } = "user";
    }

    /// <summary>Finds all previous installations (per-user and/or machine-wide).</summary>
    public static List<ExistingInstall> FindExistingInstalls()
    {
        var found = new List<ExistingInstall>();
        foreach (var hive in new[] { Registry.CurrentUser, Registry.LocalMachine })
        {
            try
            {
                using var key = hive.OpenSubKey(UninstallRegPath, writable: false);
                if (key is null) continue;
                var location = key.GetValue("InstallLocation") as string;
                if (string.IsNullOrWhiteSpace(location)) continue;
                var dir = Environment.ExpandEnvironmentVariables(location.Trim('"'));
                if (!Directory.Exists(dir)) continue;
                found.Add(new ExistingInstall
                {
                    InstallDir = dir,
                    Version = key.GetValue("DisplayVersion") as string,
                    Scope = hive == Registry.LocalMachine ? "machine" : "user",
                });
            }
            catch (System.Security.SecurityException)
            {
                // HKLM may be unreadable without elevation on some policies.
            }
        }
        return found;
    }

    /// <summary>First existing installation, if any (per-user preferred).</summary>
    public static ExistingInstall? FindExistingInstall() =>
        FindExistingInstalls().FirstOrDefault();

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    public static bool IsAdministrator()
    {
        using var identity = WindowsIdentity.GetCurrent();
        var principal = new WindowsPrincipal(identity);
        return principal.IsInRole(WindowsBuiltInRole.Administrator);
    }

    /// <summary>
    /// Relaunches this executable with the standard Windows UAC prompt
    /// (ShellExecute "runas"). Throws InstallException when the user denies
    /// the prompt or elevation is otherwise unavailable — callers must not
    /// fall back to a per-user installation silently.
    /// </summary>
    public static void RelaunchElevated(string arguments)
    {
        var self = Environment.ProcessPath
            ?? throw new InstallException("Der Pfad des Setup-Programms ist unbekannt.");
        try
        {
            Process.Start(new ProcessStartInfo(self, arguments)
            {
                Verb = "runas",
                UseShellExecute = true,
            });
        }
        catch (System.ComponentModel.Win32Exception ex) when (ex.NativeErrorCode == 1223)
        {
            throw new InstallException(
                "Die Administratorrechte wurden abgelehnt. Ohne Elevation ist " +
                "eine Installation für alle Benutzer nicht möglich. Sie können " +
                "es erneut versuchen oder das Setup abbrechen.");
        }
        catch (System.ComponentModel.Win32Exception ex)
        {
            throw new InstallException(
                $"Die elevated Ausführung konnte nicht gestartet werden: {ex.Message}");
        }
    }

    public static string DefaultInstallDir(InstallScope scope) =>
        scope == InstallScope.AllUsers
            ? Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles),
                ProductName)
            : Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                "Programs",
                ProductName);

    private static string UninstallKeyHive(InstallScope scope) =>
        scope == InstallScope.AllUsers ? "HKLM" : "HKCU";

    private static RegistryKey UninstallHive(InstallScope scope) =>
        scope == InstallScope.AllUsers ? Registry.LocalMachine : Registry.CurrentUser;

    private static string ClassesRootName(InstallScope scope) =>
        scope == InstallScope.AllUsers ? @"SOFTWARE\Classes" : @"Software\Classes";

    // ------------------------------------------------------------------
    // Install
    // ------------------------------------------------------------------

    public static void Install(
        InstallOptions options,
        ProgressReporter? progress = null,
        CancellationToken cancellation = default)
    {
        var payloadHash = PayloadSha256()
            ?? throw new InstallException("Das Setup enthält keine Payload (payload.zip fehlt).");
        var installDir = Path.GetFullPath(options.InstallDir);
        var version = AppVersion();

        // Scope/directory conflicts: an existing installation may only be
        // upgraded in place (same scope, same directory). Anything else is a
        // migration and requires explicit opt-in via MigrateExisting.
        var conflicts = FindExistingInstalls()
            .Where(e => IsConflictingInstall(e, options.Scope, installDir))
            .ToList();
        if (conflicts.Count > 0 && !options.MigrateExisting)
        {
            var first = conflicts[0];
            throw new InstallException(
                $"Eine vorhandene Installation ({first.Version ?? "?"}) in " +
                $"{first.InstallDir} [{(first.Scope == "machine" ? "alle Benutzer" : "nur dieser Benutzer")}] " +
                "wurde gefunden. Installationen werden nicht stillschweigend " +
                "parallel oder bereichsübergreifend angelegt — deinstallieren " +
                "Sie die vorhandene Installation zuerst oder bestätigen Sie " +
                "die Migration (entfernt die alte Installation, erhält " +
                "Projekte und Einstellungen).");
        }

        var needsElevation = options.Scope == InstallScope.AllUsers
            || conflicts.Any(e => e.Scope == "machine");
        if (needsElevation && !IsAdministrator())
        {
            if (options.Scope == InstallScope.AllUsers)
            {
                throw new InstallException(
                    "Die Installation für alle Benutzer benötigt Administratorrechte " +
                    "(UAC). Starten Sie das Setup erneut; der Windows-Dialog fragt " +
                    "automatisch nach Erhöhung. Es wurde keine Benutzerinstallation " +
                    "ersatzweise durchgeführt.");
            }
            throw new InstallException(
                "Die Migration erfordert Administratorrechte, weil die vorhandene " +
                "Installation für alle Benutzer entfernt werden muss (UAC).");
        }
        var selfPath = Environment.ProcessPath
            ?? throw new InstallException("Der Pfad des Setup-Programms konnte nicht ermittelt werden.");

        progress?.Invoke(2, "Payload wird geprüft …");
        var tempRoot = Path.Combine(Path.GetTempPath(), "nova-setup-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(tempRoot);

        try
        {
            // --- Extract payload -------------------------------------------------
            progress?.Invoke(5, "Programmdateien werden entpackt …");
            using (var zipStream = OpenPayloadStream())
            using (var archive = new ZipArchive(zipStream!, ZipArchiveMode.Read))
            {
                foreach (var entry in archive.Entries)
                {
                    cancellation.ThrowIfCancellationRequested();
                    var destination = Path.GetFullPath(Path.Combine(tempRoot, entry.FullName));
                    var rootWithSep = tempRoot.TrimEnd(Path.DirectorySeparatorChar)
                        + Path.DirectorySeparatorChar;
                    if (!destination.StartsWith(rootWithSep, StringComparison.OrdinalIgnoreCase)
                        && !string.Equals(destination, tempRoot, StringComparison.OrdinalIgnoreCase))
                    {
                        throw new InstallException(
                            $"Unsicherer Pfad im Archiv: {entry.FullName}");
                    }
                    if (string.IsNullOrEmpty(entry.Name))
                    {
                        Directory.CreateDirectory(destination);
                        continue;
                    }
                    Directory.CreateDirectory(Path.GetDirectoryName(destination)!);
                    entry.ExtractToFile(destination, overwrite: true);
                }
            }

            // The portable ZIP wraps everything in a single product folder; copy
            // the *contents* of that folder into the install directory.
            var payloadRoot = tempRoot;
            var topEntries = Directory.GetFileSystemEntries(tempRoot);
            if (topEntries.Length == 1 && Directory.Exists(topEntries[0]))
            {
                payloadRoot = topEntries[0];
            }

            // --- Confirmed migration: remove conflicting installs ---------------
            // The new payload is already fully extracted and verified, so a
            // failure here cannot leave the system without *any* payload
            // staged; the old install's files are only removed after that.
            if (conflicts.Count > 0)
            {
                progress?.Invoke(14, "Vorhandene Installation wird migriert …");
                foreach (var conflict in conflicts)
                {
                    RemoveInstallDir(conflict.InstallDir, conflict.Scope, progress);
                }
            }

            // --- Upgrade: remove artifacts recorded by the previous install -----
            var manifestPath = Path.Combine(installDir, ManifestFileName);
            InstallManifest? previous = null;
            if (File.Exists(manifestPath))
            {
                try
                {
                    previous = JsonSerializer.Deserialize<InstallManifest>(
                        File.ReadAllText(manifestPath));
                }
                catch (JsonException) { /* corrupt manifest: install fresh */ }
            }

            progress?.Invoke(15, "Vorherige Installation wird aktualisiert …");
            if (previous is not null)
            {
                RemoveRecordedArtifacts(previous, selfPath, progress);
            }

            // --- Copy files ------------------------------------------------------
            Directory.CreateDirectory(installDir);
            var newFiles = new List<string>();
            var allFiles = Directory.GetFiles(payloadRoot, "*", SearchOption.AllDirectories);
            var total = Math.Max(allFiles.Length, 1);
            var done = 0;
            foreach (var source in allFiles)
            {
                cancellation.ThrowIfCancellationRequested();
                var relative = Path.GetRelativePath(payloadRoot, source);
                var destination = Path.Combine(installDir, relative);
                Directory.CreateDirectory(Path.GetDirectoryName(destination)!);
                File.Copy(source, destination, overwrite: true);
                newFiles.Add(relative.Replace('\\', '/'));
                done++;
                progress?.Invoke(
                    15 + (int)(70.0 * done / total),
                    $"Installiere {Path.GetFileName(source)} …");
            }

            // Delete files that existed in the previous version but not in this one.
            if (previous is not null)
            {
                var newSet = new HashSet<string>(newFiles, StringComparer.OrdinalIgnoreCase);
                foreach (var stale in previous.Files)
                {
                    if (newSet.Contains(stale)) continue;
                    var stalePath = Path.Combine(installDir, stale.Replace('/', Path.DirectorySeparatorChar));
                    try { File.Delete(stalePath); } catch (IOException) { }
                    catch (UnauthorizedAccessException) { }
                }
            }

            // --- Uninstaller copy -------------------------------------------------
            progress?.Invoke(88, "Deinstallationsprogramm wird erstellt …");
            var uninstallerPath = Path.Combine(installDir, UninstallerFileName);
            File.Copy(selfPath, uninstallerPath, overwrite: true);

            // --- Manifest ----------------------------------------------------------
            var manifest = new InstallManifest
            {
                Version = version,
                InstallDir = installDir,
                Scope = UninstallKeyHive(options.Scope),
                PayloadSha256 = payloadHash,
                Files = newFiles.OrderBy(f => f, StringComparer.OrdinalIgnoreCase).ToList(),
            };
            File.WriteAllText(
                manifestPath,
                JsonSerializer.Serialize(manifest, JsonOptions));

            // --- Shortcuts ----------------------------------------------------------
            progress?.Invoke(92, "Verknüpfungen werden erstellt …");
            var exePath = Path.Combine(installDir, AppExeName);
            if (options.StartMenuShortcut)
            {
                var startMenuDir = options.Scope == InstallScope.AllUsers
                    ? Path.Combine(
                        Environment.GetFolderPath(Environment.SpecialFolder.CommonPrograms),
                        ProductName)
                    : Path.Combine(
                        Environment.GetFolderPath(Environment.SpecialFolder.Programs),
                        ProductName);
                var lnk = Path.Combine(startMenuDir, ProductName + ".lnk");
                CreateShortcut(lnk, exePath, installDir);
                manifest.Shortcuts.Add(lnk);
            }
            if (options.DesktopShortcut)
            {
                var desktop = options.Scope == InstallScope.AllUsers
                    ? Environment.GetFolderPath(Environment.SpecialFolder.CommonDesktopDirectory)
                    : Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory);
                var lnk = Path.Combine(desktop, ProductName + ".lnk");
                CreateShortcut(lnk, exePath, installDir);
                manifest.Shortcuts.Add(lnk);
            }
            // Re-write manifest so shortcuts are recorded.
            File.WriteAllText(manifestPath, JsonSerializer.Serialize(manifest, JsonOptions));

            // --- File associations ---------------------------------------------------
            progress?.Invoke(96, "Dateizuordnungen werden registriert …");
            if (options.AssociateNova) RegisterAssociation(".nova", EditionInfo.ProgIdNova, options.Scope, manifest);
            if (options.AssociateSwf) RegisterAssociation(".swf", EditionInfo.ProgIdSwf, options.Scope, manifest);
            File.WriteAllText(manifestPath, JsonSerializer.Serialize(manifest, JsonOptions));

            // --- Uninstall registry entry ---------------------------------------------
            progress?.Invoke(98, "Registrierung wird geschrieben …");
            using (var key = UninstallHive(options.Scope).CreateSubKey(UninstallRegPath, writable: true))
            {
                key.SetValue("DisplayName", ProductName);
                key.SetValue("DisplayVersion", version);
                key.SetValue("DisplayIcon", $"\"{exePath}\",0");
                key.SetValue("InstallLocation", $"\"{installDir}\\");
                key.SetValue("Publisher", "NØVA Flash Studio contributors");
                key.SetValue("UninstallString", $"\"{uninstallerPath}\" --uninstall");
                key.SetValue("QuietUninstallString", $"\"{uninstallerPath}\" --uninstall --quiet");
                key.SetValue("NoModify", 1, RegistryValueKind.DWord);
                key.SetValue("NoRepair", 1, RegistryValueKind.DWord);
                key.SetValue("EstimatedSize", EstimateSizeKb(installDir), RegistryValueKind.DWord);
            }

            progress?.Invoke(100, "Installation abgeschlossen.");
        }
        finally
        {
            try { Directory.Delete(tempRoot, recursive: true); } catch (IOException) { }
        }
    }

    /// <summary>True when an existing install differs in scope or directory.</summary>
    private static bool IsConflictingInstall(
        ExistingInstall existing, InstallScope scope, string installDir)
    {
        var scopeName = scope == InstallScope.AllUsers ? "machine" : "user";
        if (!string.Equals(existing.Scope, scopeName, StringComparison.OrdinalIgnoreCase))
            return true;
        var a = Path.GetFullPath(existing.InstallDir)
            .TrimEnd(Path.DirectorySeparatorChar);
        var b = installDir.TrimEnd(Path.DirectorySeparatorChar);
        return !string.Equals(a, b, StringComparison.OrdinalIgnoreCase);
    }

    /// <summary>
    /// Removes a conflicting installation during a confirmed migration:
    /// recorded files, shortcuts, associations, uninstall entry and the
    /// uninstaller copy, then empty directories. User data is never touched.
    /// </summary>
    private static void RemoveInstallDir(
        string dir, string scopeName, ProgressReporter? progress)
    {
        var fullDir = Path.GetFullPath(dir);
        var manifest = LoadManifest(Path.Combine(fullDir, ManifestFileName));
        if (manifest is not null)
        {
            RemoveRecordedArtifacts(
                manifest, Environment.ProcessPath ?? string.Empty, progress);
        }
        else
        {
            // No readable manifest: still remove our uninstall registry entry.
            var hive = scopeName == "machine" ? Registry.LocalMachine : Registry.CurrentUser;
            try { hive.DeleteSubKey(UninstallRegPath, throwOnMissingSubKey: false); }
            catch (System.Security.SecurityException) { }
        }

        foreach (var name in new[] { UninstallerFileName, ManifestFileName })
        {
            try { File.Delete(Path.Combine(fullDir, name)); }
            catch (IOException) { }
            catch (UnauthorizedAccessException) { }
        }

        try
        {
            foreach (var sub in Directory.GetDirectories(
                fullDir, "*", SearchOption.AllDirectories)
                .OrderByDescending(p => p.Length))
            {
                try { Directory.Delete(sub); } catch (IOException) { }
                catch (UnauthorizedAccessException) { }
            }
            Directory.Delete(fullDir);
        }
        catch (DirectoryNotFoundException) { }
        catch (IOException) { }
        catch (UnauthorizedAccessException) { }
    }

    private static int EstimateSizeKb(string dir)
    {
        try
        {
            return (int)(new DirectoryInfo(dir)
                .EnumerateFiles("*", SearchOption.AllDirectories)
                .Sum(f => f.Length) / 1024);
        }
        catch (IOException) { return 0; }
        catch (UnauthorizedAccessException) { return 0; }
    }

    private static void CreateShortcut(string path, string target, string workingDir)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        var shellType = Type.GetTypeFromProgID("WScript.Shell")
            ?? throw new InstallException("WScript.Shell ist nicht verfügbar.");
        dynamic shell = Activator.CreateInstance(shellType)!;
        dynamic link = shell.CreateShortcut(path);
        link.TargetPath = target;
        link.WorkingDirectory = workingDir;
        link.IconLocation = $"\"{target}\",0";
        link.Description = ProductName;
        link.Save();
    }

    private static void DeleteShortcut(string path)
    {
        try { File.Delete(path); } catch (IOException) { }
        catch (UnauthorizedAccessException) { }
    }

    private static void RegisterAssociation(
        string extension, string progId, InstallScope scope, InstallManifest manifest)
    {
        var hive = scope == InstallScope.AllUsers ? Registry.LocalMachine : Registry.CurrentUser;
        var exePath = Path.Combine(manifest.InstallDir, AppExeName);

        var record = new AssociationRecord
        {
            Extension = extension,
            ProgId = progId,
            Hive = UninstallKeyHive(scope),
        };

        using (var extKey = hive.CreateSubKey(
            ClassesRootName(scope) + "\\" + extension, writable: true))
        {
            var old = extKey.GetValue(null) as string;
            record.PreviousDefault = string.IsNullOrEmpty(old) ? null : old;
            extKey.SetValue(null, progId);
        }

        var progIdExisted = hive.GetValue(
            ClassesRootName(scope) + "\\" + progId + "\\shell\\open\\command",
            null) is not null;
        record.ProgIdPreExisting = progIdExisted;

        using (var progKey = hive.CreateSubKey(
            ClassesRootName(scope) + "\\" + progId, writable: true))
        {
            progKey.SetValue(null, $"{ProductName} File");
        }
        using (var iconKey = hive.CreateSubKey(
            ClassesRootName(scope) + $"\\{progId}\\DefaultIcon", writable: true))
        {
            iconKey.SetValue(null, $"\"{exePath}\",0");
        }
        using (var cmdKey = hive.CreateSubKey(
            ClassesRootName(scope) + $"\\{progId}\\shell\\open\\command", writable: true))
        {
            cmdKey.SetValue(null, $"\"{exePath}\" \"%1\"");
        }

        manifest.Associations.Add(record);
    }

    private static void RemoveAssociation(AssociationRecord record, string exePath)
    {
        var hive = record.Hive == "HKLM" ? Registry.LocalMachine : Registry.CurrentUser;
        var root = record.Hive == "HKLM" ? @"SOFTWARE\Classes" : @"Software\Classes";
        var scope = record.Hive == "HKLM" ? InstallScope.AllUsers : InstallScope.PerUser;

        try
        {
            // Only remove the ProgId if the open command still points to us.
            var cmdPath = $"{root}\\{record.ProgId}\\shell\\open\\command";
            using (var cmdKey = hive.OpenSubKey(cmdPath, writable: false))
            {
                var command = cmdKey?.GetValue(null) as string;
                var ours = command is not null
                    && command.Contains(exePath, StringComparison.OrdinalIgnoreCase);
                if (ours && !record.ProgIdPreExisting)
                {
                    hive.DeleteSubKeyTree($"{root}\\{record.ProgId}", throwOnMissingSubKey: false);
                }
            }

            // Restore or remove the extension default only if it still points
            // at our ProgId.
            using (var extKey = hive.OpenSubKey(
                $"{root}\\{record.Extension}", writable: true))
            {
                if (extKey is null) return;
                var current = extKey.GetValue(null) as string;
                if (!string.Equals(current, record.ProgId, StringComparison.OrdinalIgnoreCase))
                    return; // user or another app changed it — leave alone
                if (record.PreviousDefault is null)
                {
                    extKey.DeleteValue(null!, throwOnMissingValue: false);
                    extKey.DeleteValue(string.Empty, throwOnMissingValue: false);
                }
                else
                {
                    extKey.SetValue(null, record.PreviousDefault);
                }
            }
        }
        catch (System.Security.SecurityException)
        {
            // HKLM cleanup without elevation: leave the keys, they are inert.
        }
    }

    private static void RemoveRecordedArtifacts(
        InstallManifest manifest, string selfPath, ProgressReporter? progress)
    {
        var exePath = Path.Combine(manifest.InstallDir, AppExeName);
        foreach (var record in manifest.Associations)
        {
            RemoveAssociation(record, exePath);
        }
        foreach (var shortcut in manifest.Shortcuts)
        {
            DeleteShortcut(shortcut);
        }
        foreach (var file in manifest.Files)
        {
            var path = Path.Combine(
                manifest.InstallDir, file.Replace('/', Path.DirectorySeparatorChar));
            try { File.Delete(path); } catch (IOException) { }
            catch (UnauthorizedAccessException) { }
        }
        try { File.Delete(Path.Combine(manifest.InstallDir, ManifestFileName)); } catch (IOException) { }

        var hive = manifest.Scope == "HKLM" ? Registry.LocalMachine : Registry.CurrentUser;
        try { hive.DeleteSubKey(UninstallRegPath, throwOnMissingSubKey: false); }
        catch (System.Security.SecurityException) { }
    }

    // ------------------------------------------------------------------
    // Uninstall
    // ------------------------------------------------------------------

    /// <summary>Locates the install manifest belonging to this executable.</summary>
    public static InstallManifest? LocateManifestForCurrentExe()
    {
        var selfPath = Environment.ProcessPath;
        if (selfPath is null) return null;
        var dir = Path.GetDirectoryName(selfPath)!;
        var local = Path.Combine(dir, ManifestFileName);
        if (File.Exists(local))
        {
            return LoadManifest(local);
        }

        // Running copy is not inside an install (e.g. --uninstall on the dist
        // setup exe): resolve the registered InstallLocation instead.
        var existing = FindExistingInstall();
        if (existing is null) return null;
        var remote = Path.Combine(existing.InstallDir, ManifestFileName);
        return File.Exists(remote) ? LoadManifest(remote) : null;
    }

    private static InstallManifest? LoadManifest(string path)
    {
        try
        {
            return JsonSerializer.Deserialize<InstallManifest>(File.ReadAllText(path));
        }
        catch (Exception ex) when (ex is IOException or JsonException or UnauthorizedAccessException)
        {
            return null;
        }
    }

    /// <summary>
    /// Removes the installation. Returns false when the work was delegated to
    /// an elevated child process (machine-wide uninstall without admin rights).
    /// </summary>
    public static bool Uninstall(
        UninstallOptions options,
        ProgressReporter? progress = null,
        Func<bool>? confirmOverride = null)
    {
        var manifest = LocateManifestForCurrentExe()
            ?? throw new InstallException(
                "Keine Installation von NØVA Flash Studio gefunden. " +
                "Das Deinstallationsprogramm muss sich im Installationsverzeichnis befinden.");

        // Machine-wide installs write HKLM/Program Files: require elevation.
        // Relaunch through the standard UAC prompt instead of failing silently.
        if (manifest.Scope == "HKLM" && !IsAdministrator())
        {
            var args = "--uninstall" + (options.Quiet ? " --quiet" : "")
                + (options.Notify ? " --notify" : "");
            RelaunchElevated(args);
            return false;
        }

        var installDir = Path.GetFullPath(manifest.InstallDir);
        var selfPath = Environment.ProcessPath
            ?? throw new InstallException("Der Pfad des Deinstallationsprogramms ist unbekannt.");
        var uninstallerPath = Path.Combine(installDir, UninstallerFileName);

        if (!options.Quiet)
        {
            var answer = confirmOverride?.Invoke() ?? MessageBoxCompat.ConfirmUninstall();
            if (!answer) return true;
        }

        progress?.Invoke(10, "Dateizuordnungen werden entfernt …");
        var exePath = Path.Combine(installDir, AppExeName);
        foreach (var record in manifest.Associations)
        {
            RemoveAssociation(record, exePath);
        }

        progress?.Invoke(30, "Verknüpfungen werden entfernt …");
        foreach (var shortcut in manifest.Shortcuts)
        {
            DeleteShortcut(shortcut);
        }

        progress?.Invoke(50, "Dateien werden entfernt …");
        foreach (var file in manifest.Files)
        {
            var path = Path.Combine(
                installDir, file.Replace('/', Path.DirectorySeparatorChar));
            try { File.Delete(path); } catch (IOException) { }
            catch (UnauthorizedAccessException) { }
        }
        try { File.Delete(Path.Combine(installDir, ManifestFileName)); }
        catch (IOException) { }

        var hive = manifest.Scope == "HKLM" ? Registry.LocalMachine : Registry.CurrentUser;
        try { hive.DeleteSubKey(UninstallRegPath, throwOnMissingSubKey: false); }
        catch (System.Security.SecurityException) { }

        progress?.Invoke(90, "Aufräumen …");
        // Remove now-empty subdirectories synchronously (rd without /s only
        // deletes empty directories, so foreign files are never touched).
        try
        {
            foreach (var sub in Directory.GetDirectories(
                installDir, "*", SearchOption.AllDirectories)
                .OrderByDescending(p => p.Length))
            {
                try { Directory.Delete(sub); } catch (IOException) { }
                catch (UnauthorizedAccessException) { }
            }
        }
        catch (DirectoryNotFoundException) { }
        ScheduleSelfAndDirCleanup(selfPath, installDir);
        progress?.Invoke(100, "Deinstallation abgeschlossen.");
        return true;
    }

    /// <summary>
    /// A running process cannot delete itself on Windows. Spawn a detached
    /// cmd.exe that waits a moment, deletes this executable and then removes
    /// now-empty directories bottom-up (rd without /s never deletes foreign
    /// files). User data under %APPDATA% is never touched.
    /// </summary>
    private static void ScheduleSelfAndDirCleanup(string selfPath, string installDir)
    {
        // Only the install directory itself is removed (and only when empty —
        // rd without /s refuses non-empty directories). Ancestor directories
        // such as %LOCALAPPDATA%\Programs may contain other applications.
        var commands = new List<string>
        {
            "ping 127.0.0.1 -n 3 >nul",
            $"del /f /q \"{selfPath}\"",
            $"rd \"{installDir}\"",
        };
        var commandLine = string.Join(" & ", commands);
        Process.Start(new ProcessStartInfo
        {
            FileName = "cmd.exe",
            Arguments = "/c " + commandLine,
            CreateNoWindow = true,
            UseShellExecute = false,
            WindowStyle = ProcessWindowStyle.Hidden,
        });
    }

    /// <summary>Opens the installed application, if present.</summary>
    public static void LaunchInstalledApp(string installDir)
    {
        var exe = Path.Combine(installDir, AppExeName);
        if (File.Exists(exe))
        {
            Process.Start(new ProcessStartInfo(exe) { UseShellExecute = true });
        }
    }

    public static string? PayloadResourceName => "payload.zip";
}
