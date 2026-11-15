# NØVA Flash Studio Edu — Klassenzimmer (M20)

Die Education Edition enthält einen vollständigen, **offline-fähigen**
Klassenzimmer-Modus. Alles läuft ohne Konto, Server, Cloud oder
Lizenzschlüssel; Workspaces können auf USB-Sticks, Netzlaufwerken oder
lokal liegen.

> **Rollen sind keine Authentifizierung.** Lehrkraft/Assistenz/Lernende
> sind lokale *Arbeitsweisen* in `edu-profile.json`. Jede Person mit
> Dateizugriff auf den Workspace kann alle Daten lesen und ändern —
> das ist im UI und hier dokumentiert.

## Workspace

Ein Edu-Workspace ist ein Verzeichnis mit `school.json` und diesen
Ordnern (alle via `nova-edu::Workspace`, schema versioned):

```
<workspace>/
  school.json          # Schule, Policy (Cloud-KI aus!), Retention-Hinweis
  meta.json            # Workspace-Metadaten, Letzte-Öffnen-Liste
  classrooms/*.json    # Klasse, Gruppen, Mitglieder (UUIDs)
  assignments/*.json   # Aufgaben, Rubrik, Starter-Referenz, Ziele
  inbox/<class>/<asg>/<sub-id>/
      sub-<id>.json    # Submissions-Metadaten (Revision, Checksums)
      files/project/*.nova, files/exports/*
  feedback/<class>/<asg>__<member>.json
  reports/             # exportierte CSV/MD/HTML-Berichte
  templates/           # catalog.json + Starter-.nova + Tutorials
```

`school.json` enthält `SchoolPolicy`: `allowCloudAi` ist **standardmäßig
aus**, `allowSwfExport` an, `retentionDays` optionaler Hinweis (nichts
wird automatisch gelöscht). Unbekannte Felder bleiben erhalten
(forward-compatibel).

## Rollen & Profile

`LocalProfile` (UUID, Anzeigename, optionales Pseudonym,
`EduRole::{Teacher, Student, Assistant}`, verknüpfte `member_id`)
liegt in der **Edition-Config** (`%APPDATA%\NOVA Flash Studio Edu\`),
nie im Workspace und nie im Projekt. Ein Lernender verknüpft sich im
Tab „Klassen“ einmalig mit seinem Listen-Eintrag.

## Workflows

### Lehrkraft
1. Workspace öffnen/erstellen → Beispieldaten optional.
2. „Klassen“: Klasse anlegen, Gruppen, Mitglieder (oder per JSON-Liste
   importieren). Teilnehmerliste exportierbar.
3. „Aufgaben“: Titel, Markdown-Anweisungen, optionaler Starter
   (Workspace-Pfad), Fälligkeitsdatum, Spät-Policy → speichert
   `.novaassignment`-Paket zum Verteilen (z. B. auf USB).
4. „Abgaben“: eingegangene Submissions sichten — „Ansehen“ öffnet die
   verpackte `.nova` read-only im normalen Editor; Status auf
   „Gesehen“/„Bewertet“ setzen.
5. „Feedback“: Kommentare (Lob/Problem/Frage/Anforderung), Zusammenfassung
   und Rubrik-Marken → `.novafeedback` exportieren.
6. „Fortschritt“: Statusmatrix pro Aufgabe (Nicht begonnen … Bewertet),
   Verspätungen, offene Kommentare → CSV/Markdown/HTML-Export.

### Lernende/r
1. „Klassen“: eigene Klasse ansehen, Mitgliedseintrag verknüpfen.
2. „Aufgaben“: Anweisungen lesen, Starterprojekt öffnen (lädt die
   `.nova` in den normalen Editor).
3. „Aktuelles Projekt als Abgabe exportieren“: speichert die Abgabe im
   Workspace-Inbox (Revision +1, nie überschreibend) **und** schreibt
   eine `.novasubmission`-Datei zum Weitergeben (z. B. an die
   Lehrkraft per USB).
4. „Feedback“: Kommentare lesen, „Gelesen“ bestätigen.

## Paketformate (offline, versioniert)

Alle Pakete sind ZIP mit `manifest.json` (`schema`, `format`, `id`,
`createdBy`, `content`, `checksums`), Payload und
`checksums.sha256`-äquivalent im Manifest (`sha256:<hex>` pro Datei).
`PACKAGE_SCHEMA = 1`; unbekannte Schema-Major-Versionen werden ohne
Teil-Import abgelehnt.

| Extension | Inhalt | Import |
|---|---|---|
| `.novaassignment` | `assignment.json`, `starter/*.nova`, Tutorial | neue Aufgabe + Starter in `templates/` |
| `.novasubmission` | `submission.json`, `project/*.nova`, `exports/*` | neue Submissions-Revision im Inbox |
| `.novafeedback` | `feedback.json` (Kommentare, Rubrik) | Feedback-Bundle speichern |
| `.novaclassroom` | `classroom.json` (Klasse, Gruppen, Mitglieder) | Klasse mit frischen UUIDs anlegen |

**Sicherheit beim Lesen** (`nova-edu::package`): nur relative Pfade,
keine `..`/ absoluten/Laufwerks-/UNC-Namen, keine Symlinks,
Verschlüsselung abgelehnt, Limits (10 000 Einträge, 1 GiB unkomprimiert,
Kompressionsverhältnis ≤ 100), Checksummen **vor** jedem Schreiben,
Extraktion in ein Temp-Verzeichnis, dann erst commit. Paket-Inhalte
werden nie ausgeführt.

## Feedback-Anker

`ProjectAnchor` referenziert `object_id` → `layer_id`+`frame` →
`scene_id`/`label`. Beim Öffnen der Abgabe kann die UI Anker gegen das
geladene `Project` auflösen; nicht mehr auflösbare Anker werden als
`orphaned` markiert — Kommentare bleiben erhalten und sichtbar
gekennzeichnet, werden nicht stillschweigend gelöscht oder umgehängt.

## Fortschritt

`ProgressStatus` wird **nur** aus aufgezeichneten Ereignissen abgeleitet
(Submissions, Feedback-Kommentare, Rubrik-Marken) — keine Tastenanschläge,
keine Zeiterfassung, keine Telemetrie. Berichte: CSV (RFC-4180-Escaping),
Markdown, statisches HTML ohne externe Assets.

## Dateizuordnung

Windows-Installer registrieren `.nova` pro Edition (eigene ProgIds),
siehe `docs/WINDOWS_FILE_ASSOCIATION.md`. Edu-Projekte sind normale
`.nova`-Dateien — die Zuordnung öffnet die jeweilige Edition; das
Klassenzimmer ist ein Modus darin.
