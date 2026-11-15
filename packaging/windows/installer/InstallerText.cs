using System.Reflection;
using System.Text.Json;
using Microsoft.Win32;

namespace NovaFlashStudio.Setup;

/// <summary>
/// Installer localization (M24.2). Strings live in embedded
/// <c>i18n/&lt;lang&gt;.json</c> resources — the same five languages as the
/// application. The chosen language is persisted per edition in
/// HKCU\Software\&lt;UninstallKey&gt;\Installer\Language so later runs reuse it.
/// </summary>
internal static class InstallerText
{
    /// <summary>(code, native name) pairs, en-GB fallback.</summary>
    public static readonly (string Code, string Name)[] Languages =
    {
        ("en-GB", "English (UK)"),
        ("de-DE", "Deutsch"),
        ("da-DK", "Dansk"),
        ("fr-FR", "Français"),
        ("pl-PL", "Polski"),
    };

    private static readonly Dictionary<string, Dictionary<string, string>> _tables = new();
    private static string _lang = "en-GB";

    public static string Current => _lang;

    /// <summary>Load all embedded tables and restore the persisted language.</summary>
    public static void Initialize()
    {
        foreach (var (code, _) in Languages)
        {
            var table = new Dictionary<string, string>(StringComparer.Ordinal);
            try
            {
                using var stream = Assembly.GetExecutingAssembly()
                    .GetManifestResourceStream($"i18n.{code}.json");
                if (stream is not null)
                {
                    using var doc = JsonDocument.Parse(stream);
                    foreach (var prop in doc.RootElement.EnumerateObject())
                    {
                        table[prop.Name] = prop.Value.GetString() ?? prop.Name;
                    }
                }
            }
            catch { /* missing/broken table → key fallback below */ }
            _tables[code] = table;
        }
        var stored = LoadStoredLanguage();
        if (stored is not null && _tables.ContainsKey(stored))
        {
            _lang = stored;
        }
    }

    public static bool HasStoredLanguage() => LoadStoredLanguage() is not null;

    /// <summary>Switch language and persist the choice.</summary>
    public static void SetLanguage(string code)
    {
        if (!_tables.ContainsKey(code)) return;
        _lang = code;
        try
        {
            using var key = Registry.CurrentUser.CreateSubKey(
                $@"Software\{EditionInfo.UninstallKey}\Installer");
            key?.SetValue("Language", code, RegistryValueKind.String);
        }
        catch { /* non-fatal: language simply won't persist */ }
    }

    private static string? LoadStoredLanguage()
    {
        try
        {
            using var key = Registry.CurrentUser.OpenSubKey(
                $@"Software\{EditionInfo.UninstallKey}\Installer", writable: false);
            return key?.GetValue("Language") as string;
        }
        catch { return null; }
    }

    /// <summary>Translate a key; {0}/{1}… args are substituted positionally.
    /// Falls back to en-GB, then to the key itself.</summary>
    public static string T(string key, params object?[] args)
    {
        var text = _tables.TryGetValue(_lang, out var t) && t.TryGetValue(key, out var v)
            ? v
            : _tables.TryGetValue("en-GB", out var en) && en.TryGetValue(key, out var ev)
                ? ev
                : key;
        if (args.Length == 0) return text;
        try { return string.Format(text, args); }
        catch { return text; }
    }

    /// <summary>Native display name for a language code.</summary>
    public static string LanguageName(string code) =>
        Languages.FirstOrDefault(l => l.Code == code).Name ?? code;
}
