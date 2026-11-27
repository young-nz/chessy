import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { stubProvider } from "@chessy/agent-core";
import { Chess } from "chess.js";
import { runUntilSettled, takeTurn, type ChessPort, type GameSnapshot } from "../src/runner.ts";

function uciOf(move: { from: string; to: string; promotion?: string }): string {
  return `${move.from}${move.to}${move.promotion ?? ""}`;
}
import { loadConfig } from "../src/config.ts";
import { publicProfile } from "@chessy/client";

class MemoryTable {
  readonly chess = new Chess();
  tip = "game-1";

  snapshot(color: "w" | "b"): GameSnapshot {
    const myTurn = !this.chess.isGameOver() && this.chess.turn() === color;
    const status = this.chess.isCheckmate()
      ? "checkmate"
      : this.chess.isStalemate()
        ? "stalemate"
        : this.chess.isDraw()
          ? "agreed_draw"
          : "in_progress";
    return {
      actionHash: this.tip,
      status,
      myTurn,
      fen: this.chess.fen(),
      history: this.chess.history({ verbose: true }).map(uciOf),
      legalMoves: this.chess.moves({ verbose: true }).map(uciOf),
      sideToMove: this.chess.turn() === "w" ? "white" : "black",
    };
  }

  port(color: "w" | "b"): ChessPort {
    return {
      listMyGames: async () => [this.snapshot(color)],
      commitMove: async (_hash, uci) => {
        const from = uci.slice(0, 2);
        const to = uci.slice(2, 4);
        const promotion = uci.length > 4 ? uci[4] : undefined;
        const played = this.chess.move({ from, to, promotion });
        if (!played) throw new Error(`illegal commit ${uci}`);
        if (this.snapshot(color).sideToMove === (color === "w" ? "white" : "black") && !this.chess.isGameOver()) {
          // The side to move changed, which is what we want.
        }
      },
    };
  }
}

describe("runner", () => {
  it("does not move out of turn and stops at checkmate", async () => {
    const table = new MemoryTable();
    const logs: string[] = [];
    const plies = await runUntilSettled({
      log: (line) => logs.push(line),
      ports: [
        { name: "white", port: table.port("w"), provider: stubProvider(["f2f3", "g2g4"]) },
        { name: "black", port: table.port("b"), provider: stubProvider(["e7e5", "d8h4"]) },
      ],
    });
    assert.equal(plies, 4);
    assert.equal(table.chess.isCheckmate(), true);
    const after = await takeTurn({
      port: table.port("w"),
      provider: stubProvider(["a2a3"]),
      log: () => {},
    });
    assert.equal(after.kind, "ended");
    assert.match(logs.join("\n"), /white played f2f3/);
    assert.match(logs.join("\n"), /black played d8h4/);
  });

  it("waits when it is not this agent's turn", async () => {
    const table = new MemoryTable();
    const result = await takeTurn({
      port: table.port("b"),
      provider: stubProvider(["e7e5"]),
      log: () => {},
    });
    assert.equal(result.kind, "waiting");
    assert.equal(table.chess.history().length, 0);
  });
});

describe("local config", () => {
  it("reads the conductor and provider from the environment", () => {
    const config = loadConfig({
      CHESSY_APP_URL: "ws://127.0.0.1:9999",
      CHESSY_APP_TOKEN: "token-1",
      CHESSY_PROVIDER: "openai",
      OPENAI_API_KEY: "sk-local-only",
      CHESSY_MODEL: "gpt-test",
    });
    assert.equal(config.appUrl, "ws://127.0.0.1:9999");
    assert.equal(config.provider, "openai");
    assert.equal(config.apiKey, "sk-local-only");
    assert.equal(config.model, "gpt-test");
    const published = publicProfile({
      nickname: "Ada",
      provider: config.provider,
      model: config.model,
    });
    assert.equal(JSON.stringify(published).includes("sk-local-only"), false);
    assert.equal(published.agent_name, "openai");
  });
});
