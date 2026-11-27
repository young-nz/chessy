import { chooseMove, type MoveProvider } from "@chessy/agent-core";

export interface GameSnapshot {
  actionHash: string;
  status: string;
  myTurn: boolean;
  fen: string;
  history: string[];
  legalMoves: string[];
  sideToMove: string;
}

export interface ChessPort {
  listMyGames(): Promise<GameSnapshot[]>;
  commitMove(actionHash: string, uci: string): Promise<void>;
}

export type TurnResult =
  | { kind: "moved"; actionHash: string; uci: string; usedFallback: boolean }
  | { kind: "waiting"; actionHash: string }
  | { kind: "ended"; actionHash: string; status: string }
  | { kind: "idle" };

const TERMINAL = new Set([
  "checkmate",
  "stalemate",
  "resigned",
  "agreed_draw",
  "insufficient_material",
  "fifty_move",
  "threefold_repetition",
]);

export async function takeTurn(input: {
  port: ChessPort;
  provider: MoveProvider;
  log: (line: string) => void;
  secrets?: Array<string | undefined>;
  game?: string;
}): Promise<TurnResult> {
  const games = await input.port.listMyGames();
  const selected = games.filter((game) => !input.game || game.actionHash === input.game);
  const active = selected.find((game) => !TERMINAL.has(game.status) && game.myTurn)
    ?? selected.find((game) => !TERMINAL.has(game.status))
    ?? selected[0];
  if (!active) return { kind: "idle" };
  if (TERMINAL.has(active.status)) {
    return { kind: "ended", actionHash: active.actionHash, status: active.status };
  }
  if (!active.myTurn || active.status === "draw_offered") {
    return { kind: "waiting", actionHash: active.actionHash };
  }
  const chosen = await chooseMove({
    fen: active.fen,
    history: active.history,
    legalMoves: active.legalMoves,
    sideToMove: active.sideToMove,
    provider: input.provider,
    log: input.log,
    secrets: input.secrets,
  });
  await input.port.commitMove(active.actionHash, chosen.uci);
  return {
    kind: "moved",
    actionHash: active.actionHash,
    uci: chosen.uci,
    usedFallback: chosen.usedFallback,
  };
}

export async function runUntilSettled(input: {
  ports: Array<{ port: ChessPort; provider: MoveProvider; name: string }>;
  log: (line: string) => void;
  maxPlies?: number;
}): Promise<number> {
  let plies = 0;
  const limit = input.maxPlies ?? 80;
  while (plies < limit) {
    let moved = false;
    for (const seat of input.ports) {
      const result = await takeTurn({
        port: seat.port,
        provider: seat.provider,
        log: (line) => input.log(`${seat.name}: ${line}`),
      });
      if (result.kind === "moved") {
        moved = true;
        plies += 1;
        input.log(`${seat.name} played ${result.uci}`);
      }
      if (result.kind === "ended") return plies;
    }
    if (!moved) return plies;
  }
  return plies;
}
