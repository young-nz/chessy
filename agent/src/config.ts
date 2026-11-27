import { readFileSync } from "node:fs";
import { providerDefaults } from "@chessy/agent-core";

export interface AgentConfig {
  appUrl: string;
  token: string;
  installedAppId: string;
  origin: string;
  provider: string;
  model: string;
  baseUrl: string;
  apiKey: string;
  game: string;
  pollMs: number;
}

const FILE_KEYS = [
  "appUrl",
  "token",
  "installedAppId",
  "origin",
  "provider",
  "model",
  "baseUrl",
  "apiKey",
  "game",
  "pollMs",
] as const;

export function loadConfig(env: NodeJS.ProcessEnv = process.env, filePath?: string): AgentConfig {
  const fromFile = filePath ? readJson(filePath) : {};
  const provider = env.CHESSY_PROVIDER || fromFile.provider || "stub";
  const defaults = providerDefaults(provider);
  const apiKey =
    env.CHESSY_API_KEY ||
    (defaults.env ? env[defaults.env] : undefined) ||
    fromFile.apiKey ||
    "";
  return {
    appUrl: env.CHESSY_APP_URL || fromFile.appUrl || "ws://127.0.0.1:8888",
    token: env.CHESSY_APP_TOKEN || fromFile.token || "",
    installedAppId: env.CHESSY_INSTALLED_APP_ID || fromFile.installedAppId || "chessy",
    origin: env.CHESSY_ORIGIN || fromFile.origin || "chessy",
    provider,
    model: env.CHESSY_MODEL || fromFile.model || defaults.model,
    baseUrl: env.CHESSY_BASE_URL || fromFile.baseUrl || defaults.baseUrl,
    apiKey,
    game: env.CHESSY_GAME || fromFile.game || "",
    pollMs: Number(env.CHESSY_POLL_MS || fromFile.pollMs || 2000),
  };
}

function readJson(path: string): Partial<AgentConfig> {
  const parsed = JSON.parse(readFileSync(path, "utf8")) as Record<string, unknown>;
  const config: Partial<AgentConfig> = {};
  for (const key of FILE_KEYS) {
    if (parsed[key] !== undefined) {
      (config as Record<string, unknown>)[key] = parsed[key];
    }
  }
  return config;
}
