//! Deterministic chess rules shared by the integrity zome and the agent.
//!
//! Games always start from the standard initial position. Callers replay the
//! full move list; nothing here reads the clock or the network.

use serde::{Deserialize, Serialize};
use shakmaty::fen::Fen;
use shakmaty::uci::UciMove;
use shakmaty::{CastlingMode, Chess, Color, EnPassantMode, Move, Position};
use thiserror::Error;

/// Maximum half-moves stored on a game. Keeps validation bounded.
pub const MAX_PLIES: usize = 600;

/// Documented retry budget is owned by the agent. The rules crate only
/// describes the fallback the agent must use when that budget is spent:
/// the lexicographically first legal UCI move.
pub const FALLBACK_RULE: &str = "first legal UCI move in lexicographic order";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    White,
    Black,
}

impl Side {
    pub fn other(self) -> Self {
        match self {
            Side::White => Side::Black,
            Side::Black => Side::White,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Side::White => "white",
            Side::Black => "black",
        }
    }

    fn from_color(color: Color) -> Self {
        match color {
            Color::White => Side::White,
            Color::Black => Side::Black,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    InProgress,
    /// A draw has been offered. The opponent must accept or decline.
    DrawOffered,
    Checkmate,
    Stalemate,
    Resigned,
    AgreedDraw,
    InsufficientMaterial,
    FiftyMove,
    ThreefoldRepetition,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::InProgress => "in_progress",
            Status::DrawOffered => "draw_offered",
            Status::Checkmate => "checkmate",
            Status::Stalemate => "stalemate",
            Status::Resigned => "resigned",
            Status::AgreedDraw => "agreed_draw",
            Status::InsufficientMaterial => "insufficient_material",
            Status::FiftyMove => "fifty_move",
            Status::ThreefoldRepetition => "threefold_repetition",
        }
    }

    pub fn is_terminal(self) -> bool {
        !matches!(self, Status::InProgress | Status::DrawOffered)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndReason {
    Checkmate,
    Stalemate,
    Resignation,
    Agreement,
    InsufficientMaterial,
    FiftyMove,
    ThreefoldRepetition,
}

impl EndReason {
    pub fn as_str(self) -> &'static str {
        match self {
            EndReason::Checkmate => "checkmate",
            EndReason::Stalemate => "stalemate",
            EndReason::Resignation => "resignation",
            EndReason::Agreement => "agreement",
            EndReason::InsufficientMaterial => "insufficient_material",
            EndReason::FiftyMove => "fifty_move",
            EndReason::ThreefoldRepetition => "threefold_repetition",
        }
    }
}

/// What a player is allowed to do to a position.
///
/// Only the side named by [`RulesGame::actor`] may take the action.
/// Resignation is allowed only for the side to move, so a single author
/// extends the game record at every step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerAction {
    Move(String),
    Resign(Side),
    OfferDraw(Side),
    AcceptDraw(Side),
    DeclineDraw(Side),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RulesGame {
    pub moves: Vec<String>,
    pub status: Status,
    pub winner: Option<Side>,
    pub draw_offer_by: Option<Side>,
    pub end_reason: Option<EndReason>,
}

impl RulesGame {
    pub fn initial() -> Self {
        Self {
            moves: Vec::new(),
            status: Status::InProgress,
            winner: None,
            draw_offer_by: None,
            end_reason: None,
        }
    }

    /// Side that is allowed to author the next record.
    pub fn actor(&self) -> Result<Option<Side>, RulesError> {
        match self.status {
            Status::InProgress => Ok(Some(self.side_to_move()?)),
            Status::DrawOffered => {
                let offerer = self.draw_offer_by.ok_or(RulesError::Inconsistent(
                    "draw offer missing the offering side",
                ))?;
                Ok(Some(offerer.other()))
            }
            _ => Ok(None),
        }
    }

