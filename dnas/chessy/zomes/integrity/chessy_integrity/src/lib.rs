//! Integrity zome for Chessy.
//!
//! Peers re-check every profile, challenge, and game record. Chess legality
//! lives in `chess_rules` and is replayed from the standard starting position.

use chess_rules::{transition_is_valid, RulesGame, Side, Status};
use hdi::prelude::*;

pub const NICKNAME_MAX: usize = 32;
pub const LABEL_MAX: usize = 48;

#[hdk_entry_helper]
#[derive(Clone, PartialEq, Eq)]
pub struct Profile {
    pub nickname: String,
    pub agent_name: Option<String>,
    pub model_label: Option<String>,
}

#[hdk_entry_helper]
#[derive(Clone, PartialEq, Eq)]
pub struct Challenge {
    pub challenger: AgentPubKey,
    pub color_preference: ColorPreference,
    /// `None` on the original open challenge. An update points at that create.
    pub based_on: Option<ActionHash>,
    pub status: ChallengeStatus,
}

#[hdk_entry_helper]
#[derive(Clone, PartialEq, Eq)]
pub struct Game {
    pub challenge: ActionHash,
    pub white: AgentPubKey,
    pub black: AgentPubKey,
    pub moves: Vec<String>,
    pub status: Status,
    pub winner: Option<AgentPubKey>,
    pub draw_offer_by: Option<AgentPubKey>,
    pub end_reason: Option<chess_rules::EndReason>,
    /// `None` only on the initial game record. Later records point at the
    /// action they extend. Forks of the same parent are rejected.
    pub based_on: Option<ActionHash>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ColorPreference {
    White,
    Black,
    Random,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ChallengeStatus {
    Open,
    Cancelled,
    Accepted {
        opponent: AgentPubKey,
        game: ActionHash,
    },
}

#[hdk_entry_types]
#[unit_enum(UnitEntryTypes)]
pub enum EntryTypes {
    Profile(Profile),
    Challenge(Challenge),
    Game(Game),
}

#[hdk_link_types]
pub enum LinkTypes {
    AgentToProfile,
    OpenChallenge,
    AllGames,
    PlayerToGame,
    ChallengeToGame,
}

#[hdk_extern]
pub fn genesis_self_check(_data: GenesisSelfCheckData) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Valid)
}

#[hdk_extern]
pub fn validate(op: Op) -> ExternResult<ValidateCallbackResult> {
    match op.flattened()? {
        FlatOp::CreateRecord(OpRecord::CreateEntry { app_entry, action }) => {
            validate_create(app_entry, action.author().clone(), action.prev_action().cloned())
        }
        FlatOp::CreateEntry(OpEntry::CreateEntry { app_entry, action }) => {
            validate_create(app_entry, action.author().clone(), action.prev_action().cloned())
        }
        FlatOp::CreateRecord(OpRecord::UpdateEntry { app_entry, action }) => {
            validate_update(app_entry, &action)
        }
        FlatOp::CreateEntry(OpEntry::UpdateEntry { app_entry, action }) => {
            validate_update(app_entry, &action)
        }
        FlatOp::Update(OpUpdate::Entry { app_entry, action }) => validate_update(app_entry, &action),
        FlatOp::Link(link) => validate_link(link),
        FlatOp::Delete(_)
        | FlatOp::CreateRecord(OpRecord::DeleteEntry { .. }) => {
            Ok(ValidateCallbackResult::Invalid(
                "Chessy records are not deleted".into(),
            ))
        }
        _ => Ok(ValidateCallbackResult::Valid),
    }
}

fn validate_create(
    app_entry: EntryTypes,
    author: AgentPubKey,
    prev_action: Option<ActionHash>,
) -> ExternResult<ValidateCallbackResult> {
    match app_entry {
        EntryTypes::Profile(profile) => {
            if let Err(reason) = check_profile(&profile) {
                return invalid(reason);
            }
            Ok(ValidateCallbackResult::Valid)
        }
        EntryTypes::Challenge(challenge) => validate_challenge_create(&challenge, &author),
        EntryTypes::Game(game) => validate_game_create(&game, &author, prev_action),
    }
}

