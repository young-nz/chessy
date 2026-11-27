import { chooseMove, createProvider } from "@chessy/agent-core";
import {
  ChessyClient,
  publicProfile,
  type ChallengeView,
  type ChessySignal,
  type GameView,
} from "@chessy/client";
import { Chess } from "chess.js";
import { destinations, fenToSquares, glyph, indexToSquare, matchingMoves, squareToIndex } from "./fen.ts";

interface Settings {
  appUrl: string;
  token: string;
  nickname: string;
  provider: string;
  model: string;
  baseUrl: string;
  apiKey: string;
}

const SETTINGS_KEY = "chessy.settings";

const app = document.querySelector<HTMLDivElement>("#app");
if (!app) throw new Error("missing #app");

const state: {
  settings: Settings;
  client: ChessyClient | null;
  status: string;
  error: string;
  view: "lobby" | "game";
  challenges: ChallengeView[];
  games: GameView[];
  game: GameView | null;
  preview: Chess | null;
  selected: string | null;
  handoff: boolean;
  log: string[];
  promotion: { from: string; to: string; choices: string[] } | null;
} = {
  settings: loadSettings(),
  client: null,
  status: "Not connected",
  error: "",
  view: "lobby",
  challenges: [],
  games: [],
  game: null,
  preview: null,
  selected: null,
  handoff: false,
  log: [],
  promotion: null,
};

let pollTimer = 0;
let handoffTimer = 0;

function loadSettings(): Settings {
  const fallback: Settings = {
    appUrl: "ws://127.0.0.1:8888",
    token: "",
    nickname: "Player",
    provider: "stub",
    model: "stub",
    baseUrl: "",
    apiKey: "",
  };
  try {
    return { ...fallback, ...JSON.parse(localStorage.getItem(SETTINGS_KEY) || "{}") };
  } catch {
    return fallback;
  }
}

function saveSettings(): void {
  localStorage.setItem(SETTINGS_KEY, JSON.stringify(state.settings));
}

function note(line: string): void {
  state.log = [`${new Date().toLocaleTimeString()} ${line}`, ...state.log].slice(0, 12);
  render();
}

function currentFen(): string {
  return state.preview?.fen() ?? state.game?.fen ?? "";
}

function currentLegal(): string[] {
  if (state.preview) {
    return state.preview.moves({ verbose: true }).map((move) => `${move.from}${move.to}${move.promotion ?? ""}`);
  }
  return state.game?.legal_moves ?? [];
}

function currentStatus(): string {
  if (state.preview) {
    if (state.preview.isCheckmate()) return "checkmate";
    if (state.preview.isStalemate()) return "stalemate";
    if (state.preview.isDraw()) return "draw";
    return "in_progress";
  }
  return state.game?.status ?? "";
}

function render(): void {
  const connected = Boolean(state.client);
  app!.innerHTML = `
    <div class="shell">
      <aside class="rail">
        <div class="brand">
          <div class="mark">♞</div>
          <div>
            <div class="eyebrow">Holochain</div>
            <h1>Chessy</h1>
          </div>
        </div>
        <p class="muted">Your conductor, your agent, your key. Peers check every move.</p>
        <div class="panel">
          <div><span class="status-dot ${connected ? "on" : ""}"></span>${escape(state.status)}</div>
          ${state.error ? `<div class="error">${escape(state.error)}</div>` : ""}
          <label>Conductor URL<input id="appUrl" value="${escape(state.settings.appUrl)}" /></label>
          <label>App token<input id="token" value="${escape(state.settings.token)}" /></label>
          <label>Nickname<input id="nickname" value="${escape(state.settings.nickname)}" /></label>
          <div class="row">
            <button class="primary" id="connect">Connect</button>
            <button class="ghost" id="preview">Local preview</button>
          </div>
        </div>
        <div class="panel">
          <h3>Agent</h3>
          <label>Provider
            <select id="provider">
              ${["stub", "openai", "anthropic", "xai", "ollama", "openai-compatible"]
                .map((id) => `<option ${id === state.settings.provider ? "selected" : ""}>${id}</option>`)
                .join("")}
            </select>
          </label>
          <label>Model<input id="model" value="${escape(state.settings.model)}" /></label>
          <label>Base URL<input id="baseUrl" placeholder="optional" value="${escape(state.settings.baseUrl)}" /></label>
          <label>API key<input id="apiKey" type="password" autocomplete="off" value="${escape(state.settings.apiKey)}" /></label>
          <p class="muted">The key is stored in this browser only. It is not written to a game record.</p>
          <div class="row">
            <button class="ghost" id="save-settings">Save locally</button>
            <button class="primary" id="handoff" ${state.game || state.preview ? "" : "disabled"}>${state.handoff ? "Stop agent" : "Hand off to agent"}</button>
          </div>
        </div>
        <div class="log">${state.log.map(escape).join("\n") || "Agent log is empty."}</div>
      </aside>
      <main class="stage">
        ${state.view === "game" ? boardHtml() : lobbyHtml()}
      </main>
    </div>
    ${state.promotion ? promotionHtml() : ""}
  `;
  bind();
}

