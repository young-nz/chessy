import {
  AppWebsocket,
  CellType,
  type ActionHash,
  type AgentPubKey,
} from "@holochain/client";

export const ROLE = "chessy";
export const ZOME = "chessy";

export interface ConnectionConfig {
  appUrl: string;
  token: string;
  installedAppId?: string;
  origin?: string;
}

export interface ProfileInput {
  nickname: string;
  agent_name?: string | null;
  model_label?: string | null;
}

export interface GameView {
  action_hash: string;
  original_hash: string;
  challenge: string;
  white: string;
  black: string;
  white_nickname: string | null;
  black_nickname: string | null;
  moves: string[];
  fen: string;
  legal_moves: string[];
  side_to_move: string;
  actor: string | null;
  status: string;
  winner: string | null;
  draw_offer_by: string | null;
  end_reason: string | null;
  my_turn: boolean;
}

export interface ChallengeView {
  action_hash: string;
  challenger: string;
  nickname: string | null;
  color_preference: string;
  status: string;
}

export interface ChessySignal {
  type: string;
  [key: string]: unknown;
}

type RawHash = ActionHash | Uint8Array | number[];

export function bytesToHex(bytes: ArrayLike<number>): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

export function hexToBytes(hex: string): Uint8Array {
  const clean = hex.replace(/^0x/, "");
  if (clean.length % 2 !== 0) throw new Error("hash hex is malformed");
  const out = new Uint8Array(clean.length / 2);
  for (let i = 0; i < out.length; i += 1) {
    out[i] = Number.parseInt(clean.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

function asHex(value: RawHash | string): string {
  if (typeof value === "string") return value;
  return bytesToHex(value as ArrayLike<number>);
}

function asHash(value: string): ActionHash {
  return hexToBytes(value) as ActionHash;
}

/** Conductor app tokens are raw bytes, usually printed as base64 or hex. */
export function decodeToken(token: string): number[] {
  const trimmed = token.trim();
  if (/^[0-9a-fA-F]+$/.test(trimmed) && trimmed.length >= 32 && trimmed.length % 2 === 0) {
    return Array.from(hexToBytes(trimmed));
  }
  const binary = atob(trimmed);
  return Array.from(binary, (char) => char.charCodeAt(0));
}

function viewGame(raw: any): GameView {
  return {
    ...raw,
    action_hash: asHex(raw.action_hash),
    original_hash: asHex(raw.original_hash),
    challenge: asHex(raw.challenge),
    white: asHex(raw.white),
    black: asHex(raw.black),
    winner: raw.winner ? asHex(raw.winner) : null,
    draw_offer_by: raw.draw_offer_by ? asHex(raw.draw_offer_by) : null,
  };
}

function viewChallenge(raw: any): ChallengeView {
  return {
    ...raw,
    action_hash: asHex(raw.action_hash),
    challenger: asHex(raw.challenger),
  };
}

export class ChessyClient {
  app: AppWebsocket;
  cellId: [AgentPubKey, ActionHash];
  myKey: string;

  constructor(app: AppWebsocket, cellId: [AgentPubKey, ActionHash], myKey: string) {
    this.app = app;
    this.cellId = cellId;
    this.myKey = myKey;
  }

  static async connect(config: ConnectionConfig): Promise<ChessyClient> {
    if (!config.appUrl || !config.token) {
      throw new Error(
        "Set the local conductor WebSocket URL and app token. Chessy does not embed a shared server.",
      );
    }
    const origin = config.origin || "chessy";
    const app = await AppWebsocket.connect({
      url: new URL(config.appUrl),
      token: decodeToken(config.token),
      wsClientOptions: { origin },
    });
    const info = await app.appInfo();
    if (!info) throw new Error("The conductor did not return app info. Check the app id and token.");
    const cells = info.cell_info[ROLE];
    const provisioned = cells?.find((cell) => cell.type === CellType.Provisioned);
    if (!provisioned || provisioned.type !== CellType.Provisioned) {
      throw new Error(`No provisioned "${ROLE}" cell in this hApp.`);
    }
    const cellId = provisioned.value.cell_id;
    return new ChessyClient(app, cellId as [AgentPubKey, ActionHash], bytesToHex(cellId[0]));
  }

  onSignal(handler: (signal: ChessySignal) => void): void {
    this.app.on("signal", (signal: any) => {
      const payload = signal?.payload ?? signal?.data?.payload ?? signal;
      if (payload && typeof payload === "object" && "type" in payload) {
        handler(normalizeSignal(payload));
      }
    });
  }

  async close(): Promise<void> {
    const transport = this.app.client as { close?: () => Promise<unknown> };
    if (transport.close) await transport.close();
  }

  call<T>(fn: string, payload: unknown): Promise<T> {
    return this.app.callZome({
      cell_id: this.cellId,
      zome_name: ZOME,
      fn_name: fn,
      payload,
      provenance: this.cellId[0],
    }) as Promise<T>;
  }

  whoami(): Promise<string> {
    return this.call<RawHash>("whoami", null).then(asHex);
  }

  createProfile(input: ProfileInput): Promise<string> {
    return this.call<RawHash>("create_profile", input).then(asHex);
  }

  updateProfile(input: ProfileInput): Promise<string> {
    return this.call<RawHash>("update_profile", input).then(asHex);
  }

  myProfile(): Promise<{ nickname: string; agent_name: string | null; model_label: string | null } | null> {
    return this.call("get_my_profile", null);
  }

  createChallenge(colorPreference: "white" | "black" | "random"): Promise<string> {
    return this.call<RawHash>("create_challenge", colorPreference).then(asHex);
  }

  cancelChallenge(challenge: string): Promise<string> {
    return this.call<RawHash>("cancel_challenge", asHash(challenge)).then(asHex);
  }

  acceptChallenge(challenge: string): Promise<string> {
    return this.call<RawHash>("accept_challenge", asHash(challenge)).then(asHex);
  }

  listOpenChallenges(): Promise<ChallengeView[]> {
    return this.call<any[]>("list_open_challenges", null).then((rows) => rows.map(viewChallenge));
  }

  makeMove(game: string, uci: string): Promise<string> {
    return this.call<RawHash>("make_move", { game: asHash(game), uci }).then(asHex);
  }

  resign(game: string): Promise<string> {
    return this.call<RawHash>("resign", asHash(game)).then(asHex);
  }

  offerDraw(game: string): Promise<string> {
    return this.call<RawHash>("offer_draw", asHash(game)).then(asHex);
  }

  acceptDraw(game: string): Promise<string> {
    return this.call<RawHash>("accept_draw", asHash(game)).then(asHex);
  }

  declineDraw(game: string): Promise<string> {
    return this.call<RawHash>("decline_draw", asHash(game)).then(asHex);
  }

  gameState(game: string): Promise<GameView> {
    return this.call<any>("get_game_state", asHash(game)).then(viewGame);
  }

  myGames(): Promise<GameView[]> {
    return this.call<any[]>("list_my_games", null).then((rows) => rows.map(viewGame));
  }

  allGames(): Promise<GameView[]> {
    return this.call<any[]>("list_games", null).then((rows) => rows.map(viewGame));
  }
}

function normalizeSignal(payload: any): ChessySignal {
  const signal = { ...payload, type: String(payload.type) };
  for (const key of ["challenge", "game", "challenger", "opponent", "by", "winner"]) {
    if (signal[key] && typeof signal[key] !== "string") {
      signal[key] = asHex(signal[key]);
    }
  }
  return signal;
}

/** Profile fields that are safe to publish. The API key is intentionally absent. */
export function publicProfile(input: {
  nickname: string;
  provider: string;
  model: string;
}): ProfileInput {
  return {
    nickname: input.nickname.trim(),
    agent_name: input.provider || null,
    model_label: input.model || null,
  };
}