fn validate_update(
    app_entry: EntryTypes,
    action: &TypedAction<UpdateData>,
) -> ExternResult<ValidateCallbackResult> {
    let author = action.author();
    let prev_action = action.prev_action().cloned();
    let original = &action.original_action_address;
    match app_entry {
        EntryTypes::Profile(profile) => {
            if let Err(reason) = check_profile(&profile) {
                return invalid(reason);
            }
            let original_record = must_get_valid_record(original.clone())?;
            if original_record.action().author() != author {
                return invalid("a player cannot update another agent's profile");
            }
            if record_profile(&original_record)?.is_none() {
                return invalid("profile update does not point at a profile");
            }
            Ok(ValidateCallbackResult::Valid)
        }
        EntryTypes::Challenge(challenge) => {
            validate_challenge_update(&challenge, author, prev_action, original)
        }
        EntryTypes::Game(game) => validate_game_update(&game, author, prev_action, original),
    }
}

pub fn check_profile(profile: &Profile) -> Result<(), &'static str> {
    let nickname = profile.nickname.trim();
    if nickname.is_empty() || nickname.chars().count() > NICKNAME_MAX {
        return Err("nickname must be 1 to 32 characters");
    }
    if nickname.chars().any(|c| c.is_control()) {
        return Err("nickname cannot contain control characters");
    }
    if profile.nickname != nickname {
        return Err("nickname must not include surrounding whitespace");
    }
    check_label(profile.agent_name.as_deref(), "agent name")?;
    check_label(profile.model_label.as_deref(), "model label")?;
    Ok(())
}

