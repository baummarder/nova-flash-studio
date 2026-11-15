# NØVA Flash Studio Enterprise — Teamwork, Administration & Governance (M21)

Die Enterprise Edition enthält einen vollständigen, **offline-fähigen**
Administrations- und Kollaborations-Workspace. Alles läuft ohne Konto,
Server, Cloud oder Lizenzschlüssel.

> **Rollen sind keine Authentifizierung.** Owner/Admin/PM/Developer/
> Designer/Reviewer/Viewer sind lokale *Workflow-Labels*. Die
> Berechtigungsmatrix entscheidet, was die UI anbietet und was die
> Domain-API akzeptiert — die echte Zugriffskontrolle bleibt die
> **Dateisystem-ACL des Betriebssystems**. Es gibt keinen Pseudo-Login.

## Workspace

Ein Enterprise-Workspace ist ein Verzeichnis mit `org.json` (Marker +
Organisationsmetadaten) und diesen Ordnern:

```
<workspace>/
  org.json            # Organisation, Schema-Version
  team.json           # Mitglieder, Org-Rollen, Projekt-Rollen
  meta.json           # UI-Präferenzen (letztes Projekt, Tab)
  projects/<uuid>.json   # Projekt-Registry: absoluter .nova-Pfad + sha256
  assets/index.json + assets/blobs/<sha256>.bin
  versions/<project-id>/snap-*.nova + index.json
  reviews/<uuid>.json
  builds/profiles.json + builds/history.jsonl
  audit/audit.jsonl   # append-only Audit-Log
  integrations/config.json
  exports/            # Build-Artefakte
```

**Projekte werden nur registriert, nie verschoben oder verändert.** Die
Registry speichert den absoluten Pfad der `.nova`-Datei plus einen
sha256-Baseline; Abweichungen werden als „geändert seit Registrierung“
angezeigt.

## Module (`nova-ent`)

| Modul | Inhalt |
|---|---|
| `workspace`/`org` | Workspace-Layout, Organisationsmetadaten, Präferenzen |
| `profile`/`team` | `EntProfile`, 7 Rollen, `Action`-Matrix, Mitglieder, Projekt-Rollen |
| `project registry` | `.nova`-Registrierung per Pfad, Archivierung,Checksum-Status |
| `assets` | Bibliothek: Ordner, Tags, Versionen, SHA-256, Suche, Export |
| `versions` | Byte-Snapshots, Elternkette, Diff-Summary (Szenen/Assets/Layer/Objekte), Safety-Snapshot vor Restore |
| `review` | Draft→Pending→InReview→Approved/Changes/Rejected, Kommentare mit Ankern, Verwaist-Markierung |
| `admin`/`audit` | Übersicht + append-only JSONL-Audit-Log |
| `builds` | Build-Profile, SWF/XFL über `nova-build`/`nova-formats`, Historie mit sha256 |
| `integrations` | lokale Endpunkte (Ollama/OpenAI-kompatibel), standardmäßig aus, Key nur als Env-Var-Name |
| `package` | `.novaent`-Export/Import (gehärtetes ZIP, Checksummen, Whitelist-Commit) |

## Rollen & Berechtigungen

`EnterpriseRole`: Owner, Admin, ProjectManager, Developer, Designer,
Reviewer, Viewer. `default_permissions(role)` liefert die
`Action`-Menge (`ReadProjects`, `EditProjects`, `ManageVersions`,
`ManageAssets`, `ManageReviews`, `ManageBuilds`, `ManageTeam`,
`ManageOrg`, `ViewAdmin`). `team.json` kann pro Projekt eine abweichende
Rolle speichern (`project_roles`).

**Dies ist Policy, kein Zugriffsschutz** — dokumentiert in UI und Code.

## Versionierung

- `create_snapshot`: voller Byte-Copy unter `versions/<pid>/` mit
  Metadaten (Autor-Label, Nachricht, Eltern-Snapshot, sha256,
  ChangeSummary vs. Elternstand).
- `restore_version`: verweigert bei IntegritätsMismatch, erstellt
  zuerst einen **Safety-Snapshot** des aktuellen Stands, validiert den
  Snapshot als `.nova`, schreibt per temp+rename zurück.
- `compare_with_current`: Diff-Richtung Snapshot→aktuell.
- Git-Repositories werden nicht angefasst; es gibt keine Git-Integration.

## Reviews

Statusautomat mit History (`ReviewEvent` pro Übergang), Pflicht-Begründung
bei Ablehnung/Rückgabe. Kommentare tragen optional `ReviewAnchor`
(Object > LayerFrame > SceneLabel > Scene); `anchor_resolves` prüft gegen
das geladene `Project`, unverankerte/aufgelöste Anker werden `orphaned`
markiert — nie gelöscht oder umgehängt.

## Builds

`BuildProfile` (Name, Ziele SWF/XFL, compress, notes) → `run_build`
nutzt `nova_build::build_swf` bzw. `nova_formats::export_xfl`, schreibt
Artefakte nach `exports/` und hängt `BuildRecord` (sha256, Größe,
Warnungen, Dauer, Akteur) an `history.jsonl` an.
`verify_build_record` prüft Artefakt-Integrität.

## Integrationen

`Integration` speichert Endpoint-URL, Modellname, `api_key_env`
(Nur-Name der Umgebungsvariable) und `enabled=false`-Default.
`validate()` verlangt `http(s)://` ohne Credentials in der URL, wenn
ein Endpoint aktiv ist; `resolve_api_key` liest die Env-Variable zur
Laufzeit. Keine Netzwerkaufrufe im Domain-Layer.

## `.novaent`-Pakete

Export: `collect_workspace_files` (org ausgenommen → Import setzt die
lokale Organisation nicht zurück), `build_package` mit
`manifest.json` + sha256 pro Datei. Import: `read_package_bytes`
verifiziert Checksummen **vor** jeglichem Schreiben, lehnt Traversal,
absolute Pfade, Symlinks, Verschlüsselung, >10k Einträge, >1 GiB und
Kompressionsverhältnis >100 ab; `commit_files` akzeptiert nur eine
Whitelist (`team.json`, `projects/`, `reviews/`, `builds/profiles.json`,
`integrations/config.json`, `assets/index.json`, `blobs/` →
`assets/blobs/`) und **überschreibt nie** bestehende Dateien.

## Edition-Gating

`Mode::Enterprise` existiert nur, wenn
`edition::has_active_module("administration")` — also nur in der
Enterprise-Edition. Toolbar, `SetMode`-Guard und Central-Dispatch sind
alle gegated; User und Edu sehen den Modus nicht.
