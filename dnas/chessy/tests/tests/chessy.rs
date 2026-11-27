//! Two-conductor sweettests for profiles, the lobby, chess rules, and endings.
//!
//! Run after `scripts/build-happ.sh` so `dnas/chessy/workdir/chessy.dna` exists:
//!
//! ```text
//! cargo test -p chessy_sweettest -- --test-threads=1
//! ```

use std::path::PathBuf;

use chess_rules::Status;
use chessy::{ChallengeView, GameView, MoveInput, ProfileInput, ProfileView};
use chessy_integrity::ColorPreference;
use holochain::prelude::*;
use holochain::sweettest::*;

fn dna_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../workdir/chessy.dna")
}

struct Peers {
    conductors: SweetConductorBatch,
    cells: Vec<SweetCell>,
}

impl Peers {
    async fn new(n: usize) -> Self {
        let path = dna_path();
        assert!(
            path.exists(),
            "missing {} — run scripts/build-happ.sh first",
            path.display()
        );
        let mut conductors = SweetConductorBatch::standard(n).await;
        let dna = SweetDnaFile::from_bundle(&path)
            .await
            .expect("dna bundle");
        let cells = conductors
            .setup_app("chessy", &[dna])
            .await
            .expect("install chessy")
            .cells_flattened();
        Self { conductors, cells }
    }

    fn zome(cell: &SweetCell) -> SweetZome {
        cell.zome("chessy")
    }

    async fn call<I, O>(&self, index: usize, fn_name: &str, payload: I) -> O
    where
        I: serde::Serialize + std::fmt::Debug,
        O: serde::de::DeserializeOwned + std::fmt::Debug,
    {
        self.conductors[index]
            .call(&Self::zome(&self.cells[index]), fn_name, payload)
            .await
    }

    async fn fails<I, O>(&self, index: usize, fn_name: &str, payload: I) -> bool
    where
        I: serde::Serialize + std::fmt::Debug,
        O: serde::de::DeserializeOwned + std::fmt::Debug,
    {
        self.conductors[index]
            .call_fallible::<I, O>(&Self::zome(&self.cells[index]), fn_name, payload)
            .await
            .is_err()
    }

    async fn sync(&self) {
        await_consistency(&self.cells).await.expect("consistency");
    }
}

fn profile(nickname: &str) -> ProfileInput {
    ProfileInput {
        nickname: nickname.into(),
        agent_name: Some("stub".into()),
        model_label: Some("first-legal".into()),
    }
}

async fn play(peers: &Peers, moves: &[(&str, usize)]) -> ActionHash {
    let game = peers
        .call::<_, ActionHash>(0, "create_challenge", ColorPreference::White)
        .await;
    peers.sync().await;
    let game = peers
        .call::<_, ActionHash>(1, "accept_challenge", game)
        .await;
    peers.sync().await;
    let mut tip = peers
        .call::<_, GameView>(0, "get_game_state", game)
        .await
        .action_hash;
    for (uci, player) in moves {
        tip = peers
            .call::<_, ActionHash>(
                *player,
                "make_move",
                MoveInput {
                    game: tip.clone(),
                    uci: (*uci).into(),
                },
            )
            .await;
        peers.sync().await;
    }
    tip
}

#[tokio::test(flavor = "multi_thread")]
async fn profiles_are_signed_and_readable_by_peers() {
    let peers = Peers::new(2).await;
    peers
        .call::<_, ActionHash>(0, "create_profile", profile("Ada"))
        .await;
    peers
        .call::<_, ActionHash>(0, "update_profile", profile("Ada Lovelace"))
        .await;
    peers.sync().await;
    let ada: AgentPubKey = peers.call(0, "whoami", ()).await;
    let seen: Option<ProfileView> = peers.call(1, "get_profile", ada).await;
    let seen = seen.expect("profile");
    assert_eq!(seen.nickname, "Ada Lovelace");
    assert!(peers.fails::<_, ActionHash>(1, "create_profile", profile("")).await);
    assert!(peers.fails::<_, ActionHash>(1, "create_profile", profile(&"x".repeat(40))).await);
}

#[tokio::test(flavor = "multi_thread")]
async fn lobby_accepts_once_and_assigns_colors() {
    let peers = Peers::new(3).await;
    peers.call::<_, ActionHash>(0, "create_profile", profile("White")).await;
    let open = peers
        .call::<_, ActionHash>(0, "create_challenge", ColorPreference::White)
        .await;
    peers.sync().await;

    let listed: Vec<ChallengeView> = peers.call(1, "list_open_challenges", ()).await;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].nickname.as_deref(), Some("White"));
    assert!(peers.fails::<_, ActionHash>(1, "cancel_challenge", open.clone()).await);

    let game = peers
        .call::<_, ActionHash>(1, "accept_challenge", open.clone())
        .await;
    peers.sync().await;
    assert!(peers.fails::<_, ActionHash>(2, "accept_challenge", open.clone()).await);

    let state: GameView = peers.call(0, "get_game_state", game).await;
    let white: AgentPubKey = peers.call(0, "whoami", ()).await;
    let black: AgentPubKey = peers.call(1, "whoami", ()).await;
    assert_eq!(state.white, white);
    assert_eq!(state.black, black);
    assert_eq!(state.status, Status::InProgress);
    assert_eq!(state.side_to_move, "white");
    assert!(state.fen.starts_with("rnbqkbnr/pppppppp"));
    assert_eq!(state.legal_moves.len(), 20);

    let another = peers
        .call::<_, ActionHash>(0, "create_challenge", ColorPreference::Black)
        .await;
    peers.sync().await;
    peers.call::<_, ActionHash>(0, "cancel_challenge", another.clone()).await;
    peers.sync().await;
    assert!(peers.fails::<_, ActionHash>(2, "accept_challenge", another.clone()).await);
    let listed: Vec<ChallengeView> = peers.call(2, "list_open_challenges", ()).await;
    assert!(listed.iter().all(|c| c.action_hash != another));
}