fn check_label(value: Option<&str>, field: &'static str) -> Result<(), &'static str> {
    let Some(value) = value else {
        return Ok(());
    };
    if value.trim().is_empty() || value.chars().count() > LABEL_MAX || value != value.trim() {
        return Err(match field {
            "agent name" => "agent name must be 1 to 48 characters",
            _ => "model label must be 1 to 48 characters",
        });
    }
    if value.chars().any(|c| c.is_control()) {
        return Err(match field {
            "agent name" => "agent name cannot contain control characters",
            _ => "model label cannot contain control characters",
        });
    }
    Ok(())
}

fn validate_challenge_create(
    challenge: &Challenge,
    author: &AgentPubKey,
) -> ExternResult<ValidateCallbackResult> {
    if &challenge.challenger != author {
        return invalid("a player cannot forge a challenge for another agent");
    }
    if challenge.based_on.is_some() || challenge.status != ChallengeStatus::Open {
        return invalid("a new challenge must be open");
    }
    Ok(ValidateCallbackResult::Valid)
}

fn validate_challenge_update(
    challenge: &Challenge,
    author: &AgentPubKey,
    prev_action: Option<ActionHash>,
    original: &ActionHash,
) -> ExternResult<ValidateCallbackResult> {
    if challenge.based_on.as_ref() != Some(original) {
        return invalid("a challenge update must extend the original challenge");
    }
    let Some(previous) = record_challenge(&must_get_valid_record(original.clone())?)? else {
        return invalid("challenge update does not point at a challenge");
    };
    if previous.status != ChallengeStatus::Open || previous.based_on.is_some() {
        return invalid("only an open challenge can be updated");
    }
    if &previous.challenger != author || &challenge.challenger != author {
        return invalid("only the challenger can update their challenge");
    }
    if challenge.color_preference != previous.color_preference {
        return invalid("color preference is fixed when the challenge is posted");
    }
    if chain_has_challenge_extension(prev_action, original)? {
        return invalid("a challenge cannot be accepted or cancelled twice");
    }
    match &challenge.status {
        ChallengeStatus::Open => invalid("an update must cancel or accept the challenge"),
        ChallengeStatus::Cancelled => Ok(ValidateCallbackResult::Valid),
        ChallengeStatus::Accepted { opponent, game } => {
            if opponent == author {
                return invalid("you cannot accept your own challenge");
            }
            let Some(created) = record_game(&must_get_valid_record(game.clone())?)? else {
                return invalid("accepted challenge does not point at a game");
            };
            if created.challenge != *original || created.based_on.is_some() {
                return invalid("accepted game does not belong to this challenge");
            }
            let (white, black) = assign_colors(original, author, opponent, &challenge.color_preference);
            if created.white != white || created.black != black {
                return invalid("game colors do not match the challenge");
            }
            if &created.white != opponent && &created.black != opponent {
                return invalid("accepted opponent is not seated at the board");
            }
            Ok(ValidateCallbackResult::Valid)
        }
    }
}

fn validate_game_create(
    game: &Game,
    author: &AgentPubKey,
    prev_action: Option<ActionHash>,
) -> ExternResult<ValidateCallbackResult> {
    if game.based_on.is_some()
        || !game.moves.is_empty()
        || game.status != Status::InProgress
        || game.winner.is_some()
        || game.draw_offer_by.is_some()
        || game.end_reason.is_some()
    {
        return invalid("a new game must be the standard starting position");
    }
    if game.white == game.black {
        return invalid("the two players must be different agents");
    }
    if &game.white != author && &game.black != author {
        return invalid("the game author must be one of the players");
    }
    let Some(challenge) = record_challenge(&must_get_valid_record(game.challenge.clone())?)? else {
        return invalid("game does not point at a challenge");
    };
    if challenge.status != ChallengeStatus::Open || &challenge.challenger != author {
        return invalid("only the challenger can open the game, and only from an open challenge");
    }
    let opponent = if &game.white == author {
        &game.black
    } else {
        &game.white
    };
    let (white, black) = assign_colors(
        &game.challenge,
        author,
        opponent,
        &challenge.color_preference,
    );
    if game.white != white || game.black != black {
        return invalid("game colors do not match the challenge preference");
    }
    if chain_has_game_for_challenge(prev_action, &game.challenge)? {
        return invalid("a challenge can only create one game");
    }
    Ok(ValidateCallbackResult::Valid)
}

fn validate_game_update(
    game: &Game,
    author: &AgentPubKey,
    prev_action: Option<ActionHash>,
    original: &ActionHash,
) -> ExternResult<ValidateCallbackResult> {
    let Some(based_on) = &game.based_on else {
        return invalid("a game update must name the record it extends");
    };
    let Some(previous) = record_game(&must_get_valid_record(based_on.clone())?)? else {
        return invalid("game update does not extend a game record");
    };
    if previous.challenge != game.challenge
        || previous.white != game.white
        || previous.black != game.black
    {
        return invalid("players and challenge are fixed for the whole game");
    }
    let Some(root) = record_game(&must_get_valid_record(original.clone())?)? else {
        return invalid("game update does not belong to this game");
    };
    if root.challenge != game.challenge || root.based_on.is_some() {
        return invalid("game update original is not the game's first record");
    }
    let prev_rules = game_to_rules(&previous)?;
    let next_rules = game_to_rules(game)?;
    if transition_is_valid(&prev_rules, &next_rules).is_err() {
        return invalid("move, resignation, or draw does not follow chess rules");
    }
    let Some(actor) = prev_rules
        .actor()
        .map_err(|_| wasm_error!("rules error"))?
    else {
        return invalid("the game has already ended");
    };
    let Some(author_side) = side_of(&previous, author) else {
        return invalid("only a player in this game can update it");
    };
    if author_side != actor {
        return invalid("it is not this player's turn");
    }
    if chain_has_game_extension(prev_action, &game.challenge, based_on)? {
        return invalid("that position was already acted on");
    }
    Ok(ValidateCallbackResult::Valid)
}

fn validate_link(link: OpLink<LinkTypes>) -> ExternResult<ValidateCallbackResult> {
    match link {
        OpLink::CreateLink {
            link_type,
            action,
        } => {
            let author = action.author();
            match link_type {
                LinkTypes::AgentToProfile => {
                    let base: AgentPubKey = action
                        .base_address
                        .clone()
                        .try_into()
                        .map_err(|_| wasm_error!("profile link base is not an agent"))?;
                    if &base != author {
                        return invalid("a player cannot publish a profile link for someone else");
                    }
                    Ok(ValidateCallbackResult::Valid)
                }
                LinkTypes::OpenChallenge | LinkTypes::AllGames | LinkTypes::PlayerToGame | LinkTypes::ChallengeToGame => {
                    Ok(ValidateCallbackResult::Valid)
                }
            }
        }
        OpLink::DeleteLink { action, original_action, .. } => {
            if action.author() != original_action.author() {
                return invalid("only the agent who published a link can remove it");
            }
            Ok(ValidateCallbackResult::Valid)
        }
    }
}

pub fn assign_colors(
    challenge: &ActionHash,
    challenger: &AgentPubKey,
    opponent: &AgentPubKey,
    preference: &ColorPreference,
) -> (AgentPubKey, AgentPubKey) {
    match preference {
        ColorPreference::White => (challenger.clone(), opponent.clone()),
        ColorPreference::Black => (opponent.clone(), challenger.clone()),
        ColorPreference::Random => {
            let raw = challenge.get_raw_39();
            if raw.first().copied().unwrap_or(0) % 2 == 0 {
                (challenger.clone(), opponent.clone())
            } else {
                (opponent.clone(), challenger.clone())
            }
        }
    }
}

pub fn game_to_rules(game: &Game) -> ExternResult<RulesGame> {
    Ok(RulesGame {
        moves: game.moves.clone(),
        status: game.status,
        winner: match &game.winner {
            None => None,
            Some(agent) if agent == &game.white => Some(Side::White),
            Some(agent) if agent == &game.black => Some(Side::Black),
            Some(_) => {
                return Err(wasm_error!("winner is not seated at the board"));
            }
        },
        draw_offer_by: match &game.draw_offer_by {
            None => None,
            Some(agent) if agent == &game.white => Some(Side::White),
            Some(agent) if agent == &game.black => Some(Side::Black),
            Some(_) => {
                return Err(wasm_error!("draw offer is not from a player"));
            }
        },
        end_reason: game.end_reason,
    })
}

fn side_of(game: &Game, agent: &AgentPubKey) -> Option<Side> {
    if agent == &game.white {
        Some(Side::White)
    } else if agent == &game.black {
        Some(Side::Black)
    } else {
        None
    }
}

fn entry_index(record: &Record) -> Option<u8> {
    record
        .action()
        .app_entry_def()
        .map(|def| def.entry_index.0)
}

fn record_profile(record: &Record) -> ExternResult<Option<Profile>> {
    if entry_index(record) != Some(0) {
        return Ok(None);
    }
    record
        .entry()
        .to_app_option::<Profile>()
        .map_err(|error| wasm_error!("{}", error))
}

fn record_challenge(record: &Record) -> ExternResult<Option<Challenge>> {
    if entry_index(record) != Some(1) {
        return Ok(None);
    }
    record
        .entry()
        .to_app_option::<Challenge>()
        .map_err(|error| wasm_error!("{}", error))
}

fn record_game(record: &Record) -> ExternResult<Option<Game>> {
    if entry_index(record) != Some(2) {
        return Ok(None);
    }
    record
        .entry()
        .to_app_option::<Game>()
        .map_err(|error| wasm_error!("{}", error))
}

fn chain_has_game_extension(
    prev: Option<ActionHash>,
    challenge: &ActionHash,
    based_on: &ActionHash,
) -> ExternResult<bool> {
    walk_chain(prev, |game: &Game| {
        game.challenge == *challenge && game.based_on.as_ref() == Some(based_on)
    })
}

fn chain_has_game_for_challenge(prev: Option<ActionHash>, challenge: &ActionHash) -> ExternResult<bool> {
    walk_chain(prev, |game: &Game| game.challenge == *challenge)
}

fn chain_has_challenge_extension(prev: Option<ActionHash>, original: &ActionHash) -> ExternResult<bool> {
    walk_any(prev, |record| {
        Ok(record_challenge(record)?
            .map(|challenge| challenge.based_on.as_ref() == Some(original))
            .unwrap_or(false))
    })
}

fn walk_chain(prev: Option<ActionHash>, pred: impl Fn(&Game) -> bool) -> ExternResult<bool> {
    walk_any(prev, |record| {
        Ok(record_game(record)?.map(|game| pred(&game)).unwrap_or(false))
    })
}

fn walk_any(
    mut prev: Option<ActionHash>,
    pred: impl Fn(&Record) -> ExternResult<bool>,
) -> ExternResult<bool> {
    let mut steps = 0u32;
    while let Some(hash) = prev {
        steps += 1;
        if steps > 2000 {
            return Err(wasm_error!("source chain walk exceeded 2000 actions"));
        }
        let record = must_get_valid_record(hash)?;
        if pred(&record)? {
            return Ok(true);
        }
        prev = record.action().prev_action().cloned();
    }
    Ok(false)
}

fn invalid(reason: &str) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Invalid(reason.into()))
}
