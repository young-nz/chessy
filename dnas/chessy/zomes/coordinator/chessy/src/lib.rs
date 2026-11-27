//! Coordinator zome: profiles, lobby, games, and signals.
//!
//! Chess legality is decided by `chess_rules` and re-checked by the integrity
//! zome. This zome is the API clients and the agent runner call.

use chess_rules::{view, PlayerAction, RulesGame, Side, Status};
use chessy_integrity::*;
use hdk::prelude::*;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProfileInput {
    pub nickname: String,
    pub agent_name: Option<String>,
    pub model_label: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProfileView {
    pub action_hash: ActionHash,
    pub agent: AgentPubKey,
    pub nickname: String,
    pub agent_name: Option<String>,
    pub model_label: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChallengeView {
    pub action_hash: ActionHash,
    pub challenger: AgentPubKey,
    pub nickname: Option<String>,
    pub color_preference: ColorPreference,
    pub status: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GameView {
    pub action_hash: ActionHash,
    pub original_hash: ActionHash,
    pub challenge: ActionHash,
    pub white: AgentPubKey,
    pub black: AgentPubKey,
    pub white_nickname: Option<String>,
    pub black_nickname: Option<String>,
    pub moves: Vec<String>,
    pub fen: String,
    pub legal_moves: Vec<String>,
    pub side_to_move: String,
    pub actor: Option<String>,
    pub status: Status,
    pub winner: Option<AgentPubKey>,
    pub draw_offer_by: Option<AgentPubKey>,
    pub end_reason: Option<chess_rules::EndReason>,
    pub my_turn: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FinalizeAccept {
    pub challenge: ActionHash,
    pub opponent: AgentPubKey,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MoveInput {
    pub game: ActionHash,
    pub uci: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChessySignal {
    ChallengePosted {
        challenge: ActionHash,
        challenger: AgentPubKey,
    },
    ChallengeCancelled {
        challenge: ActionHash,
    },
    ChallengeAccepted {
        challenge: ActionHash,
        game: ActionHash,
        opponent: AgentPubKey,
    },
    MoveMade {
        game: ActionHash,
        uci: String,
        fen: String,
        status: Status,
    },
    GameEnded {
        game: ActionHash,
        status: Status,
        winner: Option<AgentPubKey>,
        reason: chess_rules::EndReason,
    },
    DrawOffered {
        game: ActionHash,
        by: AgentPubKey,
    },
    DrawDeclined {
        game: ActionHash,
    },
}

#[hdk_extern]
pub fn init(_: ()) -> ExternResult<InitCallbackResult> {
    let zome_name = zome_info()?.name;
    let mut functions = HashSet::new();
    functions.insert((zome_name, "finalize_accept".into()));
    create_cap_grant(CapGrantEntry::new_zome_call_grant(
        "chessy-remote".into(),
        GrantConstraint::Unrestricted,
        GrantedFunctions::Listed(functions),
    ))?;
    create_cap_grant(CapGrantEntry::new_direct_signal_grant(
        "chessy-signals".into(),
        GrantConstraint::Unrestricted,
    ))?;
    Ok(InitCallbackResult::Pass)
}

#[hdk_extern]
pub fn ping(_: ()) -> ExternResult<String> {
    Ok("chessy".into())
}

#[hdk_extern]
pub fn whoami(_: ()) -> ExternResult<AgentPubKey> {
    me()
}

#[hdk_extern]
pub fn recv_remote_signal(signal: ChessySignal) -> ExternResult<()> {
    emit_signal(signal)
}

#[hdk_extern]
pub fn create_profile(input: ProfileInput) -> ExternResult<ActionHash> {
    let agent = me()?;
    if profile_root(&agent)?.is_some() {
        return update_profile(input);
    }
    let profile = profile_from_input(input)?;
    let hash = create_entry(EntryTypes::Profile(profile))?;
    create_link(agent, hash.clone(), LinkTypes::AgentToProfile, ())?;
    Ok(hash)
}

#[hdk_extern]
pub fn update_profile(input: ProfileInput) -> ExternResult<ActionHash> {
    let agent = me()?;
    let root = profile_root(&agent)?.ok_or_else(|| wasm_error!("create a profile first"))?;
    let profile = profile_from_input(input)?;
    update_entry(root, EntryTypes::Profile(profile))
}

#[hdk_extern]
pub fn get_my_profile(_: ()) -> ExternResult<Option<ProfileView>> {
    get_profile(me()?)
}

#[hdk_extern]
pub fn get_profile(agent: AgentPubKey) -> ExternResult<Option<ProfileView>> {
    let Some(root) = profile_root(&agent)? else {
        return Ok(None);
    };
    let record = latest_by_timestamp(root)?;
    let Some(profile) = optional_profile(&record)? else {
        return Ok(None);
    };
    Ok(Some(ProfileView {
        action_hash: record.action_address().clone(),
        agent,
        nickname: profile.nickname,
        agent_name: profile.agent_name,
        model_label: profile.model_label,
    }))
}

#[hdk_extern]
pub fn create_challenge(color_preference: ColorPreference) -> ExternResult<ActionHash> {
    let challenger = me()?;
    let challenge = Challenge {
        challenger: challenger.clone(),
        color_preference,
        based_on: None,
        status: ChallengeStatus::Open,
    };
    let hash = create_entry(EntryTypes::Challenge(challenge))?;
    create_link(anchor("open_challenges")?, hash.clone(), LinkTypes::OpenChallenge, ())?;
    emit_signal(ChessySignal::ChallengePosted {
        challenge: hash.clone(),
        challenger,
    })?;
    Ok(hash)
}

#[hdk_extern]
pub fn cancel_challenge(challenge_hash: ActionHash) -> ExternResult<ActionHash> {
    let me = me()?;
    let (root, current) = load_challenge(challenge_hash)?;
    if current.challenger != me {
        return Err(wasm_error!("only the challenger can cancel this challenge"));
    }
    if current.status != ChallengeStatus::Open {
        return Err(wasm_error!("this challenge is no longer open"));
    }
    let cancelled = Challenge {
        based_on: Some(root.clone()),
        status: ChallengeStatus::Cancelled,
        ..current
    };
    let hash = update_entry(root.clone(), EntryTypes::Challenge(cancelled))?;
    delete_links_to(&anchor("open_challenges")?, LinkTypes::OpenChallenge, &root)?;
    emit_signal(ChessySignal::ChallengeCancelled { challenge: root })?;
    Ok(hash)
}

#[hdk_extern]
pub fn accept_challenge(challenge_hash: ActionHash) -> ExternResult<ActionHash> {
    let me = me()?;
    let (root, current) = load_challenge(challenge_hash)?;
    if current.challenger == me {
        return Err(wasm_error!("you cannot accept your own challenge"));
    }
    if current.status != ChallengeStatus::Open {
        return Err(wasm_error!("this challenge is no longer open"));
    }
    let response = call_remote(
        current.challenger.clone(),
        zome_info()?.name,
        "finalize_accept".into(),
        None,
        FinalizeAccept {
            challenge: root,
            opponent: me,
        },
    )?;
    match response {
        ZomeCallResponse::Ok(bytes) => bytes
            .decode()
            .map_err(|error| wasm_error!("could not read the new game: {}", error)),
        ZomeCallResponse::Unauthorized(_, _, _, _) => {
            Err(wasm_error!("the challenger has not granted remote accepts"))
        }
        ZomeCallResponse::NetworkError(message) => {
            Err(wasm_error!("could not reach the challenger: {}", message))
        }
        other => Err(wasm_error!("accept failed: {:?}", other)),
    }
}

/// Called on the challenger's conductor by the opponent. Source-chain order
/// makes two accepts serialize: the second sees the challenge is no longer open.
#[hdk_extern]
pub fn finalize_accept(input: FinalizeAccept) -> ExternResult<ActionHash> {
    let caller = call_info()?.provenance;
    if caller != input.opponent {
        return Err(wasm_error!("accept caller does not match the claimed opponent"));
    }
    let me = me()?;
    if input.opponent == me {
        return Err(wasm_error!("you cannot accept your own challenge"));
    }
    let (root, current) = load_challenge(input.challenge)?;
    if current.challenger != me {
        return Err(wasm_error!("this conductor did not post that challenge"));
    }
    if current.status != ChallengeStatus::Open {
        return Err(wasm_error!("this challenge is no longer open"));
    }
    let (white, black) = assign_colors(
        &root,
        &me,
        &input.opponent,
        &current.color_preference,
    );
    let game = Game {
        challenge: root.clone(),
        white: white.clone(),
        black: black.clone(),
        moves: Vec::new(),
        status: Status::InProgress,
        winner: None,
        draw_offer_by: None,
        end_reason: None,
        based_on: None,
    };
    let game_hash = create_entry(EntryTypes::Game(game))?;
    let accepted = Challenge {
        based_on: Some(root.clone()),
        status: ChallengeStatus::Accepted {
            opponent: input.opponent.clone(),
            game: game_hash.clone(),
        },
        ..current
    };
    update_entry(root.clone(), EntryTypes::Challenge(accepted))?;
    create_link(anchor("all_games")?, game_hash.clone(), LinkTypes::AllGames, ())?;
    create_link(white, game_hash.clone(), LinkTypes::PlayerToGame, ())?;
    create_link(black, game_hash.clone(), LinkTypes::PlayerToGame, ())?;
    create_link(root.clone(), game_hash.clone(), LinkTypes::ChallengeToGame, ())?;
    delete_links_to(&anchor("open_challenges")?, LinkTypes::OpenChallenge, &root)?;
    let signal = ChessySignal::ChallengeAccepted {
        challenge: root,
        game: game_hash.clone(),
        opponent: input.opponent.clone(),
    };
    emit_signal(signal.clone())?;
    let _ = send_remote_signal(signal, vec![input.opponent]);
    Ok(game_hash)
}

#[hdk_extern]
pub fn list_open_challenges(_: ()) -> ExternResult<Vec<ChallengeView>> {
    let links = get_links(
        LinkQuery::try_new(anchor("open_challenges")?, LinkTypes::OpenChallenge)?,
        GetStrategy::Network,
    )?;
    let mut views = Vec::new();
    for link in links {
        let Ok(hash) = ActionHash::try_from(link.target) else {
            continue;
        };
        let Ok((root, challenge)) = load_challenge(hash) else {
            continue;
        };
        if challenge.status != ChallengeStatus::Open {
            continue;
        }
        let nickname = get_profile(challenge.challenger.clone())?
            .map(|profile| profile.nickname);
        views.push(ChallengeView {
            action_hash: root,
            challenger: challenge.challenger,
            nickname,
            color_preference: challenge.color_preference,
            status: "open".into(),
        });
    }
    Ok(views)
}

#[hdk_extern]
pub fn make_move(input: MoveInput) -> ExternResult<ActionHash> {
    commit_game_action(input.game, PlayerAction::Move(input.uci))
}

#[hdk_extern]
pub fn resign(game: ActionHash) -> ExternResult<ActionHash> {
    let side = my_side(&game)?;
    commit_game_action(game, PlayerAction::Resign(side))
}

#[hdk_extern]
pub fn offer_draw(game: ActionHash) -> ExternResult<ActionHash> {
    let side = my_side(&game)?;
    commit_game_action(game, PlayerAction::OfferDraw(side))
}

#[hdk_extern]
pub fn accept_draw(game: ActionHash) -> ExternResult<ActionHash> {
    let side = my_side(&game)?;
    commit_game_action(game, PlayerAction::AcceptDraw(side))
}

#[hdk_extern]
pub fn decline_draw(game: ActionHash) -> ExternResult<ActionHash> {
    let side = my_side(&game)?;
    commit_game_action(game, PlayerAction::DeclineDraw(side))
}

#[hdk_extern]
pub fn get_game_state(game: ActionHash) -> ExternResult<GameView> {
    let (root, tip, record) = load_game(game)?;
    game_view(root, tip, &record)
}

#[hdk_extern]
pub fn list_my_games(_: ()) -> ExternResult<Vec<GameView>> {
    let links = get_links(
        LinkQuery::try_new(me()?, LinkTypes::PlayerToGame)?,
        GetStrategy::Network,
    )?;
    games_from_links(links)
}

#[hdk_extern]
pub fn list_games(_: ()) -> ExternResult<Vec<GameView>> {
    let links = get_links(
        LinkQuery::try_new(anchor("all_games")?, LinkTypes::AllGames)?,
        GetStrategy::Network,
    )?;
    games_from_links(links)
}

fn games_from_links(links: Vec<Link>) -> ExternResult<Vec<GameView>> {
    let mut games = Vec::new();
    for link in links {
        let Ok(hash) = ActionHash::try_from(link.target) else {
            continue;
        };
        if let Ok((root, tip, game)) = load_game(hash) {
            games.push(game_view(root, tip, &game)?);
        }
    }
    Ok(games)
}

fn commit_game_action(game_hash: ActionHash, action: PlayerAction) -> ExternResult<ActionHash> {
    let (root, tip, game) = load_game(game_hash.clone())?;
    if game_hash != tip {
        return Err(wasm_error!("stale game; refresh and try again"));
    }
    let me = me()?;
    let rules = game_to_rules(&game)?;
    let actor = rules
        .actor()
        .map_err(|error| wasm_error!("{}", error))?
        .ok_or_else(|| wasm_error!("the game has already ended"))?;
    if side_of(&game, &me) != Some(actor) {
        return Err(wasm_error!("it is not your turn"));
    }
    let previous_status = game.status;
    let next_rules = chess_rules::apply(&rules, action.clone()).map_err(|error| wasm_error!("{}", error))?;
    let uci = match &action {
        PlayerAction::Move(uci) => Some(uci.clone()),
        _ => None,
    };
    let next = game_from_rules(&game, next_rules, Some(tip))?;
    let hash = update_entry(root, EntryTypes::Game(next.clone()))?;
    let rendered = view(&game_to_rules(&next)?).map_err(|error| wasm_error!("{}", error))?;
    let opponent = if me == next.white {
        next.black.clone()
    } else {
        next.white.clone()
    };
    let signal = match next.status {
        Status::InProgress if previous_status == Status::DrawOffered => {
            ChessySignal::DrawDeclined { game: hash.clone() }
        }
        Status::DrawOffered => ChessySignal::DrawOffered {
            game: hash.clone(),
            by: me,
        },
        status if status.is_terminal() => ChessySignal::GameEnded {
            game: hash.clone(),
            status,
            winner: next.winner.clone(),
            reason: next
                .end_reason
                .ok_or_else(|| wasm_error!("ended game is missing a reason"))?,
        },
        status => ChessySignal::MoveMade {
            game: hash.clone(),
            uci: uci.unwrap_or_default(),
            fen: rendered.fen,
            status,
        },
    };
    emit_signal(signal.clone())?;
    let _ = send_remote_signal(signal, vec![opponent]);
    Ok(hash)
}

fn game_view(root: ActionHash, tip: ActionHash, game: &Game) -> ExternResult<GameView> {
    let rules = game_to_rules(game)?;
    let rendered = view(&rules).map_err(|error| wasm_error!("{}", error))?;
    let actor = rules.actor().map_err(|error| wasm_error!("{}", error))?;
    let me = me()?;
    Ok(GameView {
        action_hash: tip,
        original_hash: root,
        challenge: game.challenge.clone(),
        white: game.white.clone(),
        black: game.black.clone(),
        white_nickname: nickname_of(&game.white)?,
        black_nickname: nickname_of(&game.black)?,
        moves: game.moves.clone(),
        fen: rendered.fen,
        legal_moves: rendered.legal_moves,
        side_to_move: rendered.side_to_move.as_str().into(),
        actor: actor.map(|side| side.as_str().into()),
        status: game.status,
        winner: game.winner.clone(),
        draw_offer_by: game.draw_offer_by.clone(),
        end_reason: game.end_reason,
        my_turn: side_of(game, &me) == actor,
    })
}

fn game_from_rules(template: &Game, rules: RulesGame, based_on: Option<ActionHash>) -> ExternResult<Game> {
    Ok(Game {
        challenge: template.challenge.clone(),
        white: template.white.clone(),
        black: template.black.clone(),
        moves: rules.moves,
        status: rules.status,
        winner: rules.winner.map(|side| seat(template, side)),
        draw_offer_by: rules.draw_offer_by.map(|side| seat(template, side)),
        end_reason: rules.end_reason,
        based_on,
    })
}

fn seat(game: &Game, side: Side) -> AgentPubKey {
    match side {
        Side::White => game.white.clone(),
        Side::Black => game.black.clone(),
    }
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

fn my_side(game_hash: &ActionHash) -> ExternResult<Side> {
    let (_, _, game) = load_game(game_hash.clone())?;
    side_of(&game, &me()?).ok_or_else(|| wasm_error!("you are not a player in this game"))
}

fn load_game(hash: ActionHash) -> ExternResult<(ActionHash, ActionHash, Game)> {
    let record = get(hash.clone(), GetOptions::default())?
        .ok_or_else(|| wasm_error!("game not found"))?;
    let game = require_game(&record)?;
    let root = root_of_game(hash, &game)?;
    let (tip, tip_game) = latest_game(root.clone())?;
    Ok((root, tip, tip_game))
}

fn root_of_game(start: ActionHash, game: &Game) -> ExternResult<ActionHash> {
    let mut cursor = start;
    let mut current = game.clone();
    for _ in 0..600 {
        let Some(parent) = current.based_on.clone() else {
            return Ok(cursor);
        };
        let record = get(parent.clone(), GetOptions::default())?
            .ok_or_else(|| wasm_error!("missing earlier game record"))?;
        current = require_game(&record)?;
        cursor = parent;
    }
    Err(wasm_error!("game history is too deep to walk"))
}

fn latest_game(root: ActionHash) -> ExternResult<(ActionHash, Game)> {
    let Details::Record(details) = get_details(root.clone(), GetOptions::default())?
        .ok_or_else(|| wasm_error!("game not found"))?
    else {
        return Err(wasm_error!("expected game record details"));
    };
    let mut current_hash = root;
    let mut current = require_game(&details.record)?;
    let mut children = Vec::new();
    for update in details.updates {
        let hash = update.action_address().clone();
        let Some(record) = get(hash.clone(), GetOptions::default())? else {
            continue;
        };
        let Ok(game) = require_game(&record) else {
            continue;
        };
        if let Some(parent) = game.based_on.clone() {
            children.push((parent, hash, game));
        }
    }
    for _ in 0..600 {
        let matches: Vec<_> = children
            .iter()
            .filter(|(parent, _, _)| parent == &current_hash)
            .collect();
        if matches.len() > 1 {
            return Err(wasm_error!("this game has a forked history"));
        }
        let Some((_, hash, game)) = matches.first() else {
            break;
        };
        current_hash = (*hash).clone();
        current = (*game).clone();
    }
    Ok((current_hash, current))
}

fn load_challenge(hash: ActionHash) -> ExternResult<(ActionHash, Challenge)> {
    let record = get(hash.clone(), GetOptions::default())?
        .ok_or_else(|| wasm_error!("challenge not found"))?;
    let challenge = require_challenge(&record)?;
    let root = challenge.based_on.clone().unwrap_or(hash);
    let Details::Record(details) = get_details(root.clone(), GetOptions::default())?
        .ok_or_else(|| wasm_error!("challenge not found"))?
    else {
        return Err(wasm_error!("expected challenge record details"));
    };
    if details.updates.len() > 1 {
        return Err(wasm_error!("this challenge was updated more than once"));
    }
    if let Some(update) = details.updates.first() {
        let record = get(update.action_address().clone(), GetOptions::default())?
            .ok_or_else(|| wasm_error!("challenge update is missing"))?;
        return Ok((root, require_challenge(&record)?));
    }
    Ok((root, require_challenge(&details.record)?))
}

fn latest_by_timestamp(root: ActionHash) -> ExternResult<Record> {
    let Details::Record(details) = get_details(root, GetOptions::default())?
        .ok_or_else(|| wasm_error!("record not found"))?
    else {
        return Err(wasm_error!("expected record details"));
    };
    let mut best: Option<(Timestamp, ActionHash)> = None;
    for update in details.updates {
        let stamp = update.action().timestamp();
        let hash = update.action_address().clone();
        if best.as_ref().map(|(at, _)| stamp > *at).unwrap_or(true) {
            best = Some((stamp, hash));
        }
    }
    if let Some((_, hash)) = best {
        return get(hash, GetOptions::default())?.ok_or_else(|| wasm_error!("update record missing"));
    }
    Ok(details.record)
}

fn profile_root(agent: &AgentPubKey) -> ExternResult<Option<ActionHash>> {
    let links = get_links(
        LinkQuery::try_new(agent.clone(), LinkTypes::AgentToProfile)?,
        GetStrategy::Network,
    )?;
    let Some(link) = links.into_iter().max_by_key(|link| link.timestamp) else {
        return Ok(None);
    };
    let hash = ActionHash::try_from(link.target).map_err(|_| wasm_error!("profile link is not an action"))?;
    let record = get(hash, GetOptions::default())?.ok_or_else(|| wasm_error!("profile record missing"))?;
    let action = record.action();
    let root = match &action.data {
        ActionData::Update(update) => update.original_action_address.clone(),
        _ => record.action_address().clone(),
    };
    Ok(Some(root))
}

fn profile_from_input(input: ProfileInput) -> ExternResult<Profile> {
    let profile = Profile {
        nickname: input.nickname,
        agent_name: input.agent_name,
        model_label: input.model_label,
    };
    check_profile(&profile).map_err(|reason| wasm_error!("{}", reason))?;
    Ok(profile)
}

fn nickname_of(agent: &AgentPubKey) -> ExternResult<Option<String>> {
    Ok(get_profile(agent.clone())?.map(|profile| profile.nickname))
}

fn me() -> ExternResult<AgentPubKey> {
    Ok(agent_info()?.agent_initial_pubkey)
}

fn anchor(label: &str) -> ExternResult<EntryHash> {
    Path::from(label).path_entry_hash()
}

fn delete_links_to(base: &EntryHash, link_type: LinkTypes, target: &ActionHash) -> ExternResult<()> {
    let links = get_links(LinkQuery::try_new(base.clone(), link_type)?, GetStrategy::Network)?;
    for link in links {
        let Ok(hash) = ActionHash::try_from(link.target) else {
            continue;
        };
        if &hash == target {
            delete_link(link.create_link_hash, GetOptions::default())?;
        }
    }
    Ok(())
}

fn require_game(record: &Record) -> ExternResult<Game> {
    record
        .entry()
        .to_app_option::<Game>()
        .map_err(|error| wasm_error!("{}", error))?
        .ok_or_else(|| wasm_error!("record is not a game"))
}

fn require_challenge(record: &Record) -> ExternResult<Challenge> {
    record
        .entry()
        .to_app_option::<Challenge>()
        .map_err(|error| wasm_error!("{}", error))?
        .ok_or_else(|| wasm_error!("record is not a challenge"))
}

fn optional_profile(record: &Record) -> ExternResult<Option<Profile>> {
    record
        .entry()
        .to_app_option::<Profile>()
        .map_err(|error| wasm_error!("{}", error))
}
