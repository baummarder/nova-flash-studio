# AGENTS.md — NØVA Flash Studio

## AI co-author attribution

AI agents that **actually contributed** to a change MUST be listed as git
co-authors via `Co-authored-by:` trailers at the END of the commit message.
No blanket or speculative co-author entries are allowed.

### Rules

- Only agents that genuinely worked on the diff may be credited.
- Multiple agents may co-author the same commit.
- Never rewrite history or amend past commit messages to add co-authors.
- Never change global git config; commit trailers only.
- Do not push, create releases, or alter GitHub settings/apps/workflows as
  part of attribution setup.

### Commit message format

```
<MILESTONE>: <short description>

<optional body>

Generated with [Devin](https://devin.ai)        <- only if Devin worked on it

Co-authored-by: Devin <158243242+devin-ai-integration[bot]@users.noreply.github.com>
Co-authored-by: <Agent Name> <<verified email>>
```

Trailers go last, one per line, standard git trailer format
(`Co-authored-by: Name <email>`).

### Verified identities

| Agent | GitHub account | Co-author email | Status |
|---|---|---|---|
| Devin (Cognition) | `devin-ai-integration[bot]` (id 158243242) | `158243242+devin-ai-integration[bot]@users.noreply.github.com` | verified — already used in repo history |
| GitHub Copilot SWE agent | `copilot-swe-agent` (id 203248971) | `203248971+copilot-swe-agent@users.noreply.github.com` | verified account exists; only credit if actually used |
| Princeton SWE-agent | `SWE-agent` org (id 166046056) | — | research project, not a commit bot; do NOT credit unless a real contributing identity is provided |
| DeepSeek | `deepseek-ai` (org, id n/a) | — | org account only; NO verified commit email — requires user-provided account before use |

### Unverified — do not use until confirmed

- Any `deepseek@users.noreply.github.com` style address: unverified, do not
  guess. If a DeepSeek agent contributes, the user must supply the exact
  GitHub account + email first.
- Princeton SWE-agent: if it contributes, use the identity the operator
  actually used (e.g. the human's account or a dedicated bot), not the org.

### Before committing

1. Ask: did this agent write or modify code in this change?
2. If yes, append its trailer with the verified email above.
3. If unsure whether an identity is valid, omit the trailer and ask.
