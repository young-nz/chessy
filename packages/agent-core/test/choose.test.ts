import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  FALLBACK_RULE,
  MAX_ATTEMPTS,
  buildPrompt,
  chooseMove,
  createProvider,
  parseMove,
  redact,
  type MoveProvider,
} from "../src/index.ts";

const context = {
  fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
  history: [] as string[],
  legalMoves: ["e2e4", "a2a3", "g1f3"],
  sideToMove: "white",
};

function scripted(replies: string[]): MoveProvider & { prompts: string[] } {
  const prompts: string[] = [];
  let i = 0;
  return {
    id: "test",
    prompts,
    async complete(prompt: string) {
      prompts.push(prompt);
      const reply = replies[i] ?? "";
      i += 1;
      return reply;
    },
  };
}

describe("prompt and parse", () => {
  it("includes FEN, history, and legal moves", () => {
    const prompt = buildPrompt({ ...context, history: ["e2e4", "e7e5"] });
    assert.match(prompt, /FEN: rnbqkbnr/);
    assert.match(prompt, /History: e2e4 e7e5/);
    assert.match(prompt, /Legal moves: a2a3, e2e4, g1f3/);
  });

  it("accepts a legal token and rejects prose that is not a move", () => {
    assert.equal(parseMove("I play e2e4.", context.legalMoves), "e2e4");
    assert.equal(parseMove("e2e5", context.legalMoves), null);
    assert.equal(parseMove("castle kingside", context.legalMoves), null);
  });
});

describe("retries and fallback", () => {
  it("commits the first legal reply", async () => {
    const logs: string[] = [];
    const provider = scripted(["e2e4"]);
    const chosen = await chooseMove({
      ...context,
      provider,
      log: (line) => logs.push(line),
    });
    assert.equal(chosen.uci, "e2e4");
    assert.equal(chosen.usedFallback, false);
    assert.equal(chosen.attempts, 1);
    assert.equal(provider.prompts.length, 1);
  });

  it("retries an illegal reply and then accepts a legal one", async () => {
    const provider = scripted(["e2e5", "not a move", "g1f3"]);
    const chosen = await chooseMove({
      ...context,
      provider,
      log: () => {},
    });
    assert.equal(chosen.uci, "g1f3");
    assert.equal(chosen.attempts, 3);
    assert.match(provider.prompts[1], /previous reply was rejected/i);
    assert.match(provider.prompts[2], /FEN:/);
  });

  it("uses the documented fallback after the retry limit", async () => {
    const logs: string[] = [];
    const chosen = await chooseMove({
      ...context,
      provider: scripted(["nope", "still nope", "zzzz"]),
      log: (line) => logs.push(line),
    });
    assert.equal(MAX_ATTEMPTS, 3);
    assert.equal(chosen.usedFallback, true);
    assert.equal(chosen.uci, "a2a3");
    assert.match(logs.at(-1) ?? "", /fallback: playing a2a3/);
    assert.match(logs.at(-1) ?? "", new RegExp(FALLBACK_RULE));
  });

  it("retries when the provider throws and does not leak the key", async () => {
    const secret = "sk-test-secret-value";
    const logs: string[] = [];
    let calls = 0;
    const provider: MoveProvider = {
      id: "openai",
      async complete() {
        calls += 1;
        throw new Error(`unauthorized ${secret}`);
      },
    };
    const chosen = await chooseMove({
      ...context,
      provider,
      secrets: [secret],
      log: (line) => logs.push(line),
    });
    assert.equal(calls, MAX_ATTEMPTS);
    assert.equal(chosen.usedFallback, true);
    assert.equal(logs.some((line) => line.includes(secret)), false);
    assert.match(logs[0], /\[redacted\]/);
  });
});

describe("providers", () => {
  it("fails clearly when the key is missing and does not call the network", async () => {
    let called = false;
    const fetchImpl: typeof fetch = async () => {
      called = true;
      throw new Error("should not be called");
    };
    assert.throws(
      () => createProvider({ provider: "openai", apiKey: "" }, fetchImpl),
      /Missing API key for openai/,
    );
    assert.equal(called, false);
  });

  it("posts to a custom OpenAI-compatible base URL and redacts the key", async () => {
    const secret = "sk-live-example";
    let seenUrl = "";
    let seenAuth = "";
    const fetchImpl: typeof fetch = async (url, init) => {
      seenUrl = String(url);
      seenAuth = String(new Headers(init?.headers).get("authorization"));
      return new Response(JSON.stringify({ error: `bad key ${secret}` }), { status: 401 });
    };
    const provider = createProvider(
      {
        provider: "openai-compatible",
        baseUrl: "http://127.0.0.1:8080/v1",
        model: "local-model",
        apiKey: secret,
      },
      fetchImpl,
    );
    await assert.rejects(
      () => provider.complete("prompt"),
      (error: Error) => {
        assert.match(error.message, /openai-compatible request failed \(401\)/);
        assert.equal(error.message.includes(secret), false);
        assert.match(error.message, /\[redacted\]/);
        return true;
      },
    );
    assert.equal(seenUrl, "http://127.0.0.1:8080/v1/chat/completions");
    assert.equal(seenAuth, `Bearer ${secret}`);
  });

  it("speaks Anthropic, xAI, and Ollama request shapes", async () => {
    const hits: string[] = [];
    const fetchImpl: typeof fetch = async (url, init) => {
      hits.push(`${url} ${init?.body}`);
      const target = String(url);
      if (target.includes("anthropic")) {
        return new Response(JSON.stringify({ content: [{ text: "e2e4" }] }));
      }
      if (target.includes("11434")) {
        return new Response(JSON.stringify({ message: { content: "a2a3" } }));
      }
      return new Response(JSON.stringify({ choices: [{ message: { content: "g1f3" } }] }));
    };
    const anthropic = createProvider(
      { provider: "anthropic", apiKey: "ant-key", model: "claude-test" },
      fetchImpl,
    );
    assert.equal(await anthropic.complete("fen"), "e2e4");
    const xai = createProvider({ provider: "xai", apiKey: "xai-key", model: "grok-test" }, fetchImpl);
    assert.equal(await xai.complete("fen"), "g1f3");
    const ollama = createProvider({ provider: "ollama", model: "llama-test" }, fetchImpl);
    assert.equal(await ollama.complete("fen"), "a2a3");
    assert.match(hits[0], /api.anthropic.com\/v1\/messages/);
    assert.match(hits[0], /claude-test/);
    assert.match(hits[1], /api.x.ai\/v1\/chat\/completions/);
    assert.match(hits[2], /127.0.0.1:11434\/api\/chat/);
  });

  it("redacts secrets that appear in thrown network errors", () => {
    assert.equal(redact("socket hang up sk-abc", ["sk-abc"]), "socket hang up [redacted]");
  });
});