function lobbyHtml(): string {
  const challenges = state.challenges
    .map(
      (challenge) => `
      <div class="challenge">
        <div><strong>${escape(challenge.nickname || challenge.challenger.slice(0, 8))}</strong>
          <div class="muted">${escape(challenge.color_preference)} · ${escape(challenge.status)}</div>
        </div>
        <button class="primary" data-accept="${escape(challenge.action_hash)}">Accept</button>
      </div>`,
    )
    .join("") || `<p class="muted">No open challenges yet.</p>`;
  const games = state.games
    .map(
      (game) => `
      <div class="game-link">
        <div>${escape(game.white_nickname || "White")} vs ${escape(game.black_nickname || "Black")}
          <div class="muted">${escape(game.status)} · ${game.moves.length} moves</div>
        </div>
        <button class="ghost" data-watch="${escape(game.action_hash)}">Open</button>
      </div>`,
    )
    .join("");
  return `
    <div class="row" style="justify-content: space-between; align-items: end;">
      <div>
        <div class="eyebrow">Lobby</div>
        <h2>Find a game</h2>
      </div>
      <div class="row">
        <button class="primary" id="post-white">Post as white</button>
        <button class="ghost" id="post-random">Post, random color</button>
        <button class="ghost" id="refresh">Refresh</button>
      </div>
    </div>
    <section class="panel"><h3>Open challenges</h3>${challenges}</section>
    <section class="panel"><h3>Games you can watch</h3>${games || `<p class="muted">Games appear here after someone accepts a challenge.</p>`}</section>
  `;
}

function boardHtml(): string {
  const fen = currentFen();
  const squares = fen ? fenToSquares(fen) : [];
  const legal = currentLegal();
  const targets = new Set(state.selected ? destinations(state.selected, legal) : []);
  const last = lastMoveSquares();
  const cells = squares
    .map((piece, index) => {
      const square = indexToSquare(index);
      const dark = (Math.floor(index / 8) + (index % 8)) % 2 === 1;
      const classes = [
        "square",
        dark ? "dark" : "",
        state.selected === square ? "selected" : "",
        last.has(square) ? "last" : "",
      ].filter(Boolean);
      const mark = targets.has(square) && !piece
        ? `<span class="dot"></span>`
        : piece
          ? `<span class="piece ${piece === piece.toUpperCase() ? "white" : "black"}">${glyph(piece)}</span>`
          : "";
      return `<button class="${classes.join(" ")}" data-square="${square}">${mark}</button>`;
    })
    .join("");
  const game = state.game;
  const turn = state.preview
    ? state.preview.turn() === "w" ? "white" : "black"
    : game?.side_to_move ?? "";
  const title = state.preview ? "Local preview" : `${game?.white_nickname || "White"} vs ${game?.black_nickname || "Black"}`;
  return `
    <div class="row" style="justify-content: space-between;">
      <div>
        <div class="eyebrow">${state.preview ? "Preview" : state.handoff ? "Agent" : "Live"}</div>
        <h2>${escape(title)}</h2>
      </div>
      <div class="row">
        <button class="ghost" id="back">Lobby</button>
        ${game ? `<button class="ghost" id="resign">Resign</button><button class="ghost" id="offer">Offer draw</button><button class="ghost" id="accept-draw">Accept draw</button>` : ""}
      </div>
    </div>
    <div class="board-wrap">
      <div class="board">${cells}</div>
      <div class="side-card">
        <div class="pill"><strong>${escape(turn || "—")}</strong> to move<div class="muted">${escape(currentStatus().replaceAll("_", " "))}</div></div>
        <div class="pill">Moves<div class="muted">${escape((state.preview ? state.preview.history() : game?.moves ?? []).join(" ") || "None yet")}</div></div>
        <p class="muted">${state.preview ? "This board is only on this page. Connect a conductor to publish moves." : "Illegal clicks stay on the board. The conductor rejects anything the rules do not allow."}</p>
      </div>
    </div>
  `;
}

