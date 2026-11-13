# Milestone 5 – Windows-Installer, Portable Distribution & Lizenzierung

**Stand:** 2026-11-11 (Revision: All-Users-Standard, UAC, deutsche EULA)
**Umfang:** Windows-Setup-Assistent, Portable ZIP, Installation/Deinstallation,
Dateizuordnungen, Upgrade-Behandlung, Lizenzdateien, Prüfsummen und
Release-Automatisierung. Milestone 4 blieb unverändert funktionsfähig; es
wurden keine Anwendungs-Funktionen ersetzt.

## Revision 2026-11-11 (nach Erstimplementierung)

- **All-Users ist jetzt Standard:** Der Assistent und der Headless-Modus
  installieren standardmäßig nach `C:\Program Files\NØVA Flash Studio` mit
  HKLM-Registrierung und allen Benutzer-Startmenü-Verknüfungen.
- **UAC-Elevation:** Ist der Prozess nicht elevated, startet die Installation
  über `ShellExecute`/`runas` neu und übergibt alle gewählten Optionen. Eine
  abgelehnte UAC-Abfrage zeigt eine Fehlermeldung; es gibt **kein** stilles
  Zurückfallen auf eine Benutzerinstallation. Die maschinenweite Deinstallation
  fordert ebenfalls UAC-Elevation an.
- **Keine Key/Activation-Konzepte:** Im gesamten Installer und der Anwendung
  existieren keine Produkt-/Lizenzschlüssel, Aktivierungen, Online-Prüfungen,
  Konten oder Telemetrie (per Code-Suche verifiziert).
- **Deutsche EULA** (`legal/EULA-DRAFT.md` neu gefasst): kostenlose Nutzung
  ohne Schlüssel/Aktivierung, Offline-Fähigkeit, Verhältnis zu MIT/Apache-2.0,
  Drittkomponenten, Backup-Verantwortung, Gewährleistungsausschluss und
  Haftungsbeschränkung (nur soweit gesetzlich zulässig), Weitergabe/Modifikation
  nach Open-Source-Lizenzen, keine Konto-/Telemetrie-Anforderungen. Deutlich als
  **ENTWURF** markiert; Platzhalter für Verantwortlichen/Kontakt; kein
  erfundener Rechtsinhaber.
- **`docs/LICENSING.md`** neu: Lizenzüberblick, No-Activation/No-Telemetry-
  Grundsatz, Third-Party-Notices und EULA-Status.
- **Lizenzseite des Assistenten** zeigt weiterhin die EULA und verlangt
  Zustimmung vor der Installation.

### Geänderte Dateien (Revision)

```text
packaging/windows/installer/
  InstallEngine.cs   UAC-Relaunch, HKLM-Deinstallations-Elevation, Meldungen
  WizardForm.cs      All-Users-Standard, Elevations-Übergabe, Options-Texte
  Program.cs         --per-user, --notify, Headless-Standard AllUsers
legal/EULA-DRAFT.md  deutsche Neufassung
docs/LICENSING.md    NEU
docs/PACKAGING.md    Installer-Abschnitt + Verifikationsnachweise
README.md            Installationsbeschreibung + Lizenzabschnitt
scripts/verify-packaging.py  EULA-/Elevation-/Scope-Checks
```

### Verifikation der Revision (Windows, 2026-11-11, echte UAC-Prompts)

| Prüfung | Ergebnis |
| --- | --- |
| `--install --all-users` ohne Elevation | Exit 2, klare Meldung, **nichts** installiert (kein stiller Fallback) |
| Elevierter All-Users-Install (`-Verb RunAs`, UAC bestätigt) | Exit 0; HKLM-Uninstall-Key, HKLM-`.nova`-Zuordnung, Manifest `HKLM` |
| All-Users-Shortcuts (Startmenü + Desktop) | unter `%ProgramData%` bzw. `%PUBLIC%\Desktop` erstellt und deinstalliert |
| Maschinelle Deinstallation ohne Elevation | UAC-Relaunch; HKLM-Keys, Dateien, Zuordnungen entfernt |
| Per-User-Install/Deinstall (`--per-user`) | unverändert sauber, ohne Elevation |
| `--smoke-test` / `--version` | OK; EULA jetzt ~4,9 KB deutsch |
| `verify-packaging.py` | alle Checks bestanden |

## Scope-Korrektur 2026-11-11 (nach der Revision)