    pub fn side_to_move(&self) -> Result<Side, RulesError> {
        let replay = replay_standard(&self.moves)?;
        Ok(replay.side_to_move)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PositionView {
    pub fen: String,
    pub legal_moves: Vec<String>,
    pub side_to_move: Side,
    pub status: Status,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RulesError {
    #[error("{0}")]
    Illegal(&'static str),
    #[error("{0}")]
    Inconsistent(&'static str),
}

pub fn apply(game: &RulesGame, action: PlayerAction) -> Result<RulesGame, RulesError> {
    let actor = game
        .actor()?
        .ok_or(RulesError::Illegal("the game has already ended"))?;
    match action {
        PlayerAction::Move(uci) => apply_move(game, actor, &uci),
        PlayerAction::Resign(side) => {
            if game.status != Status::InProgress {
                return Err(RulesError::Illegal(
                    "resignation is only valid while the game is in progress",
                ));
            }
            if side != actor {
                return Err(RulesError::Illegal(
                    "only the side to move may resign",
                ));
            }
            Ok(RulesGame {
                moves: game.moves.clone(),
                status: Status::Resigned,
                winner: Some(side.other()),
                draw_offer_by: None,
                end_reason: Some(EndReason::Resignation),
            })
        }
        PlayerAction::OfferDraw(side) => {
            if game.status != Status::InProgress {
                return Err(RulesError::Illegal(
                    "a draw can only be offered on your turn, before moving",
                ));
            }
            if side != actor {
                return Err(RulesError::Illegal("only the side to move may offer a draw"));
            }
            Ok(RulesGame {
                moves: game.moves.clone(),
                status: Status::DrawOffered,
                winner: None,
                draw_offer_by: Some(side),
                end_reason: None,
            })
        }
        PlayerAction::AcceptDraw(side) => {
            if game.status != Status::DrawOffered {
                return Err(RulesError::Illegal("there is no draw offer to accept"));
            }
            if side != actor {
                return Err(RulesError::Illegal("only the opponent may accept a draw offer"));
            }
            Ok(RulesGame {
                moves: game.moves.clone(),
                status: Status::AgreedDraw,
                winner: None,
                draw_offer_by: None,
                end_reason: Some(EndReason::Agreement),
            })
        }
        PlayerAction::DeclineDraw(side) => {
            if game.status != Status::DrawOffered {
                return Err(RulesError::Illegal("there is no draw offer to decline"));
            }
            if side != actor {
                return Err(RulesError::Illegal("only the opponent may decline a draw offer"));
            }
            Ok(RulesGame {
                moves: game.moves.clone(),
                status: Status::InProgress,
                winner: None,
                draw_offer_by: None,
                end_reason: None,
            })
        }
    }
}

fn apply_move(game: &RulesGame, _actor: Side, uci: &str) -> Result<RulesGame, RulesError> {
    if game.status != Status::InProgress {
        return Err(RulesError::Illegal(
            "moves are only accepted while the game is in progress",
        ));
    }
    if game.moves.len() >= MAX_PLIES {
        return Err(RulesError::Illegal("move history exceeds the supported length"));
    }
    let uci = normalize_uci(uci)?;
    let mut moves = game.moves.clone();
    moves.push(uci);
    let replay = replay_standard(&moves)?;
    let (status, winner, end_reason) = classify_after_move(&replay);
    Ok(RulesGame {
        moves,
        status,
        winner,
        draw_offer_by: None,
        end_reason,
    })
}

/// Confirm `next` is exactly the successor of `prev` under the rules.
pub fn transition_is_valid(prev: &RulesGame, next: &RulesGame) -> Result<PlayerAction, RulesError> {
    let action = infer_action(prev, next)?;
    let expected = apply(prev, action.clone())?;
    if &expected != next {
        return Err(RulesError::Inconsistent(
            "recorded result does not match the rules engine",
        ));
    }
    Ok(action)
}

fn infer_action(prev: &RulesGame, next: &RulesGame) -> Result<PlayerAction, RulesError> {
    if next.moves.len() == prev.moves.len() + 1
        && next.moves[..prev.moves.len()] == prev.moves[..]
    {
        return Ok(PlayerAction::Move(next.moves.last().unwrap().clone()));
    }
    if next.moves != prev.moves {
        return Err(RulesError::Illegal(
            "a game update must append exactly one move or leave the history unchanged",
        ));
    }
    match (prev.status, next.status) {
        (Status::InProgress, Status::Resigned) => {
            let winner = next
                .winner
                .ok_or(RulesError::Inconsistent("resignation requires a winner"))?;
            Ok(PlayerAction::Resign(winner.other()))
        }
        (Status::InProgress, Status::DrawOffered) => {
            let by = next.draw_offer_by.ok_or(RulesError::Inconsistent(
                "draw offer requires the offering side",
            ))?;
            Ok(PlayerAction::OfferDraw(by))
        }
        (Status::DrawOffered, Status::AgreedDraw) => {
            let by = prev.draw_offer_by.ok_or(RulesError::Inconsistent(
                "missing the side that offered the draw",
            ))?;
            Ok(PlayerAction::AcceptDraw(by.other()))
        }
        (Status::DrawOffered, Status::InProgress) => {
            let by = prev.draw_offer_by.ok_or(RulesError::Inconsistent(
                "missing the side that offered the draw",
            ))?;
            Ok(PlayerAction::DeclineDraw(by.other()))
        }
        _ => Err(RulesError::Illegal(
            "that status change is not a legal game action",
        )),
    }
}

pub fn view(game: &RulesGame) -> Result<PositionView, RulesError> {
    let replay = replay_standard(&game.moves)?;
    let legal_moves = if game.status == Status::InProgress {
        replay.legal_moves.clone()
    } else {
        Vec::new()
    };
    Ok(PositionView {
        fen: replay.fen,
        legal_moves,
        side_to_move: replay.side_to_move,
        status: game.status,
    })
}

/// Lexicographically first legal UCI move. This is the agent fallback.
pub fn fallback_move(game: &RulesGame) -> Result<String, RulesError> {
    let view = view(game)?;
    view.legal_moves
        .first()
        .cloned()
        .ok_or(RulesError::Illegal("there is no legal move to fall back to"))
}

struct Replay {
    fen: String,
    legal_moves: Vec<String>,
    side_to_move: Side,
    checkmate: bool,
    stalemate: bool,
    insufficient_material: bool,
    fifty_move: bool,
    threefold: bool,
}

fn replay_standard(moves: &[String]) -> Result<Replay, RulesError> {
    if moves.len() > MAX_PLIES {
        return Err(RulesError::Illegal("move history exceeds the supported length"));
    }
    let mut pos = Chess::default();
    let mut keys = Vec::with_capacity(moves.len() + 1);
    keys.push(repetition_key(&pos));
    for uci in moves {
        let mv = parse_legal_move(&pos, uci)?;
        pos.play_unchecked(&mv);
        keys.push(repetition_key(&pos));
    }
    let mut legal_moves = Vec::new();
    for mv in pos.legal_moves() {
        legal_moves.push(uci_of(&mv));
    }
    legal_moves.sort();
    legal_moves.dedup();
    let current = keys.last().map(|k| k.as_str()).unwrap_or("");
    let repetitions = keys.iter().filter(|k| k.as_str() == current).count();
    Ok(Replay {
        fen: Fen::from_position(pos.clone(), EnPassantMode::Legal).to_string(),
        legal_moves,
        side_to_move: Side::from_color(pos.turn()),
        checkmate: pos.is_checkmate(),
        stalemate: pos.is_stalemate(),
        insufficient_material: pos.is_insufficient_material(),
        fifty_move: pos.halfmoves() >= 100,
        threefold: repetitions >= 3,
    })
}

fn classify_after_move(replay: &Replay) -> (Status, Option<Side>, Option<EndReason>) {
    // The side that just moved is the opposite of the side now to move.
    let mover = replay.side_to_move.other();
    if replay.checkmate {
        return (
            Status::Checkmate,
            Some(mover),
            Some(EndReason::Checkmate),
        );
    }
    if replay.stalemate {
        return (Status::Stalemate, None, Some(EndReason::Stalemate));
    }
    if replay.insufficient_material {
        return (
            Status::InsufficientMaterial,
            None,
            Some(EndReason::InsufficientMaterial),
        );
    }
    if replay.fifty_move {
        return (Status::FiftyMove, None, Some(EndReason::FiftyMove));
    }
    if replay.threefold {
        return (
            Status::ThreefoldRepetition,
            None,
            Some(EndReason::ThreefoldRepetition),
        );
    }
    (Status::InProgress, None, None)
}

fn parse_legal_move(pos: &Chess, uci: &str) -> Result<Move, RulesError> {
    let normalized = normalize_uci(uci)?;
    let parsed = UciMove::from_ascii(normalized.as_bytes())
        .map_err(|_| RulesError::Illegal("move is not valid UCI"))?;
    let mv = parsed
        .to_move(pos)
        .map_err(|_| RulesError::Illegal("move is illegal in the current position"))?;
    if !pos.is_legal(&mv) {
        return Err(RulesError::Illegal(
            "move is illegal in the current position",
        ));
    }
    Ok(mv)
}

fn normalize_uci(uci: &str) -> Result<String, RulesError> {
    let trimmed = uci.trim().to_ascii_lowercase();
    if trimmed.len() < 4 || trimmed.len() > 5 || !trimmed.is_ascii() {
        return Err(RulesError::Illegal("move must be UCI such as e2e4 or e7e8q"));
    }
    Ok(trimmed)
}

fn uci_of(mv: &Move) -> String {
    UciMove::from_move(mv, CastlingMode::Standard).to_string()
}

fn repetition_key(pos: &Chess) -> String {
    let fen = Fen::from_position(pos.clone(), EnPassantMode::Legal).to_string();
    fen.split_whitespace()
        .take(4)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Replay an arbitrary FEN. Used by unit tests for end-state detectors that
/// are awkward to reach from the standard opening in a short assertion.
/// Production validation always starts from [`RulesGame::initial`].
pub fn classify_fen(fen: &str) -> Result<Status, RulesError> {
    let parsed: Fen = fen
        .parse()
        .map_err(|_| RulesError::Illegal("could not parse FEN"))?;
    let pos: Chess = parsed
        .into_position(CastlingMode::Standard)
        .map_err(|_| RulesError::Illegal("FEN is not a legal chess position"))?;
    let key = repetition_key(&pos);
    let keys = vec![key.clone(), key.clone(), key];
    let replay = Replay {
        fen: Fen::from_position(pos.clone(), EnPassantMode::Legal).to_string(),
        legal_moves: Vec::new(),
        side_to_move: Side::from_color(pos.turn()),
        checkmate: pos.is_checkmate(),
        stalemate: pos.is_stalemate(),
        insufficient_material: pos.is_insufficient_material(),
        fifty_move: pos.halfmoves() >= 100,
        threefold: keys.iter().filter(|k| *k == &keys[0]).count() >= 3 && false,
    };
    // `threefold` above is forced false: a single FEN cannot prove history.
    // Fifty-move, mate, stalemate, and insufficient material are properties
    // of the position itself (the halfmove clock is part of the FEN).
    let _ = replay.threefold;
    Ok(classify_after_move(&replay).0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play_all(ucis: &[&str]) -> RulesGame {
        let mut game = RulesGame::initial();
        for uci in ucis {
            let side = game.actor().unwrap().unwrap();
            let _ = side;
            game = apply(&game, PlayerAction::Move((*uci).to_string())).unwrap();
        }
        game
    }

    #[test]
    fn starting_position_lists_twenty_moves() {
        let view = view(&RulesGame::initial()).unwrap();
        assert_eq!(view.side_to_move, Side::White);
        assert_eq!(view.legal_moves.len(), 20);
        assert_eq!(view.legal_moves[0], "a2a3");
        assert!(view.fen.starts_with("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w"));
        assert_eq!(fallback_move(&RulesGame::initial()).unwrap(), "a2a3");
    }

    #[test]
    fn rejects_illegal_pawn_jump_and_moving_out_of_turn() {
        let start = RulesGame::initial();
        let err = apply(&start, PlayerAction::Move("e2e5".into())).unwrap_err();
        assert!(matches!(err, RulesError::Illegal(_)));

        let after = apply(&start, PlayerAction::Move("e2e4".into())).unwrap();
        let err = apply(&after, PlayerAction::Resign(Side::White)).unwrap_err();
        assert!(matches!(err, RulesError::Illegal(_)));
    }

    #[test]
    fn fools_mate_and_no_moves_after() {
        let game = play_all(&["f2f3", "e7e5", "g2g4", "d8h4"]);
        assert_eq!(game.status, Status::Checkmate);
        assert_eq!(game.winner, Some(Side::Black));
        assert_eq!(game.end_reason, Some(EndReason::Checkmate));
        let err = apply(&game, PlayerAction::Move("a2a3".into())).unwrap_err();
        assert!(matches!(err, RulesError::Illegal(_)));
        assert!(view(&game).unwrap().legal_moves.is_empty());
    }

    #[test]
    fn scholars_mate() {
        let game = play_all(&["e2e4", "e7e5", "f1c4", "b8c6", "d1h5", "g8f6", "h5f7"]);
        assert_eq!(game.status, Status::Checkmate);
        assert_eq!(game.winner, Some(Side::White));
    }

    #[test]
    fn cannot_leave_the_king_in_check() {
        let mut game = play_all(&["f2f3", "e7e5", "g2g4", "d8h4"]);
        assert_eq!(game.status, Status::Checkmate);
        game = play_all(&["f2f3", "e7e5", "g2g4"]);
        let err = apply(&game, PlayerAction::Move("a2a3".into())).unwrap_err();
        assert!(matches!(err, RulesError::Illegal(_)));
    }

    #[test]
    fn en_passant_is_legal_only_immediately() {
        let game = play_all(&["e2e4", "b8c6", "e4e5", "d7d5", "e5d6"]);
        assert_eq!(game.status, Status::InProgress);
        assert!(game.moves.last().unwrap() == "e5d6");

        let missed = play_all(&["e2e4", "b8c6", "e4e5", "d7d5", "g1f3", "a7a6"]);
        let err = apply(&missed, PlayerAction::Move("e5d6".into())).unwrap_err();
        assert!(matches!(err, RulesError::Illegal(_)));
    }

    #[test]
    fn castling_and_lost_castling_rights() {
        let mut probe = RulesGame::initial();
        for uci in ["e2e4", "e7e5", "g1f3", "b8c6", "f1c4", "g8f6", "e1g1", "f8c5", "d2d3", "e8g8"] {
            let legal = view(&probe).unwrap().legal_moves;
            assert!(
                legal.iter().any(|m| m == uci),
                "{uci} not in {legal:?} fen {}",
                view(&probe).unwrap().fen
            );
            probe = apply(&probe, PlayerAction::Move(uci.into())).unwrap();
        }
        let game = probe;
        assert_eq!(game.status, Status::InProgress);
        assert!(view(&game).unwrap().fen.contains(' '));

        let err = apply(
            &play_all(&[
                "e2e4", "e7e5", "e1e2", "e8e7", "e2e1", "e7e8", "g1f3", "g8f6", "f1c4", "f8c5",
            ]),
            PlayerAction::Move("e1g1".into()),
        )
        .unwrap_err();
        assert!(matches!(err, RulesError::Illegal(_)));
    }

    #[test]
    fn promotion_move_is_legal_from_fen() {
        let parsed: Fen = "4k3/P7/8/8/8/8/8/4K3 w - - 0 1".parse().unwrap();
        let pos: Chess = parsed.into_position(CastlingMode::Standard).unwrap();
        assert!(parse_legal_move(&pos, "a7a8q").is_ok());
        assert!(parse_legal_move(&pos, "a7a8n").is_ok());
        assert!(parse_legal_move(&pos, "a7a8").is_err());
        assert!(parse_legal_move(&pos, "a7a8k").is_err());
    }

    #[test]
    fn stalemate_line_ends_as_draw() {
        // Edward Winter's 10-move stalemate, verified by the engine below.
        let game = play_all(&[
            "e2e3", "a7a5", "d1h5", "a8a6", "h5a5", "h7h5", "a5c7", "a6h6", "h2h4", "f7f6",
            "c7d7", "e8f7", "d7b7", "d8d3", "b7b8", "d3h7", "b8c8", "f7g6", "c8e6",
        ]);
        assert_eq!(
            game.status,
            Status::Stalemate,
            "fen {}",
            view(&game).unwrap().fen
        );
        let err = apply(&game, PlayerAction::Move("a2a3".into())).unwrap_err();
        assert!(matches!(err, RulesError::Illegal(_)));
    }

    #[test]
    fn resign_offer_accept_and_decline() {
        let mut game = play_all(&["e2e4"]);
        let resigned = apply(&game, PlayerAction::Resign(Side::Black)).unwrap();
        assert_eq!(resigned.status, Status::Resigned);
        assert_eq!(resigned.winner, Some(Side::White));
        assert!(transition_is_valid(&game, &resigned).is_ok());

        game = apply(&RulesGame::initial(), PlayerAction::OfferDraw(Side::White)).unwrap();
        assert_eq!(game.status, Status::DrawOffered);
        assert_eq!(game.actor().unwrap(), Some(Side::Black));
        let declined = apply(&game, PlayerAction::DeclineDraw(Side::Black)).unwrap();
        assert_eq!(declined.status, Status::InProgress);
        let offered = apply(&declined, PlayerAction::OfferDraw(Side::White)).unwrap();
        let agreed = apply(&offered, PlayerAction::AcceptDraw(Side::Black)).unwrap();
        assert_eq!(agreed.status, Status::AgreedDraw);
        assert_eq!(agreed.end_reason, Some(EndReason::Agreement));
        assert!(apply(&agreed, PlayerAction::Move("e2e4".into())).is_err());
    }

    #[test]
    fn threefold_repetition_returns_to_the_start() {
        let game = play_all(&[
            "g1f3", "g8f6", "f3g1", "f6g8", "g1f3", "g8f6", "f3g1", "f6g8",
        ]);
        assert_eq!(game.status, Status::ThreefoldRepetition);
        assert_eq!(game.winner, None);
    }

    #[test]
    fn fifty_move_and_insufficient_material_from_fen() {
        assert_eq!(
            classify_fen("8/8/8/4k3/8/4K3/4P3/8 w - - 99 80").unwrap(),
            Status::InProgress
        );
        // A quiet king move from a 99 halfmove FEN is tested by parsing the
        // position and applying one non-capturing, non-pawn move.
        let parsed: Fen = "8/8/8/4k3/8/8/4K3/8 w - - 99 80".parse().unwrap();
        let mut pos: Chess = parsed.into_position(CastlingMode::Standard).unwrap();
        let mv = parse_legal_move(&pos, "e2e3").unwrap();
        // e2e3 is a king move here? The king is on e2. e2e3 moves the king.
        pos.play_unchecked(&mv);
        assert!(pos.halfmoves() >= 100);
        assert!(!pos.is_checkmate());

        assert_eq!(
            classify_fen("8/8/8/8/8/4k3/8/4K3 w - - 0 1").unwrap(),
            Status::InsufficientMaterial
        );
    }

    #[test]
    fn recorded_transition_must_match_the_engine() {
        let prev = RulesGame::initial();
        let mut next = apply(&prev, PlayerAction::Move("e2e4".into())).unwrap();
        assert!(transition_is_valid(&prev, &next).is_ok());
        next.status = Status::Checkmate;
        next.winner = Some(Side::White);
        next.end_reason = Some(EndReason::Checkmate);
        assert!(transition_is_valid(&prev, &next).is_err());
    }

    #[test]
    fn fallback_is_stable() {
        let game = play_all(&["e2e4", "e7e5"]);
        assert_eq!(fallback_move(&game).unwrap(), "a2a3");
    }
}