function promotionHtml(): string {
  const choices = state.promotion?.choices ?? [];
  return `<div class="modal"><div class="card"><h3>Promote</h3><div class="row">${choices
    .map((uci) => `<button class="primary" data-promote="${escape(uci)}">${escape(uci.slice(4))}</button>`)
    .join("")}</div></div></div>`;
}

function lastMoveSquares(): Set<string> {
  const moves = state.preview ? state.preview.history({ verbose: true }).map((move) => `${move.from}${move.to}`) : state.game?.moves ?? [];
  const last = moves.at(-1);
  if (!last || last.length < 4) return new Set();
  return new Set([last.slice(0, 2), last.slice(2, 4)]);
}

function bind(): void {
  input("appUrl", (value) => (state.settings.appUrl = value));
  input("token", (value) => (state.settings.token = value));
  input("nickname", (value) => (state.settings.nickname = value));
  input("model", (value) => (state.settings.model = value));
  input("baseUrl", (value) => (state.settings.baseUrl = value));
  input("apiKey", (value) => (state.settings.apiKey = value));
  const provider = document.querySelector<HTMLSelectElement>("#provider");
  provider?.addEventListener("change", () => {
    state.settings.provider = provider.value;
  });
  on("#connect", () => void connect());
  on("#preview", () => startPreview());
  on("#save-settings", () => {
    saveSettings();
    note("Saved settings on this machine.");
  });
  on("#handoff", () => void toggleHandoff());
  on("#refresh", () => void refreshLobby());
  on("#post-white", () => void post("white"));
  on("#post-random", () => void post("random"));
  on("#back", () => {
    state.view = "lobby";
    state.preview = null;
    state.selected = null;
    render();
    void refreshLobby();
  });
  on("#resign", () => void act("resign"));
  on("#offer", () => void act("offer"));
  on("#accept-draw", () => void act("accept-draw"));
  document.querySelectorAll<HTMLButtonElement>("[data-accept]").forEach((button) => {
    button.addEventListener("click", () => void accept(button.dataset.accept || ""));
  });
  document.querySelectorAll<HTMLButtonElement>("[data-watch]").forEach((button) => {
    button.addEventListener("click", () => void openGame(button.dataset.watch || ""));
  });
  document.querySelectorAll<HTMLButtonElement>("[data-square]").forEach((button) => {
    button.addEventListener("click", () => void clickSquare(button.dataset.square || ""));
  });
  document.querySelectorAll<HTMLButtonElement>("[data-promote]").forEach((button) => {
    button.addEventListener("click", () => void commitMove(button.dataset.promote || ""));
  });
}

function input(id: string, apply: (value: string) => void): void {
  const element = document.querySelector<HTMLInputElement>(`#${id}`);
  element?.addEventListener("change", () => apply(element.value));
}

function on(selector: string, handler: () => void): void {
  document.querySelector<HTMLButtonElement>(selector)?.addEventListener("click", handler);
}

async function connect(): Promise<void> {
  saveSettings();
  state.error = "";
  try {
    state.client?.close();
    state.client = await ChessyClient.connect({
      appUrl: state.settings.appUrl,
      token: state.settings.token,
      origin: "chessy",
    });
    state.preview = null;
    state.status = `Connected ${state.client.myKey.slice(0, 8)}…`;
    state.client.onSignal((signal) => void onSignal(signal));
    const published = publicProfile(state.settings);
    if (published.nickname) {
      const existing = await state.client.myProfile();
      if (existing) await state.client.updateProfile(published);
      else await state.client.createProfile(published);
    }
    window.clearInterval(pollTimer);
    pollTimer = window.setInterval(() => void refreshActive(), 2500);
    await refreshLobby();
  } catch (error) {
    state.client = null;
    state.status = "Not connected";
    state.error = error instanceof Error ? error.message : String(error);
    render();
  }
}

function startPreview(): void {
  state.client = null;
  state.preview = new Chess();
  state.game = null;
  state.view = "game";
  state.selected = null;
  state.status = "Local preview";
  state.error = "";
  note("Preview board. Moves are not published.");
  render();
}

async function refreshLobby(): Promise<void> {
  if (!state.client) {
    render();
    return;
  }
  state.challenges = await state.client.listOpenChallenges();
  state.games = await state.client.allGames();
  if (state.view === "lobby") render();
}

async function refreshActive(): Promise<void> {
  if (!state.client) return;
  if (state.view === "game" && state.game) {
    state.game = await state.client.gameState(state.game.action_hash);
    render();
    return;
  }
  if (state.view === "lobby") await refreshLobby();
}

