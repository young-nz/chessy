# Agent versus agent

Each player starts their own runner against their own conductor. The runners do not talk to each other. They watch Holochain and, on their own turn, commit one legal move.

## Stub versus stub

The stub provider does not call a model. It plays the first legal UCI move in the prompt, unless you pass a script. Two stubs can finish a game with no API key.

1. Follow [local-development.md](local-development.md) until two sandboxes are running and a game exists.
2. Copy the example config twice:

```bash
cp agent/chessy.agent.example.json /tmp/alice.agent.json
cp agent/chessy.agent.example.json /tmp/bob.agent.json
```

3. Edit each file. Set `appUrl` and `token` from that sandbox. Leave `provider` as `stub`. Do not put a key in the file.
4. Start both runners:

```bash
npm run start -w @chessy/agent -- /tmp/alice.agent.json
npm run start -w @chessy/agent -- /tmp/bob.agent.json
```

5. Open either UI, connect to that player's sandbox, and open the game. You are watching. With hand-off left off, the page will not move for you. The runners move when `my_turn` is true.

The same stub loop is what `agent/test/runner.test.ts` runs in memory: white plays `f2f3` and `g2g4`, black plays `e7e5` and `d8h4`, and the runner stops on checkmate without sending another move.

## A real model

Set `provider` to `openai`, `anthropic`, `xai`, `ollama`, or `openai-compatible`. Put the key in the environment or in the gitignored config, never in the profile you publish.

```bash
CHESSY_PROVIDER=openai OPENAI_API_KEY=sk-... CHESSY_MODEL=gpt-4o-mini \
  npm run start -w @chessy/agent -- /tmp/alice.agent.json
```

The prompt contains the FEN, the history, and the legal moves. The runner accepts a reply only when the parsed UCI is in that list. It retries up to 3 times. The log line `fallback: playing … after 3 rejected replies (first legal UCI move in lexicographic order)` means the fallback was used. See [keys.md](keys.md).

Ollama needs a local server, usually `http://127.0.0.1:11434`, and no key. An OpenAI-compatible server needs `baseUrl` (for example `http://127.0.0.1:8080/v1`) and `CHESSY_API_KEY`.

## Hand off from the UI

On a connected game, **Hand off to agent** runs the same chooser in the page on a timer. The key is read from the settings saved in this browser. Stop the agent before you move yourself, so the page and a CLI runner are not both committing.

Watching still works if you open the game and leave hand-off off. Signals update the board; a poll every few seconds covers a missed signal.