Die vorherige Revision hatte „alle Benutzer" zum Standard gemacht. Korrigiert:

- **Standard ist wieder die Per-User-Installation**
  (`%LOCALAPPDATA%\Programs\NØVA Flash Studio`, HKCU, Benutzer-Startmenü/-
  Desktop, keine Elevation, kein UAC). „Für alle Benutzer" bleibt eine
  explizite Wahl mit UAC-Hinweis und -Elevation.
- **Scope-Trennung beim Upgrade:** `FindExistingInstalls()` prüft HKCU und
  HKLM getrennt. Eine Installation darf nur **am selben Ort und in
  demselben Bereich** aktualisiert werden; abweichender Bereich oder
  Verzeichnis wird mit klarer Begründung abgelehnt (Wizard-Validierung
  **und** Engine-Check `InstallEngine.Install`). Es entstehen keine
  parallelen oder bereichsübergreifenden Installationen.
- Headless-Standard ebenfalls Per-User (`--per-user`); `--all-users`
  bleibt explizit.
- Verknüpfungen landen im richtigen Geltungsbereich (CommonPrograms/
  CommonDesktopDirectory für Machine, Programs/DesktopDirectory für
  Benutzer).

### Verifikation der Scope-Korrektur (Windows, 2026-11-11)

| Szenario | Ergebnis |
| --- | --- |
| Per-User-Standard-Install ohne Elevation | Exit 0; HKCU-Key, Benutzer-Shortcuts, kein UAC |
| Per-User-Deinstallation ohne Elevation | Exit 0; alles entfernt, `%APPDATA%` unberührt |
| All-Users-Install mit UAC | Exit 0; HKLM-Key, Machine-Shortcuts, HKLM-`.nova` |
| All-Users-Upgrade (gleiches Verzeichnis) | Exit 0 |
| Scope-Mix-Versuch (per-user in anderes Verzeichnis bei bestehender Machine-Installation) | Exit 2, nichts installiert |
| All-Users-Deinstallation mit UAC | HKLM-Keys, Machine-Shortcuts, Dateien entfernt |
| UAC-Ablehnung | **nicht getestet** (benötigt interaktives „Nein"); Codepfad mit klarer Fehlermeldung vorhanden |
| Portable ZIP | unverändert unabhängig (keine Registry/Shortcuts) |

## Implementierte Funktionen

### Portable Distribution

- `dist/windows/NØVA Flash Studio/` enthält jetzt zusätzlich zu EXE, README und
  `docs/` die Dateien `EULA-DRAFT.md`, `LICENSE-MIT.txt`,
  `LICENSE-APACHE-2.0.txt`, `THIRD-PARTY-NOTICES.md` und `licenses/` mit 15
  SPDX-Lizenztexten.
- `NOVA-Flash-Studio-0.2.0-windows-x64.zip` ist die portable Distribution:
  entpacken und `NØVA Flash Studio.exe` starten, ohne Installation oder
  Admin-Rechte.

### Windows-Installer

- `NOVA-Flash-Studio-0.2.0-windows-x64-Setup.exe` (ca. 75,7 MB): selbst
  enthaltene, Single-File-.NET-8-WinForms-Application. Kein WiX/Inno/NSIS
  nötig; .NET 8 SDK wird nur für den Build benötigt.
- Assistent mit NØVA-Logo und deutscher Oberfläche (wie die Anwendung):
  Willkommens-, Lizenz- (akzeptanzpflichtig), Installationsort-, Optionen-,
  Fortschritts- und Abschlussseite.
- Installationsmodi:
  - **Nur für mich** (Standard): `%LOCALAPPDATA%\Programs\NØVA Flash Studio`,
    keine elevation nötig;
  - **Für alle Benutzer**: `C:\Program Files` + HKLM, erfordert einen
    Prozess mit Administratorrechten (wird erkannt und als Hinweis
    angezeigt, nicht still eskaliert);
  - **Benutzerdefiniertes Verzeichnis** mit Browse-Dialog.
- **Dateizuordnungen (opt-in):** `.nova` (Standard an) und `.swf`
  (Standard aus) unter `HKCU\Software\Classes` bzw. `HKLM\Software\Classes`.
  Der bisherige Standardwert wird im Manifest gespeichert und bei der
  Deinstallation wiederhergestellt.
- **Upgrade:** vorhandene Installation wird über die
  `Uninstall\NOVAFlashStudio`-Registry-Einträge gefunden, Verzeichnis wird
  vorbefüllt, Dateien des alten Manifests werden entfernt, Verknüpfungen und
  Zuordnungen nach aktuellen Optionen neu geschrieben.
- **Deinstaller:** `NOVA Flash Studio Uninstall.exe` wird ins Installations-
  verzeichnis kopiert und unter `UninstallString`/`QuietUninstallString`
  registriert. `%APPDATA%\NOVA Flash Studio` (Projekte/Einstellungen) wird
  niemals angefasst; nur Manifest-Dateien, registrierte Verknüpfungen,
  selbst erstellte Registry-Einträge und leere Verzeichnisse werden entfernt.
  Geänderte Dateizuordnungen bleiben unangetastet.
- **Headless-Modi** für Tests/CI: `--install <dir>`, `--uninstall`,
  `--smoke-test`, `--version`, `--help`.

### Lizenzierung & Notices

- `legal/LICENSE-MIT.txt`, `legal/LICENSE-APACHE-2.0.txt` (Spiegel von
  `Cargo.toml` `license = "MIT OR Apache-2.0"`).
- `legal/THIRD-PARTY-NOTICES.md`: 466 Drittanbieter-Crates mit
  Lizenz-Expressions, generiert durch `scripts/gen-third-party-notices.py`
  aus `cargo metadata`.
- `legal/EULA-DRAFT.md`: EULA-Entwurf mit klaren[DRAFT]-Markierungen und
  Platzhaltern für Verlag/Jurisdiktion – **nicht** rechtsgeprüft.
- `LICENSES/` (15 SPDX-Texte) wird als `licenses/` in jedes Bundle gelegt.

### Automatisierung & Doku

- `scripts/package-installer.ps1`: Payload aus altem ZIP oder Neubau,
  `dotnet publish` (win-x64, self-contained, single-file, komprimiert),
  `--version`/`--smoke-test`-Prüfung, SHA256-Nachtrag in `SHA256SUMS.txt`.
- `scripts/build-release.ps1 -Target windows` baut jetzt ZIP + Installer
  (`-SkipInstaller` wählbar).
- `.github/workflows/release.yml`: Windows-Job publiziert zusätzlich den
  Setup-Installer (`windows-latest` bringt das .NET SDK mit).
- `scripts/verify-packaging.py` prüft legal/, Installer-Quellen und
  Skript-/Workflow-Verdrahtung.

## Geänderte/neue Dateien

```text
legal/                                    NEU
  LICENSE-MIT.txt  LICENSE-APACHE-2.0.txt
  THIRD-PARTY-NOTICES.md  EULA-DRAFT.md
scripts/
  gen-third-party-notices.py              NEU
  package-installer.ps1                   NEU
  package-windows.ps1                     GEÄNDERT (legal/ + licenses/ im Bundle)
  build-release.ps1                       GEÄNDERT (Installer-Dispatch)
  verify-packaging.py                     GEÄNDERT (Legal-/Installer-Checks)
packaging/windows/installer/              NEU
  NovaFlashStudio.Setup.csproj  app.manifest
  Program.cs  InstallEngine.cs  WizardForm.cs  Helpers.cs
.github/workflows/release.yml             GEÄNDERT (Installer-Step)
docs/PACKAGING.md  README.md              GEÄNDERT
docs/MILESTONE_5_REPORT.md                NEU (dieser Bericht)
```

Keine Rust-/Anwendungsquellen wurden geändert; SWF-Export, AS3-Compiler,
Animationseditor und Flash-Player-Integration bleiben unverändert.

## Build-Befehle

```powershell
# Portable ZIP (Tests + Release-Build + Bundle)
powershell -ExecutionPolicy Bypass -File scripts/package-windows.ps1

# Installer (nutzt vorhandenes ZIP oder baut es neu)
powershell -ExecutionPolicy Bypass -File scripts/package-installer.ps1 -UseExistingPayload
powershell -ExecutionPolicy Bypass -File scripts/package-installer.ps1            # mit Payload-Neubau

# Alles
powershell -ExecutionPolicy Bypass -File scripts/build-release.ps1 -Target windows -SkipTests

# Strukturprüfungen
python scripts/verify-packaging.py
python scripts/gen-third-party-notices.py   # Notices nach Dependency-Änderung neu generieren
```

## Testergebnisse (Windows, 2026-11-11)

| Prüfung | Ergebnis |
| --- | --- |
| `dotnet build` Installer-Projekt | 0 Warnungen, 0 Fehler |
| `dotnet publish` Single-File win-x64 | erfolgreich, ~75,7 MB |
| `Setup.exe --version` | `NØVA Flash Studio Setup 0.2.0` |
| `Setup.exe --smoke-test` | Payload-SHA256, 31 Einträge, 1 EXE, EULA vorhanden – OK |
| Headless-Installation (per-user, Temp-Dir) | Exit 0; Dateien, Manifest, HKCU-Uninstall-Key, `.nova`-Zuordnung registriert |
| Headless-Deinstallation | Exit 0; Installationsordner, Uninstall-Key, ProgId und `.nova`-Standardwert entfernt; `%APPDATA%` unberührt |
| Upgrade (Reinstall ins gleiche Verzeichnis) | Exit 0; alte Manifest-Dateien bereinigt; abgewählte Option entfernte Zuordnung |
| GUI-Start des Assistenten | Fenster `NØVA Flash Studio – Setup` geöffnet und sauber geschlossen |
| `python scripts/verify-packaging.py` | alle Checks bestanden (PNG/ICO/ICNS/Plist/AppStream/Skripte/Legal/Installer) |

## Generierte Artefakte

```text
dist/windows/
  NØVA Flash Studio-0.2.0-windows-x64-Setup.exe   ~75,7 MB  (NEU)
  NOVA-Flash-Studio-0.2.0-windows-x64.zip         ~7,4 MB   (mit legal/ + licenses/)
  NØVA Flash Studio/                              Portable Bundle (mit legal/ + licenses/)
  SHA256SUMS.txt                                  enthält EXE + ZIP + Setup-Hashes
```

## Bekannte Einschränkungen

- **Keine Codesignatur:** Weder App noch Installer sind Authenticode-signiert;
  SmartScreen wird voraussichtlich warnen. Ein Zertifikat ist erforderlich.
- **EULA ist ein Entwurf** (`legal/EULA-DRAFT.md`); die Lizenz
  `MIT OR Apache-2.0` stammt aus `Cargo.toml` und ist noch keine juristisch
  bestätigte Endentscheidung.
- **All-Users-Pfad wurde mit echter UAC-Elevation getestet** (Install,
  Shortcuts, HKLM-Registry, Deinstallation). Was bleibt ungetestet: die
  Klickstrecke im Assistenten bis zur Elevation (Codepfad identisch zur
  Headless-Variante) und das Verhalten bei UAC-Ablehnung im GUI (Fehlerbox
  ist implementiert, wurde nicht manuell geklickt).
- **Wizard nicht Klick-für-Klick getestet** — nur Start/Schließen verifiziert;
  die Seiten sind code-reviewiert und der Headless-Pfad (gleiche
  `InstallEngine`-Logik) wurde vollständig getestet.
- **Keine Silent-Install-Semantik wie Inno** (`/VERYSILENT` etc.); es gibt
  `--install ... --quiet` als dokumentierte Automatisierung.
- **Dateizuordnungen** decken nur `shell\open` ab (kein Open-With,
  keine Icons/Overlays); `.swf`-Zuordnung ist opt-in, weil NØVA SWF-Dateien
  nicht bearbeitet.
- **CI-Installer-Schritt nicht ausgeführt** — die Workflow-Änderung ist
  vorbereitet, aber das Repository hat keinen Remote-Lauf.
- Installer-UI-Sprache ist Deutsch (wie die App); CLI-Hilfe ist deutsch.

## Verbleibende Arbeit

- Rechtsprüfung EULA/Lizenz durch den Projektverantwortlichen; Platzhalter in
  `legal/EULA-DRAFT.md` (`[VERSION]`, `[PROJEKTVERANTWORTLICHER]`,
  `[KONTAKT …]`) füllen.
- Code-Signing-Zertifikat beschaffen und `signtool`-Schritt ergänzen
  (App + Setup).
- Klicktest der Elevations-Übergabe im Assistenten (UAC annehmen/ablehnen).
- Klicktest des Assistenten auf einer zweiten Maschine/VM.
- Optional: MSI via WiX als alternative Installationsform, wenn MSMI
  erforderlich wird; CURRENT_USER-Transforms sind bei MSIs unkritisch.
