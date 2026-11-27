const GLYPHS: Record<string, string> = {
  K: "♔",
  Q: "♕",
  R: "♖",
  B: "♗",
  N: "♘",
  P: "♙",
  k: "♚",
  q: "♛",
  r: "♜",
  b: "♝",
  n: "♞",
  p: "♟",
};

/** 64 squares, index 0 = a8, from a FEN piece-placement field. */
export function fenToSquares(fen: string): Array<string | null> {
  const placement = fen.split(" ")[0] ?? "";
  const squares: Array<string | null> = [];
  for (const row of placement.split("/")) {
    for (const char of row) {
      if (/\d/.test(char)) {
        for (let i = 0; i < Number(char); i += 1) squares.push(null);
      } else {
        squares.push(char);
      }
    }
  }
  if (squares.length !== 64) {
    throw new Error("FEN did not describe 64 squares");
  }
  return squares;
}

export function glyph(piece: string | null): string {
  if (!piece) return "";
  // FE0E asks for the text glyph so the square color, not an emoji palette, paints the piece.
  return `${GLYPHS[piece] ?? piece}\uFE0E`;
}

export function indexToSquare(index: number): string {
  const file = index % 8;
  const rank = 8 - Math.floor(index / 8);
  return `${"abcdefgh"[file]}${rank}`;
}

export function squareToIndex(square: string): number {
  const file = "abcdefgh".indexOf(square[0] ?? "");
  const rank = Number(square[1]);
  return (8 - rank) * 8 + file;
}

export function destinations(from: string, legal: string[]): string[] {
  return legal.filter((move) => move.startsWith(from)).map((move) => move.slice(2, 4));
}

export function matchingMoves(from: string, to: string, legal: string[]): string[] {
  return legal.filter((move) => move.startsWith(from + to));
}
