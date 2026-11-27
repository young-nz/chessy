import { redact } from "./redact.ts";

export type FetchLike = (url: string, init?: RequestInit) => Promise<Response>;

export class MissingKeyError extends Error {
  constructor(provider: string, envName: string) {
    super(
      `Missing API key for ${provider}. Set ${envName} or apiKey in the local config file. The key stays on this machine and is never written to Holochain.`,
    );
    this.name = "MissingKeyError";
  }
}

export interface MoveProvider {
  readonly id: string;
  complete(prompt: string): Promise<string>;
}

export interface ProviderConfig {
  provider: string;
  model?: string;
  baseUrl?: string;
  apiKey?: string;
  /** Scripted replies for tests and the stub chooser. */
  script?: string[];
}

const DEFAULTS: Record<string, { baseUrl: string; env: string; model: string }> = {
  openai: { baseUrl: "https://api.openai.com/v1", env: "OPENAI_API_KEY", model: "gpt-4o-mini" },
  anthropic: { baseUrl: "https://api.anthropic.com/v1", env: "ANTHROPIC_API_KEY", model: "claude-3-5-haiku-latest" },
  xai: { baseUrl: "https://api.x.ai/v1", env: "XAI_API_KEY", model: "grok-2-latest" },
  ollama: { baseUrl: "http://127.0.0.1:11434", env: "", model: "llama3.2" },
  "openai-compatible": { baseUrl: "", env: "CHESSY_API_KEY", model: "local" },
  stub: { baseUrl: "", env: "", model: "stub" },
};

export function providerDefaults(id: string): { baseUrl: string; env: string; model: string } {
  return DEFAULTS[id] ?? DEFAULTS["openai-compatible"];
}

export function createProvider(
  config: ProviderConfig,
  fetchImpl: FetchLike = fetch,
): MoveProvider {
  const id = config.provider || "stub";
  if (id === "stub") return stubProvider(config.script ?? []);
  if (id === "ollama") return ollamaProvider(config, fetchImpl);
  if (id === "anthropic") return anthropicProvider(config, fetchImpl);
  if (id === "openai" || id === "xai" || id === "openai-compatible") {
    return openAiCompatibleProvider(id, config, fetchImpl);
  }
  throw new Error(
    `Unknown provider "${id}". Use stub, openai, anthropic, xai, ollama, or openai-compatible.`,
  );
}

export function stubProvider(script: string[] = []): MoveProvider {
  let cursor = 0;
  return {
    id: "stub",
    async complete(prompt: string) {
      if (cursor < script.length) {
        const reply = script[cursor];
        cursor += 1;
        return reply ?? "";
      }
      const match = prompt.match(/Legal moves: (.+)/);
      const first = match?.[1]?.split(",")[0]?.trim();
      if (!first) throw new Error("stub provider could not find the legal move list in the prompt");
      return first;
    },
  };
}

function requireKey(id: string, apiKey: string | undefined): string {
  const envName = providerDefaults(id).env || "CHESSY_API_KEY";
  if (!apiKey || apiKey.trim().length === 0) throw new MissingKeyError(id, envName);
  return apiKey.trim();
}

function openAiCompatibleProvider(
  id: string,
  config: ProviderConfig,
  fetchImpl: FetchLike,
): MoveProvider {
  const defaults = providerDefaults(id);
  const apiKey = requireKey(id, config.apiKey);
  const base = (config.baseUrl || defaults.baseUrl).replace(/\/$/, "");
  if (!base) {
    throw new Error(`${id} needs a base URL. Set baseUrl in the local config.`);
  }
  const model = config.model || defaults.model;
  return {
    id,
    complete: (prompt) =>
      postJson(
        id,
        `${base}/chat/completions`,
        {
          Authorization: `Bearer ${apiKey}`,
          "Content-Type": "application/json",
        },
        { model, temperature: 0, messages: [{ role: "user", content: prompt }] },
        apiKey,
        fetchImpl,
        (body) => {
          const text = body?.choices?.[0]?.message?.content;
          if (typeof text !== "string" || text.length === 0) {
            throw new Error(`${id} returned an empty completion`);
          }
          return text;
        },
      ),
  };
}

function anthropicProvider(config: ProviderConfig, fetchImpl: FetchLike): MoveProvider {
  const apiKey = requireKey("anthropic", config.apiKey);
  const model = config.model || providerDefaults("anthropic").model;
  const base = (config.baseUrl || providerDefaults("anthropic").baseUrl).replace(/\/$/, "");
  return {
    id: "anthropic",
    complete: (prompt) =>
      postJson(
        "anthropic",
        `${base}/messages`,
        {
          "x-api-key": apiKey,
          "anthropic-version": "2023-06-01",
          "Content-Type": "application/json",
        },
        { model, max_tokens: 64, messages: [{ role: "user", content: prompt }] },
        apiKey,
        fetchImpl,
        (body) => {
          const text = body?.content?.[0]?.text;
          if (typeof text !== "string" || text.length === 0) {
            throw new Error("anthropic returned an empty completion");
          }
          return text;
        },
      ),
  };
}

function ollamaProvider(config: ProviderConfig, fetchImpl: FetchLike): MoveProvider {
  const base = (config.baseUrl || providerDefaults("ollama").baseUrl).replace(/\/$/, "");
  const model = config.model || providerDefaults("ollama").model;
  return {
    id: "ollama",
    complete: (prompt) =>
      postJson(
        "ollama",
        `${base}/api/chat`,
        { "Content-Type": "application/json" },
        { model, stream: false, messages: [{ role: "user", content: prompt }] },
        undefined,
        fetchImpl,
        (body) => {
          const text = body?.message?.content;
          if (typeof text !== "string" || text.length === 0) {
            throw new Error("ollama returned an empty completion");
          }
          return text;
        },
      ),
  };
}

async function postJson(
  id: string,
  url: string,
  headers: Record<string, string>,
  body: unknown,
  secret: string | undefined,
  fetchImpl: FetchLike,
  read: (body: any) => string,
): Promise<string> {
  let response: Response;
  try {
    response = await fetchImpl(url, {
      method: "POST",
      headers,
      body: JSON.stringify(body),
    });
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`${id} is unreachable: ${redact(message, [secret])}`);
  }
  const raw = await response.text();
  const safe = redact(raw, [secret]);
  if (!response.ok) {
    throw new Error(`${id} request failed (${response.status}): ${safe.slice(0, 300)}`);
  }
  let parsed: any;
  try {
    parsed = JSON.parse(raw);
  } catch {
    throw new Error(`${id} returned a response that was not JSON`);
  }
  try {
    return read(parsed);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(redact(message, [secret]));
  }
}