#[tokio::test(flavor = "multi_thread")]
async fn peers_reject_illegal_and_out_of_turn_moves() {
    let peers = Peers::new(2).await;
    let tip = play(&peers, &[]).await;
    assert!(
        peers
            .fails::<_, ActionHash>(
                0,
                "make_move",
                MoveInput { game: tip.clone(), uci: "e2e5".into() }
            )
            .await
    );
    let tip = peers
        .call::<_, ActionHash>(
            0,
            "make_move",
            MoveInput { game: tip, uci: "e2e4".into() },
        )
        .await;
    peers.sync().await;
    assert!(
        peers
            .fails::<_, ActionHash>(
                0,
                "make_move",
                MoveInput { game: tip.clone(), uci: "a2a3".into() }
            )
            .await
    );
    let state: GameView = peers.call(1, "get_game_state", tip).await;
    assert_eq!(state.moves, vec!["e2e4".to_string()]);
    assert_eq!(state.side_to_move, "black");
    assert!(state.my_turn);
}

#[tokio::test(flavor = "multi_thread")]
async fn checkmate_stalemate_resign_and_draw() {
    let peers = Peers::new(2).await;
    let tip = play(
        &peers,
        &[("f2f3", 0), ("e7e5", 1), ("g2g4", 0), ("d8h4", 1)],
    )
    .await;
    let state: GameView = peers.call(0, "get_game_state", tip.clone()).await;
    assert_eq!(state.status, Status::Checkmate);
    assert_eq!(state.winner.as_ref(), Some(&state.black));
    assert!(state.legal_moves.is_empty());
    assert!(
        peers
            .fails::<_, ActionHash>(
                0,
                "make_move",
                MoveInput { game: tip, uci: "a2a3".into() }
            )
            .await
    );

    let peers = Peers::new(2).await;
    let tip = play(
        &peers,
        &[
            ("e2e3", 0),
            ("a7a5", 1),
            ("d1h5", 0),
            ("a8a6", 1),
            ("h5a5", 0),
            ("h7h5", 1),
            ("a5c7", 0),
            ("a6h6", 1),
            ("h2h4", 0),
            ("f7f6", 1),
            ("c7d7", 0),
            ("e8f7", 1),
            ("d7b7", 0),
            ("d8d3", 1),
            ("b7b8", 0),
            ("d3h7", 1),
            ("b8c8", 0),
            ("f7g6", 1),
            ("c8e6", 0),
        ],
    )
    .await;
    let state: GameView = peers.call(1, "get_game_state", tip).await;
    assert_eq!(state.status, Status::Stalemate);
    assert!(state.winner.is_none());

    let peers = Peers::new(2).await;
    let tip = play(&peers, &[("e2e4", 0)]).await;
    let ended = peers.call::<_, ActionHash>(1, "resign", tip).await;
    peers.sync().await;
    let state: GameView = peers.call(0, "get_game_state", ended.clone()).await;
    assert_eq!(state.status, Status::Resigned);
    assert_eq!(state.winner.as_ref(), Some(&state.white));
    assert!(peers.fails::<_, ActionHash>(0, "resign", ended).await);

    let peers = Peers::new(2).await;
    let tip = play(&peers, &[]).await;
    let offered = peers.call::<_, ActionHash>(0, "offer_draw", tip).await;
    peers.sync().await;
    let state: GameView = peers.call(1, "get_game_state", offered.clone()).await;
    assert_eq!(state.status, Status::DrawOffered);
    assert!(state.my_turn);
    let drawn = peers.call::<_, ActionHash>(1, "accept_draw", offered).await;
    peers.sync().await;
    let state: GameView = peers.call(0, "get_game_state", drawn.clone()).await;
    assert_eq!(state.status, Status::AgreedDraw);
    assert!(
        peers
            .fails::<_, ActionHash>(
                0,
                "make_move",
                MoveInput { game: drawn, uci: "e2e4".into() }
            )
            .await
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn threefold_repetition_ends_the_game() {
    let peers = Peers::new(2).await;
    let tip = play(
        &peers,
        &[
            ("g1f3", 0),
            ("g8f6", 1),
            ("f3g1", 0),
            ("f6g8", 1),
            ("g1f3", 0),
            ("g8f6", 1),
            ("f3g1", 0),
            ("f6g8", 1),
        ],
    )
    .await;
    let state: GameView = peers.call(0, "get_game_state", tip).await;
    assert_eq!(state.status, Status::ThreefoldRepetition);
    assert!(state.end_reason.is_some());
}
