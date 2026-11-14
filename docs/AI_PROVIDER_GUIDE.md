# AI provider guide

NØVA Flash Studio can use a local or cloud language model for the integrated
**AI Agent Editor**. Providers are configured independently from projects, and
API keys are never written to project files or provider JSON.

## Supported providers

| Kind | Default endpoint | Model discovery | Credential required | Classification |
| --- | --- | --- | --- | --- |
| Ollama (local) | `http://localhost:11434` | `GET /api/tags` | No | Local only; remote URLs are rejected |
| Ollama Cloud | `https://ollama.com` | `GET /api/tags` | Yes | Cloud |
| Groq | `https://api.groq.com/openai/v1` | `GET /models` | Yes | Cloud |
| OpenAI-compatible | configurable, e.g. `http://localhost:8000/v1` | `GET /models` | Depends on endpoint | Local for localhost/private names; cloud otherwise |

The OpenAI-compatible kind covers servers that implement the common
`/v1/models` and `/v1/chat/completions` shape. The exact endpoint is editable.
If a provider does not expose model discovery, configure the model ID manually
and use a provider whose endpoint is known to support the expected API.

## Setup

1. Open **KI → KI-Anbieter …** (or the **KI-Anbieter** button in the AI Agent
   Editor).
2. Choose one of the four provider kinds.
3. Set the display name, base URL and model. **Modelle laden** queries the
   provider; you can also type a model ID.
4. Enter the API key for cloud providers. The field is write-only: an existing
   key is shown only as **gespeichert**, never rendered back.
5. Press **Speichern**. The provider is enabled by default.
6. Press **Verbindung testen**. A successful test means the models endpoint was
   reachable and authenticated; it is not a guarantee that a particular model
   can answer a particular prompt.
7. Choose **Als Standard** for the default provider. A per-session override can
   be selected in the AI Agent Editor header without changing the default.

Multiple providers can be enabled. The AI Agent Editor lists only enabled
providers. Disabling a provider does not delete its credential. Removing a
provider deletes its stored credential after confirmation.

## Configuration files

Provider settings live in the per-user configuration directory:

- Windows: `%APPDATA%\NOVA Flash Studio\ai-providers.json`
- macOS: `~/Library/Application Support/NOVA Flash Studio/ai-providers.json`
- Linux: `$XDG_CONFIG_HOME/nova-flash-studio/ai-providers.json` (or
  `~/.config/nova-flash-studio/ai-providers.json`)

`NOVA_FLASH_STUDIO_CONFIG_DIR` overrides the directory for development and
tests.

The JSON contains provider metadata only:

```json
{
  "version": 1,
  "defaultProvider": "ollama-local-…",
  "providers": [
    {
      "id": "ollama-local-…",
      "name": "Ollama (lokal)",
      "kind": "ollama_local",
      "baseUrl": "http://localhost:11434",
      "model": "llama3.2",
      "enabled": true
    }
  ]
}
```

There is no API-key field. Do not add one. Keys are stored separately in the
`credentials` directory.

## Credential storage

On Windows the default backend is **DPAPI** (`CryptProtectData`), scoped to the
current Windows user and bound to a NØVA-specific entropy value. The encrypted
blob is written under the per-user configuration directory. Another Windows
user, or a copied blob on another machine, cannot decrypt it through the normal
DPAPI path.

The settings dialog displays the active backend. The fallback backend used on
non-Windows development builds is a restricted file; it is named explicitly as
not encrypted. It is not presented as equivalent to DPAPI.

Keys are never:

- written to `ai-providers.json`;
- included in `nova ai-status` output (only `ja`/`nein` is printed);
- included in logs, activity entries or error messages;
- placed in URLs or query strings.

The HTTP layer sends the key in the `Authorization: Bearer …` header. Error
messages from providers are sanitized and truncated before display.

## Privacy model

NØVA classifies an endpoint as **local** when its host is `localhost`, a
loopback address, or a single-label host name; otherwise it is **cloud**. The
AI Agent Editor shows the classification next to the provider.

