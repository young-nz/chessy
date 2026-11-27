/**
 * Ask a model for one legal UCI move. The prompt always includes the FEN,
 * the move history, and the legal-move list so the model is not guessing
 * the position from memory.
 */

export const MAX_ATTEMPTS = 3;

/** Used when every attempt is illegal or unparseable. Stable across peers. */
export const FALLBACK_RULE =
  "first legal UCI move in lexicographic order";

export interface PromptContext {
  fen: string;
  history: string[];
  legalMoves: string[];
  sideToMove: string;
}

export function buildPrompt(
  context: PromptContext,
  previous?: { reply: string; reason: string },
): string {
  const legal = [...context.legalMoves].sort();
  const history = context.history.length > 0 ? context.history.join(" ") : "(none)";
  const lines = [
    "You are playing chess for the player to move.",
    "Reply with exactly one legal UCI move and no other text.",
    "Examples: e2e4, e7e8q, e1g1.",
    `Side to move: ${context.sideToMove}`,
    `FEN: ${context.fen}`,
    `History: ${history}`,
    `Legal moves: ${legal.join(", ")}`,
  ];
  if (previous) {
    lines.push(
      `Your previous reply was rejected (${previous.reason}): ${previous.reply || "(empty)"}`,
      "Answer again with a single move copied from the legal move list.",
    );
  }
  return lines.join("\n");
}

export function parseMove(reply: string, legalMoves: string[]): string | null {
  const legal = new Set(legalMoves.map((move) => move.toLowerCase()));
  const cleaned = reply.trim().toLowerCase();
  if (legal.has(cleaned)) return cleaned;
  const tokens = cleaned.split(/[^a-h1-8qrnb]+/).filter((token) => token.length >= 4);
  for (const token of tokens) {
    if (legal.has(token)) return token;
  }
  for (const move of legal) {
    const pattern = new RegExp(`(^|[^a-z0-9])${move}([^a-z0-9]|$)`);
    if (pattern.test(cleaned)) return move;
  }
  return null;
}

export function fallbackMove(legalMoves: string[]): string {
  const sorted = [...legalMoves].sort();
  const move = sorted[0];
  if (!move) {
    throw new Error("there is no legal move to fall back to");
  }
  return move;
}
