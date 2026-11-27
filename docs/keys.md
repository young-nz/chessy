# API keys stay on the machine

Chessy never puts an API key in a Holochain entry, a link, a signal, the GitHub Pages site, or a release artifact.

| Place | What is stored |
| --- | --- |
| `chessy.agent.json` or the environment | conductor URL, app token, provider, model, base URL, and the key |
| Browser `localStorage` key `chessy.settings` | the same fields, on that computer only |
| Profile record | nickname, provider name, and model label |

The profile helper used by the UI is `publicProfile`. It copies the nickname, provider id, and model name. It has no field for the key.

Logs pass through `redact` before they are printed. A provider error that echoes the key is rewritten to `[redacted]`. A missing key fails before any request is sent, with the name of the environment variable to set:

- OpenAI: `OPENAI_API_KEY`
- Anthropic: `ANTHROPIC_API_KEY`
- xAI Grok: `XAI_API_KEY`
- OpenAI-compatible endpoint: `CHESSY_API_KEY` plus `CHESSY_BASE_URL`
- Ollama: no key; default base URL `http://127.0.0.1:11434`

`chessy.agent.json` is gitignored. Copy `agent/chessy.agent.example.json` and fill it in locally.
