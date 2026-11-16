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

### OpenCode / DeepSeek rule

OpenCode MUST check before every commit which AI agents actually worked on
the diff — including whether OpenCode itself or the DeepSeek model it used
contributed to the change.

- If OpenCode/DeepSeek created or edited changes, that participation MUST be
  reflected — but NOT via an invented identity.
- As long as no verified DeepSeek co-author identity exists:
  - do NOT list DeepSeek as a git co-author;
  - mention the participation in the commit body or documentation instead,
    where sensible (e.g. `Note: changes drafted with OpenCode/DeepSeek`);
  - inform the user that a verified GitHub identity is missing.
- Once the user provides a real, verified GitHub identity and matching
  commit email, OpenCode MAY credit DeepSeek as co-author — but only for
  changes it actually contributed to.
- This rule applies to all future milestones and subtasks.

### Unverified — do not use until confirmed

- Any `deepseek@users.noreply.github.com` style address: unverified, do not
  guess. If a DeepSeek agent contributes, the user must supply the exact
  GitHub account + email first (see OpenCode / DeepSeek rule above).
- Princeton SWE-agent: if it contributes, use the identity the operator
  actually used (e.g. the human's account or a dedicated bot), not the org.

### Before committing

1. Ask: did this agent write or modify code in this change?
2. If yes and a verified identity exists, append its trailer.
3. If the agent contributed but has no verified identity (e.g. DeepSeek),
   mention the participation in the commit body and tell the user a verified
   GitHub identity is missing — do NOT invent one.
4. If unsure whether an identity is valid, omit the trailer and ask.

## M27 notes

- Agent build/test commands MUST run inside the per-task `Sandbox`
  (`nova-ai/src/sandbox.rs`), never in the original project. The M25
  `ReviewSession` stays the only apply path.
- The sandbox is a project copy, not an OS sandbox — no network/process
  isolation claims without real implementation.
- EULA files live in `legal/LICENSE-EULA*`; installer embeds them per
  language. Never overwrite `legal/EULA-DRAFT.md` or `LICENSES/`.
- Agent tools (`nova-ai/src/tools.rs`) are the only model→project path:
  read tools via `GuardedWorkspace`, writes via `ReviewSession`, editor
  ops via `ProjectOperation`. Never add a tool that writes originals or
  executes arbitrary code; destructive ops must stay `Destructive`.
- Edit Rust files only via the file tools — PowerShell round-trips have
  corrupted UTF-8 before (see encoding regression test).

## Local commit permissions

Devin MAY create local git commits autonomously once a task is fully
implemented.

### Requirements before committing

- Relevant tests and builds MUST have been run first.
- `git status` and `git diff` MUST be reviewed before every commit.
- Commit ONLY changes belonging to the current task; never sweep up
  unrelated or pre-existing modifications.
- Use meaningful commit messages.
- If Devin actually contributed, use its verified co-author identity
  (see Verified identities).
- All existing rules for OpenCode, DeepSeek and other agents still apply.

### Boundaries

- Local commits are allowed.
- `git push` still requires explicit user approval.
- No releases or publishing without approval.
- No history rewrites or force pushes.
- On unexpected changes or unclear attribution, STOP and ask the user.