- **Local:** the request goes to the configured endpoint. The app still shows
  exactly which files and text are in the context.
- **Cloud:** the request is blocked until **Cloud-Zustimmung** is checked in the
  AI Agent Editor. The consent is per session and is not persisted as a blanket
  permission.
- The user chooses the context files. The editor never uploads the entire
  project by default. Source text is included only for scripts explicitly
  checked in the context panel.
- Preserved ABC is described as bytecode. Its bytes are never sent as source.
- The agent does not receive shell access, arbitrary process access, or paths
  outside the configured project root.

## Model discovery and connection tests

- **Ollama:** `GET <base>/api/tags`; accepts `name` or `model` fields.
- **Groq / OpenAI-compatible:** `GET <base>/models`; reads `data[].id`.
- A `401`/`403` response is reported as rejected credentials, without echoing
  the key.
- A timeout is reported as a timeout, not as a model error.
- A provider that is disabled cannot be tested or used.
- The default tests in `crates/nova-ai/src/provider.rs` use a mock transport and
  make no live network requests. Opt-in live Ollama tests live in
  `crates/nova-ai/tests/live_ollama.rs` and are ignored unless explicitly run.

## Milestone 7.3 live validation

| Check | Result |
| --- | --- |
| Local Ollama `0.40.16` at `http://localhost:11434` | **PASS** — 32 models discovered |
| `qwen2.5-coder:7b` chat | **PASS** |
| Agent request/parsing | **PASS** — 2,009 context characters, `cloud=false` |
| Unavailable endpoint | **PASS** — clear network error, no fabricated answer |
| Ollama Cloud / Groq / custom cloud | **NOT AVAILABLE** — no credential or endpoint configured; no request attempted |
| DPAPI plaintext check | **PASS** — encrypted blob does not contain the secret |
| Provider JSON secret check | **PASS** — no key or key field is serialized |
| Error redaction | **PASS** — keys in provider error text are replaced with `[redacted]` |
| Interactive key-entry GUI | **PARTIAL** — write-only UI and tests exist; no full interactive session was repeated |

Run the live tests with:

```powershell
cargo test -p nova-ai --test live_ollama -- --ignored --nocapture --test-threads=1
```

No real cloud API key was used or printed. The live tests are deliberately
separate from the default offline suite.

## CLI diagnostics

`nova ai-status` prints provider names, kinds, endpoints, model IDs, default
status, local/cloud classification, credential backend and whether a credential
is present. It never prints the credential value.

```powershell
cargo run -p nova-cli -- ai-status
```

## Troubleshooting

| Symptom | Likely cause | Action |
| --- | --- | --- |
| `Kein KI-Anbieter konfiguriert` | No enabled provider or no default | Add a provider and set a default |
| `benötigt einen API-Schlüssel` | Cloud provider without a stored key | Enter the key and save |
| `Anmeldedaten abgelehnt (HTTP 401)` | Wrong/expired key or provider account issue | Replace the key; do not paste it into logs |
| `Zeitüberschreitung` | Endpoint unreachable or slow | Check URL, network, local Ollama service |
| `Modell … ist nicht ausgewählt` | Provider saved without a model | Load models or type the exact model ID |
| Cloud request blocked | Consent not checked | Check **Cloud-Zustimmung** only when you intend to send the selected context |
| Ollama local rejected remote URL | Ollama-local kind is local-only | Use Ollama Cloud or OpenAI-compatible for remote endpoints |

## Deliberate limits

- No streaming responses in this milestone.
- No automatic retry loop and no background token accounting.
- No image/audio input.
- No provider-side tool execution. The agent can request a local build, file
  diffs and typed project operations; the provider never runs them. File changes
  require a confirmed diff and project operations require a confirmed preview.
- M8 adds structured project operations. Their tests use mocked HTTP and no live
  provider; the local-Ollama results remain M7.3 evidence. Cloud providers were
  not configured or exercised in M8.
- Provider terms, data retention and model quality are outside NØVA's control.
  Choose a provider appropriate for the project data involved.