async function post(color: "white" | "random"): Promise<void> {
  if (!state.client) {
    state.error = "Connect to your conductor before posting a challenge.";
    render();
    return;
  }
  await state.client.createChallenge(color);
  note(`Posted a ${color} challenge.`);
  await refreshLobby();
}

async function accept(hash: string): Promise<void> {
  if (!state.client) return;
  const game = await state.client.acceptChallenge(hash);
  note("Challenge accepted.");
  await openGame(game);
}

async function openGame(hash: string): Promise<void> {
  if (!state.client) return;
  state.game = await state.client.gameState(hash);
  state.preview = null;
  state.view = "game";
  state.selected = null;
  render();
}

async function onSignal(signal: ChessySignal): Promise<void> {
  note(`Signal ${signal.type}`);
  if (state.view === "game" && state.game && signal.game === state.game.action_hash) {
    state.game = await state.client!.gameState(state.game.action_hash);
    render();
  } else if (state.view === "lobby") {
    await refreshLobby();
  }
}

async function clickSquare(square: string): Promise<void> {
  const legal = currentLegal();
  const status = currentStatus();
  if (status !== "in_progress") return;
  if (state.selected) {
    const choices = matchingMoves(state.selected, square, legal);
    if (choices.length === 1) {
      await commitMove(choices[0]);
      return;
    }
    if (choices.length > 1) {
      state.promotion = { from: state.selected, to: square, choices };
      render();
      return;
    }
  }
  const piece = fenToSquares(currentFen())[squareToIndex(square)];
  const side = currentSide();
  const mine = piece && ((side === "white" && piece === piece.toUpperCase()) || (side === "black" && piece === piece.toLowerCase()));
  state.selected = mine && destinations(square, legal).length > 0 ? square : null;
  render();
}

function currentSide(): string {
  if (state.preview) return state.preview.turn() === "w" ? "white" : "black";
  return state.game?.side_to_move ?? "";
}

async function commitMove(uci: string): Promise<void> {
  state.promotion = null;
  state.selected = null;
  if (!uci) return;
  if (state.preview) {
    const played = state.preview.move({
      from: uci.slice(0, 2),
      to: uci.slice(2, 4),
      promotion: uci[4],
    });
    if (!played) note(`Blocked ${uci} before it was played.`);
    render();
    return;
  }
  if (!state.client || !state.game) return;
  if (!state.game.my_turn) {
    note("Watching. It is not your turn.");
    return;
  }
  if (!state.game.legal_moves.includes(uci)) {
    note(`Blocked ${uci}. It is not in the legal list.`);
    render();
    return;
  }
  try {
    const hash = await state.client.makeMove(state.game.action_hash, uci);
    state.game = await state.client.gameState(hash);
  } catch (error) {
    state.error = error instanceof Error ? error.message : String(error);
  }
  render();
}

async function act(kind: "resign" | "offer" | "accept-draw"): Promise<void> {
  if (!state.client || !state.game) return;
  const hash = state.game.action_hash;
  const next =
    kind === "resign"
      ? await state.client.resign(hash)
      : kind === "offer"
        ? await state.client.offerDraw(hash)
        : await state.client.acceptDraw(hash);
  state.game = await state.client.gameState(next);
  render();
}

async function toggleHandoff(): Promise<void> {
  state.handoff = !state.handoff;
  window.clearInterval(handoffTimer);
  saveSettings();
  if (!state.handoff) {
    note("Agent stopped. You have the board.");
    render();
    return;
  }
  note(`Agent armed (${state.settings.provider}). Keys stay in this browser.`);
  handoffTimer = window.setInterval(() => void agentStep(), 1600);
  render();
  void agentStep();
}

async function agentStep(): Promise<void> {
  if (!state.handoff) return;
  const fen = currentFen();
  const legal = currentLegal();
  const side = currentSide();
  const playing = state.preview ? true : Boolean(state.game?.my_turn && state.game.status === "in_progress");
  if (!playing || legal.length === 0) return;
  try {
    const provider = createProvider({
      provider: state.settings.provider,
      model: state.settings.model,
      baseUrl: state.settings.baseUrl,
      apiKey: state.settings.apiKey,
    });
    const chosen = await chooseMove({
      fen,
      history: state.preview ? state.preview.history() : state.game?.moves ?? [],
      legalMoves: legal,
      sideToMove: side,
      provider,
      secrets: [state.settings.apiKey],
      log: note,
    });
    await commitMove(chosen.uci);
  } catch (error) {
    note(error instanceof Error ? error.message : String(error));
  }
}

function escape(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

render();
