# Security Notes (M29 additions)

## Code signing boundary

- Authenticode signing proves file integrity/origin; it does **not**
  guarantee safety, SmartScreen reputation or Defender acceptance.
- Only `signtool verify /pa`-verified signatures count as `SIGNED`.
  Self-signed certificates are technical tests only.
- Private keys never enter the repository, logs, installers, release
  archives or AI provider contexts. PFX passwords come from environment
  variables only and are never placed on command lines.
- `packaging/signing.json` is gitignored; only
  `packaging/signing.example.json` is committed.

## AI agent sandbox (M27) — unchanged

The agent build/test sandbox remains a **project copy** with
allowlist-only commands: no OS-level network/process/memory isolation.
See `docs/AI_AGENT_EDITOR.md` for the honest limits.

## Tool calling (M28) — unchanged

Model→project access goes exclusively through typed tools behind M25
grants and the M25 `ReviewSession`; destructive operations require
explicit per-file review acceptance.

## Installer

- UAC elevation only for all-users scope; per-user installs need none.
- No UserChoice/registry-hijack changes; uninstaller removes only its
  own files/keys/associations and preserves user data.
- No Defender exclusions, SmartScreen bypasses or whitelist
  recommendations are part of the installer.
