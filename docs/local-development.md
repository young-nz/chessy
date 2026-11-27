# Local development

Chessy builds with Rust 1.95 (see `rust-toolchain.toml`) and Node 22. Holochain **0.7.1** supplies the `hc` CLI and, for a live two-peer game, the `holochain` and `lair-keystore` binaries. Nix is optional. The commands below use the release binaries.

## Tools

```bash
rustup show   # 1.95.0 with the wasm32-unknown-unknown target
node --version
```

Download from the [Holochain 0.7.1 release](https://github.com/holochain/holochain/releases/tag/holochain-0.7.1) and put them on `PATH`:

- `hc-x86_64-unknown-linux-gnu` (or the asset for your OS) as `hc`
- `holochain-x86_64-unknown-linux-gnu` as `holochain`
- `lair-keystore-x86_64-unknown-linux-gnu` as `lair-keystore`

macOS and Windows assets use the same names with `apple-darwin` or `pc-windows-msvc.exe`.

## Build the hApp

`.cargo/config.toml` selects getrandom's custom backend so the zomes can compile to `wasm32-unknown-unknown`. The HDK implements that backend.

```bash
scripts/build-happ.sh
```

That writes:

- `dnas/chessy/workdir/chessy.dna`
- `dnas/chessy/workdir/chessy.happ`

WASM inputs are copied from `target/wasm32-unknown-unknown/release/` into `dnas/chessy/workdir/wasm/` first. Those copies and the bundles are gitignored.

## Tests that do not need a conductor binary

```bash
cargo test -p chess_rules
npm ci
npm test
```

`chess_rules` covers castling, en passant, promotion, check, mate, stalemate, resignation, agreed draws, threefold repetition, the fifty-move rule, and insufficient material.

## Two conductors

The automated form is Sweettest. It links Holochain into the test binary, so the first compile is large.

```bash
scripts/build-happ.sh
cargo test -p chessy_sweettest -- --test-threads=1
```

The suite uses two or three in-process conductors. It covers profiles, posting and cancelling challenges, a single accept, rejected illegal and out-of-turn moves, fool's mate, a stalemate line, resignation, an agreed draw, and threefold repetition.

## Two sandboxes you can point the UI at

From the repo root, with `holochain` and `lair-keystore` on `PATH`:

```bash
mkdir -p .hc
printf '\n' | hc sandbox --piped generate dnas/chessy/workdir/chessy.happ \
  --num-sandboxes 2 \
  --app-id chessy \
  --root "$(pwd)/.hc" \
  --directories alice,bob \
  --in-process-lair
```

`--piped` reads the lair passphrase from stdin. The empty line above is an empty passphrase, which is fine for a local sandbox.

In two terminals:

```bash
hc sandbox -f 19001 run 0 -p 8888
hc sandbox -f 19002 run 1 -p 8889
```

Each process prints an app interface port and an authentication token. The token is base64. Put it in the UI's token field or in `CHESSY_APP_TOKEN`. The WebSocket URL is `ws://127.0.0.1:8888` for the first sandbox and `ws://127.0.0.1:8889` for the second.

Then:

```bash
npm run dev -w @chessy/ui
```

Open the printed Vite URL, connect each browser profile (or each machine) to its own sandbox, set a nickname, and post a challenge from one side. Accept it from the other.

Admin ports change unless you pass `-f`. App interface ports passed with `-p` are persistent for that sandbox.

## UI without a conductor

The **Local preview** button plays a board in the page only. Nothing is published. Use it to click through moves, promotions, and the agent hand-off with the stub provider.
