import { createProvider } from "@chessy/agent-core";
import { ChessyClient, type GameView } from "@chessy/client";
import { loadConfig } from "./config.ts";
import { takeTurn, type ChessPort, type GameSnapshot } from "./runner.ts";

function snapshot(game: GameView): GameSnapshot {
  return {
    actionHash: game.action_hash,
    status: game.status,
    myTurn: game.my_turn && game.status === "in_progress",
    fen: game.fen,
    history: game.moves,
    legalMoves: game.legal_moves,
    sideToMove: game.side_to_move,
  };
}

function holochainPort(client: ChessyClient): ChessPort {
  return {
    async listMyGames() {
      const games = await client.myGames();
      return games.map(snapshot);
    },
    async commitMove(actionHash, uci) {
      await client.makeMove(actionHash, uci);
    },
  };
}

const configPath = process.argv[2];
const config = loadConfig(process.env, configPath);
const log = (line: string) => console.log(line);

const provider = createProvider({
  provider: config.provider,
  model: config.model,
  baseUrl: config.baseUrl,
  apiKey: config.apiKey,
});

const client = await ChessyClient.connect({
  appUrl: config.appUrl,
  token: config.token,
  installedAppId: config.installedAppId,
  origin: config.origin,
});

log(`connected as ${client.myKey.slice(0, 8)}… using provider ${provider.id}`);
const port = holochainPort(client);

for (;;) {
  try {
    const result = await takeTurn({
      port,
      provider,
      log,
      secrets: [config.apiKey],
      game: config.game || undefined,
    });
    if (result.kind === "ended") {
      log(`game ${result.actionHash.slice(0, 8)} ended: ${result.status}`);
      break;
    }
    if (result.kind === "moved") {
      log(`played ${result.uci}${result.usedFallback ? " (fallback)" : ""}`);
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    log(message);
  }
  await new Promise((resolve) => setTimeout(resolve, config.pollMs));
}

await client.close();
