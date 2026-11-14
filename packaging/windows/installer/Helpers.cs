using System.Drawing;
using System.Reflection;
using System.Windows.Forms;

namespace NovaFlashStudio.Setup;

/// <summary>
/// Console output for CLI modes. The setup is a WinExe (GUI subsystem), so
/// Console.WriteLine cannot reach redirected pipes; writing through the
/// standard-handle streams works for both consoles and captures.
/// </summary>
internal static class Cli
{
    private static readonly StreamWriter OutWriter = new(
        Console.OpenStandardOutput(), new System.Text.UTF8Encoding(false)) { AutoFlush = true };
    private static readonly StreamWriter ErrWriter = new(
        Console.OpenStandardError(), new System.Text.UTF8Encoding(false)) { AutoFlush = true };

    public static void WriteLine(string text = "") => OutWriter.WriteLine(text);
    public static void WriteLines(IEnumerable<string> lines) { foreach (var l in lines) OutWriter.WriteLine(l); }
    public static void Error(string text) => ErrWriter.WriteLine(text);
}

/// <summary>Small UI helpers shared by the wizard and headless modes.</summary>
internal static class MessageBoxCompat
{
    public static bool ConfirmUninstall()
    {
        var result = MessageBox.Show(
            $"Möchten Sie {EditionInfo.ProductName} wirklich deinstallieren?\n\n" +
            "Ihre Projekte und Einstellungen unter %APPDATA% bleiben erhalten.",
            $"{EditionInfo.ProductName} – Deinstallation",
            MessageBoxButtons.YesNo,
            MessageBoxIcon.Question,
            MessageBoxDefaultButton.Button2);
        return result == DialogResult.Yes;
    }

    public static void ShowError(string message)
    {
        MessageBox.Show(
            message,
            $"{EditionInfo.ProductName} – Setup",
            MessageBoxButtons.OK,
            MessageBoxIcon.Error);
    }

    public static void ShowInfo(string message)
    {
        MessageBox.Show(
            message,
            $"{EditionInfo.ProductName} – Setup",
            MessageBoxButtons.OK,
            MessageBoxIcon.Information);
    }

    public static Image? LoadLogo()
    {
        using var stream = Assembly.GetExecutingAssembly()
            .GetManifestResourceStream("logo.png");
        if (stream is null) return null;
        return Image.FromStream(stream);
    }
}
