import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { destinations, fenToSquares, indexToSquare, matchingMoves } from "../src/fen.ts";

describe("board geometry", () => {
  it("parses the starting position and names squares", () => {
    const squares = fenToSquares("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
    assert.equal(squares[0], "r");
    assert.equal(squares[4], "k");
    assert.equal(indexToSquare(0), "a8");
    assert.equal(indexToSquare(63), "h1");
  });

  it("blocks a click that is not in the legal list", () => {
    const legal = ["e2e4", "e2e3", "g1f3"];
    assert.deepEqual(destinations("e2", legal), ["e4", "e3"]);
    assert.deepEqual(matchingMoves("e2", "e5", legal), []);
    assert.deepEqual(matchingMoves("e7", "e8", ["e7e8q", "e7e8r"]), ["e7e8q", "e7e8r"]);
  });
});
