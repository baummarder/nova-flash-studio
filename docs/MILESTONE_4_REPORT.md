# Milestone 4 – Abschlussbericht

**Stand:** 2026-11-11  
**Umfang:** Branding, Splash, Windows-Ressourcen, Packaging, Flash-Player-Integration und Dokumentation.

## Ergebnis

Milestone 4 ist funktional abgeschlossen. Die Anwendung wurde für Windows gebaut,
ihre eingebetteten Ressourcen wurden geprüft und die vorhandenen Packaging- und
Integrationspfade wurden erneut getestet. Es wurden für diesen Abschlussbericht
keine funktionalen Änderungen an Milestone 4 vorgenommen.

## Erneut ausgeführte Prüfungen

| Prüfung | Ergebnis |
| --- | --- |
| `cargo fmt --all -- --check` | sauber, keine Formatabweichungen |
| `cargo test --workspace` | 167 Tests bestanden, 0 fehlgeschlagen |
| `python scripts/verify-packaging.py` | alle Icon-, Container- und Metadatenprüfungen bestanden |
| Windows-Release-Build | erfolgreich |
| `NØVA Flash Studio.exe --version` | `NØVA Flash Studio 0.2.0` |
| `NØVA Flash Studio.exe --startup-check` | Fenster-Icon, Player-Status und Konfigurationspfad erfolgreich gemeldet |
| EXE-Metadaten | `ProductName=NØVA Flash Studio`, `FileVersion=0.2.0`, `ProductVersion=0.2.0` |
| EXE-Icon | 32-px-Ressource pixelidentisch mit `assets/branding/nova-logo-32.png` |
| ZIP-Integrität | `zipfile.testzip()` liefert `None`; 11 Einträge |

## Vorhandene Release-Artefakte

```text
dist/windows/
  NØVA Flash Studio/
    NØVA Flash Studio.exe
    README.md
    docs/*.md
  NOVA-Flash-Studio-0.2.0-windows-x64.zip
  SHA256SUMS.txt
```

Die EXE ist etwa 18 MB groß, das ZIP etwa 7,4 MB. Die SHA256-Summen werden bei
jedem erneuten Packaging neu erzeugt.

## Visuelle Prüfung

Der GUI-Smoke-Test zeigte Splash, Hauptfenster, das Menü **Flash Player**, die
Aktion **Open in Flash Player**, den Einstellungsdialog und die Fehlermeldung
ohne vorhandenen SWF-Export. Der getestete Projector-Pfad
`C:/Windows/notepad.exe` wurde strukturell als Windows-EXE erkannt; es wurde
kein Player gestartet und keine Einstellung gespeichert.

## Einschränkungen

- Eine echte SWF-Wiedergabe wurde nicht verifiziert, weil auf der
  Entwicklungsmaschine weder ein Adobe Flash Player Projector noch Ruffle
  installiert ist.
- macOS- und Linux-Artefakte wurden in dieser Sitzung nicht auf ihren nativen
  Hosts gebaut. Ihre Skripte und Metadaten wurden nur strukturell geprüft.
- Die Windows-Artefakte sind nicht signiert. Ohne Code-Signing-Zertifikat ist
  keine Authenticode-Signatur möglich.
- Es existiert in diesem Arbeitsverzeichnis kein Git-Repository. Der Stand wurde
  durch die erneute Prüfung und die unveränderten Quelldateien gesichert; es
  wurde kein Commit erzeugt.
- Die Lizenzangabe `MIT OR Apache-2.0` stammt aus `Cargo.toml` und ist noch keine
  bestätigte, rechtlich geprüfte Endentscheidung des Projektverantwortlichen.

## Freigabe für Milestone 5

Der Milestone-4-Stand ist unverändert lauffähig und als Ausgangspunkt für
Milestone 5 geeignet. Milestone 5 ergänzt Installer, Portable-Distribution,
Lizenzstruktur, Deinstallation und Release-Automatisierung, ohne die geprüften
Milestone-4-Funktionen zu ersetzen.
