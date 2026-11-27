import {
  FALLBACK_RULE,
  MAX_ATTEMPTS,
  buildPrompt,
  fallbackMove,
  parseMove,
  type PromptContext,
} from "./prompt.ts";
import type { MoveProvider } from "./providers.ts";
import { redact } from "./redact.ts";

export interface ChooseOptions extends PromptContext {
  provider: MoveProvider;
  log: (line: string) => void;
  secrets?: Array<string | undefined>;
}

export interface ChosenMove {
  uci: string;
  usedFallback: boolean;
  attempts: number;
}

/**
 * Ask the provider up to {@link MAX_ATTEMPTS} times. Only a move from the
 * legal list is returned. After that, play the documented fallback.
 */
export async function chooseMove(options: ChooseOptions): Promise<ChosenMove> {
  const legal = [...options.legalMoves];
  if (legal.length === 0) {
    throw new Error("no legal moves; the game is not waiting for a move");
  }
  const secrets = options.secrets ?? [];
  let lastReply = "";
  let lastReason = "empty reply";
  for (let attempt = 1; attempt <= MAX_ATTEMPTS; attempt += 1) {
    const prompt = buildPrompt(options, attempt === 1 ? undefined : { reply: lastReply, reason: lastReason });
    if (attempt === 1 && !prompt.includes(options.fen)) {
      throw new Error("prompt omitted the FEN");
    }
    let reply = "";
    try {
      reply = await options.provider.complete(prompt);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      lastReply = "";
      lastReason = redact(message, secrets);
      options.log(`attempt ${attempt}: provider error: ${lastReason}`);
      continue;
    }
    const parsed = parseMove(reply, legal);
    if (parsed) {
      options.log(`attempt ${attempt}: accepted ${parsed}`);
      return { uci: parsed, usedFallback: false, attempts: attempt };
    }
    lastReply = reply.trim().slice(0, 180);
    lastReason = "not in the legal move list";
    options.log(`attempt ${attempt}: rejected reply "${redact(lastReply, secrets)}"`);
  }
  const uci = fallbackMove(legal);
  options.log(
    `fallback: playing ${uci} after ${MAX_ATTEMPTS} rejected replies (${FALLBACK_RULE})`,
  );
  return { uci, usedFallback: true, attempts: MAX_ATTEMPTS };
}
