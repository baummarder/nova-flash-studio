# EULA — Endbenutzer-Lizenzvereinbarung (M27.8)

## Dateien

| Datei | Sprache |
|---|---|
| `legal/LICENSE-EULA.md` | Deutsch (de-DE, Masterfassung) |
| `legal/LICENSE-EULA.en-GB.md` | Englisch (UK) |
| `legal/LICENSE-EULA.da-DK.md` | Dänisch |
| `legal/LICENSE-EULA.fr-FR.md` | Französisch |
| `legal/LICENSE-EULA.pl-PL.md` | Polnisch |

**EULA-Version:** 1.0 · **Stand:** 15. November 2026

## Status und rechtliche Hinweise

- Alle fünf Fassungen sind **inhaltlich gleichwertige Übersetzungen** der
  deutschen Masterfassung. Sie sind **Entwürfe** und wurden **nicht
  anwaltlich geprüft**.
- Vor einer öffentlichen Veröffentlichung müssen die Texte rechtlich
  geprüft und die Platzhalter ersetzt werden.
- **Platzhalter** (eckige Klammern) bewusst offen gelassen:
  `[RECHTEINHABER]`, `[JURISTISCHE ANSCHRIFT]`, `[KONTAKTADRESSE]`,
  `[DATENSCHUTZHINWEIS / URL]` (jeweils in der Sprache der Fassung).
  Es wurden keine Firma, Anschrift oder Kontaktadresse erfunden.
- Die EULA steht **neben** den Open-Source-Lizenzen (MIT/Apache-2.0 in
  `LICENSES/`), überschreibt diese nicht und stellt keine
  Drittkomponenten unter eine andere Lizenz. Bei Widersprüchen gehen die
  Open-Source-Lizenzen vor (EULA §7).
- `legal/EULA-DRAFT.md` bleibt als historischer Entwurf unverändert
  bestehen; die neuen `LICENSE-EULA.*.md` sind die verbindlichen Fassungen.

## Installer-Integration

- `packaging/windows/installer/NovaFlashStudio.Setup.csproj` bettet die
  fünf Dateien als `eula.<lang>.txt` ein.
- `Installer.ReadEulaText(lang)` wählt die Sprache des Installers,
  Fallback-Reihenfolge: gewählte Sprache → `en-GB` → `de-DE` → Legacy
  `EULA.txt`.
- Die Lizenzseite zeigt `EULA v1.0` und den vollständigen Text
  (scrollbar). **Weiter/Installieren ist deaktiviert, bis „Ich akzeptiere
  die Endbenutzer-Lizenzvereinbarung“ gewählt wurde** — nichts ist
  vorankreuzt.
- `InstallManifest.EulaVersion` und der Registry-Wert `EulaVersion`
  (Uninstall-Key) dokumentieren die akzeptierte Version. Eine neue
  EULA-Version erfordert eine neue Installation mit erneuter Zustimmung —
  eine bereits akzeptierte Version gilt nie stillschweigend als
  akzeptiert.
- Die Anwendung öffnet die EULA erneut über **Hilfe → Lizenzvereinbarung
  anzeigen (EULA)** (`menu.help.eula`, alle fünf Sprachen); die
  App sucht `LICENSE-EULA.<ui-sprache>.md` → `en-GB` → `EULA-DRAFT.md`
  im Programmverzeichnis.

## Wichtige Lizenzgrundsätze (umgesetzt)

Keine Lizenzschlüssel, keine Aktivierung, kein DRM, keine Pflicht-
Online-Verbindung, keine Geräte-/Seat-Limits; Nutzer behalten alle Rechte
an eigenen Inhalten; Editionen unterscheiden sich nur durch tatsächlichen
Funktionsumfang; Drittanbieter-Lizenzen bleiben unberührt; externe
KI-Dienste unterliegen eigenen Bedingungen; lokale KI-Nutzung bleibt
möglich (§10–§11).
