//! M45: explicit combat actions, per-owner vitals and server-filtered logs.
use crate::{
    auth::now,
    campaigns::{identifier, text, timestamp},
    error::{ApiError, Result},
    vtt::{BoardToken, access, board_token, gm, save_token},
};
use axum::http::StatusCode;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Combatant {
    pub token_id: String,
    pub initiative: Option<i32>,
    pub secret: bool,
}
#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Condition {
    pub id: String,
    pub token_id: String,
    pub name: String,
    pub rounds: Option<u32>,
    pub expired: bool,
}
#[derive(Clone, Default, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CombatState {
    pub running: bool,
    pub scene_id: Option<String>,
    pub round: u32,
    pub current: Option<String>,
    pub order: Vec<Combatant>,
    pub conditions: Vec<Condition>,
}
#[derive(Deserialize, Serialize, ToSchema)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CombatAction {
    Start {
        scene_id: String,
        token_ids: Vec<String>,
    },
    End,
    Initiative {
        token_id: String,
        value: i32,
        roll: bool,
        secret: bool,
    },
    Order {
        token_ids: Vec<String>,
    },
    Next,
    Delay,
    Add {
        token_id: String,
    },
    Remove {
        token_id: String,
    },
    SetVitals {
        token_id: String,
        hp: i32,
        max_hp: i32,
        ac: Option<i32>,
        show_hp: bool,
        gm_note: Option<String>,
        status: Option<String>,
    },
    SetCondition {
        condition: Condition,
    },
    RemoveCondition {
        condition_id: String,
    },
    Roll {
        token_id: String,
        count: u32,
        sides: u32,
        modifier: i32,
        secret: bool,
        label: String,
    },
}
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VitalsView {
    pub token_id: String,
    pub show_hp: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hp: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_hp: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ac: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gm_note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}
#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CombatLog {
    pub id: i64,
    pub kind: String,
    pub token_id: Option<String>,
    pub amount: Option<i32>,
    pub dice: Vec<u32>,
    pub label: String,
    pub created_at: String,
}
pub(crate) fn state(conn: &Connection, table: &str) -> Result<CombatState> {
    let data: Option<String> = conn
        .query_row(
            "SELECT data FROM table_combats WHERE table_id=?1",
            [table],
            |r| r.get(0),
        )
        .optional()?;
    data.map(|s| serde_json::from_str(&s).map_err(|_| ApiError::internal()))
        .unwrap_or(Ok(CombatState::default()))
}
fn store(conn: &Connection, table: &str, state: &CombatState) -> Result<()> {
    conn.execute("INSERT INTO table_combats(table_id,data) VALUES(?1,?2) ON CONFLICT(table_id) DO UPDATE SET data=excluded.data",params![table,serde_json::to_string(state).map_err(|_|ApiError::internal())?])?;
    Ok(())
}
pub(crate) fn log(
    conn: &Connection,
    table: &str,
    token: Option<&BoardToken>,
    audience: &str,
    details: (&str, Option<i32>, Vec<u32>, &str),
) -> Result<()> {
    let (kind, amount, dice, label) = details;
    let visibility = if audience == "public" && token.is_some_and(|t| t.hidden) {
        "gm"
    } else {
        audience
    };
    let event = CombatLog {
        id: 0,
        kind: kind.into(),
        token_id: token.map(|t| t.id.clone()),
        amount,
        dice,
        label: label.into(),
        created_at: timestamp(now()),
    };
    conn.execute("INSERT INTO table_combat_log(table_id,token_id,scene_id,audience,owner_id,data) VALUES(?1,?2,?3,?4,?5,?6)",params![table,event.token_id,token.map(|t|t.scene_id.clone()).or(state(conn,table)?.scene_id),visibility,token.map(|t|&t.owner_id),serde_json::to_string(&event).map_err(|_|ApiError::internal())?])?;
    conn.execute("DELETE FROM table_combat_log WHERE table_id=?1 AND id NOT IN(SELECT id FROM table_combat_log WHERE table_id=?1 ORDER BY id DESC LIMIT 1000)",[table])?;
    Ok(())
}
fn own(conn: &Connection, table: &str, account: &str, id: &str) -> Result<BoardToken> {
    let t = board_token(conn, table, id)?;
    if t.owner_id != account {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "Only the token owner may change this value.",
        ));
    }
    access(conn, table, &t.owner_id)?;
    Ok(t)
}
fn dice(count: u32, sides: u32) -> Result<Vec<u32>> {
    if !(1..=20).contains(&count) || !(2..=1000).contains(&sides) {
        return Err(ApiError::invalid());
    }
    let bound = u32::MAX - u32::MAX % sides;
    let mut values = Vec::new();
    for _ in 0..count {
        loop {
            let mut bytes = [0; 4];
            getrandom::fill(&mut bytes).map_err(|_| ApiError::internal())?;
            let n = u32::from_le_bytes(bytes);
            if n < bound {
                values.push(n % sides + 1);
                break;
            }
        }
    }
    Ok(values)
}
fn round_start(s: &mut CombatState) {
    s.round = s.round.saturating_add(1);
    for c in &mut s.conditions {
        if let Some(rounds) = &mut c.rounds {
            *rounds = rounds.saturating_sub(1);
            c.expired = *rounds == 0;
        }
    }
}
fn reset_movement(conn: &Connection, table: &str, s: &CombatState) -> Result<()> {
    if let Some(id) = &s.current {
        let mut t = board_token(conn, table, id)?;
        t.spent = 0.0;
        t.diagonals = 0;
        save_token(conn, table, &t)?;
    }
    Ok(())
}
pub(crate) fn movement_allowed(
    conn: &Connection,
    table: &str,
    token: &BoardToken,
    free: bool,
) -> Result<bool> {
    let s = state(conn, table)?;
    if s.running {
        Ok(s.scene_id.as_deref() == Some(&token.scene_id)
            && s.current.as_deref() == Some(&token.id))
    } else {
        Ok(free)
    }
}
pub(crate) fn apply(
    conn: &Connection,
    table: &str,
    account: &str,
    role: &str,
    action: &CombatAction,
) -> Result<()> {
    let mut s = state(conn, table)?;
    match action {
        CombatAction::Start {
            scene_id,
            token_ids,
        } => {
            gm(role)?;
            identifier(scene_id)?;
            if s.running {
                return Err(ApiError(
                    StatusCode::CONFLICT,
                    "End the current combat first.",
                ));
            }
            if token_ids.is_empty() || token_ids.len() > 200 {
                return Err(ApiError::invalid());
            }
            let mut order = Vec::new();
            for id in token_ids {
                let t = board_token(conn, table, id)?;
                if &t.scene_id != scene_id || order.iter().any(|v: &Combatant| &v.token_id == id) {
                    return Err(ApiError::invalid());
                }
                order.push(Combatant {
                    token_id: id.clone(),
                    initiative: None,
                    secret: false,
                });
            }
            s = CombatState {
                running: true,
                scene_id: Some(scene_id.clone()),
                round: 0,
                current: None,
                order,
                conditions: s.conditions,
            };
            store(conn, table, &s)?;
            log(conn, table, None, "public", ("start", None, vec![], ""))?;
        }
        CombatAction::End => {
            gm(role)?;
            s.running = false;
            s.current = None;
            s.order.clear();
            log(conn, table, None, "public", ("end", None, vec![], ""))?;
        }
        CombatAction::Initiative {
            token_id,
            value,
            roll,
            secret,
        } => {
            if !s.running || !(-1000..=1000).contains(value) {
                return Err(ApiError::invalid());
            }
            let t = own(conn, table, account, token_id)?;
            if *secret {
                gm(role)?;
            }
            let entry = s
                .order
                .iter_mut()
                .find(|v| v.token_id == *token_id)
                .ok_or_else(ApiError::invalid)?;
            let dice = if *roll { dice(1, 20)? } else { vec![] };
            let result = *value + dice.iter().sum::<u32>() as i32;
            entry.initiative = Some(result);
            entry.secret = *secret;
            // Stable sort keeps the GM's chosen tie order; changing it is explicit.
            s.order.sort_by(|a, b| {
                b.initiative
                    .unwrap_or(i32::MIN)
                    .cmp(&a.initiative.unwrap_or(i32::MIN))
            });
            log(
                conn,
                table,
                Some(&t),
                if *secret { "gm" } else { "public" },
                ("initiative", Some(result), dice, ""),
            )?;
        }
        CombatAction::Order { token_ids } => {
            gm(role)?;
            if !s.running || token_ids.len() != s.order.len() {
                return Err(ApiError::invalid());
            }
            let mut order = Vec::new();
            for id in token_ids {
                if order.iter().any(|v: &Combatant| &v.token_id == id) {
                    return Err(ApiError::invalid());
                }
                order.push(
                    s.order
                        .iter()
                        .find(|v| &v.token_id == id)
                        .ok_or_else(ApiError::invalid)?
                        .clone(),
                );
            }
            s.order = order;
        }
        CombatAction::Next | CombatAction::Delay => {
            gm(role)?;
            if !s.running || s.order.is_empty() {
                return Err(ApiError::invalid());
            }
            if s.round == 0 && matches!(action, CombatAction::Next) {
                s.round = 1;
                s.current = Some(s.order[0].token_id.clone());
                reset_movement(conn, table, &s)?;
                let t = board_token(conn, table, s.current.as_deref().unwrap())?;
                log(
                    conn,
                    table,
                    Some(&t),
                    "public",
                    ("turn", Some(1), vec![], ""),
                )?;
                return store(conn, table, &s);
            }
            let index = s
                .order
                .iter()
                .position(|v| Some(&v.token_id) == s.current.as_ref())
                .ok_or_else(ApiError::invalid)?;
            let delayed = matches!(action, CombatAction::Delay);
            if delayed && s.order.len() > 1 {
                let entry = s.order.remove(index);
                let next = if index < s.order.len() {
                    index
                } else {
                    round_start(&mut s);
                    0
                };
                s.current = Some(s.order[next].token_id.clone());
                s.order.push(entry);
            } else {
                let next = if index + 1 < s.order.len() {
                    index + 1
                } else {
                    round_start(&mut s);
                    0
                };
                s.current = Some(s.order[next].token_id.clone());
            }
            reset_movement(conn, table, &s)?;
            let t = board_token(conn, table, s.current.as_deref().unwrap())?;
            log(
                conn,
                table,
                Some(&t),
                "public",
                (
                    if delayed { "delay" } else { "turn" },
                    Some(s.round as i32),
                    vec![],
                    "",
                ),
            )?;
        }
        CombatAction::Add { token_id } => {
            gm(role)?;
            let t = board_token(conn, table, token_id)?;
            if !s.running
                || Some(&t.scene_id) != s.scene_id.as_ref()
                || s.order.len() >= 200
                || s.order.iter().any(|v| &v.token_id == token_id)
            {
                return Err(ApiError::invalid());
            }
            s.order.push(Combatant {
                token_id: token_id.clone(),
                initiative: None,
                secret: false,
            });
            log(conn, table, Some(&t), "public", ("add", None, vec![], ""))?;
        }
        CombatAction::Remove { token_id } => {
            gm(role)?;
            if !s.running {
                return Err(ApiError::invalid());
            }
            let index = s
                .order
                .iter()
                .position(|v| &v.token_id == token_id)
                .ok_or_else(ApiError::invalid)?;
            let t = board_token(conn, table, token_id)?;
            let current = s.current.as_deref() == Some(token_id);
            s.order.remove(index);
            if s.order.is_empty() {
                s.running = false;
                s.current = None;
            } else if current {
                let next = if index < s.order.len() {
                    index
                } else {
                    round_start(&mut s);
                    0
                };
                s.current = Some(s.order[next].token_id.clone());
                reset_movement(conn, table, &s)?;
            }
            log(
                conn,
                table,
                Some(&t),
                "public",
                ("remove", None, vec![], ""),
            )?;
        }
        CombatAction::SetVitals {
            token_id,
            hp,
            max_hp,
            ac,
            show_hp,
            gm_note,
            status,
        } => {
            let t = own(conn, table, account, token_id)?;
            if !(0..=1000000).contains(max_hp)
                || !(-1000000..=*max_hp).contains(hp)
                || ac.is_some_and(|n| !(-1000..=1000).contains(&n))
            {
                return Err(ApiError::invalid());
            }
            if let Some(note) = gm_note {
                gm(role)?;
                text(note, 0, 4000)?;
            }
            if let Some(status) = status {
                text(status, 0, 80)?;
            }
            let old: Option<i32> = conn
                .query_row(
                    "SELECT hp FROM table_token_vitals WHERE token_id=?1",
                    [token_id],
                    |r| r.get(0),
                )
                .optional()?;
            conn.execute("INSERT INTO table_token_vitals(token_id,table_id,hp,max_hp,ac,show_hp,gm_note,status) VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(token_id) DO UPDATE SET hp=excluded.hp,max_hp=excluded.max_hp,ac=excluded.ac,show_hp=excluded.show_hp,gm_note=excluded.gm_note,status=excluded.status",params![token_id,table,hp,max_hp,ac,show_hp,gm_note,status])?;
            log(
                conn,
                table,
                Some(&t),
                if *show_hp { "public" } else { "owner" },
                ("hp", old.map(|n| hp - n), vec![], ""),
            )?;
        }
        CombatAction::SetCondition { condition: c } => {
            identifier(&c.id)?;
            let t = board_token(conn, table, &c.token_id)?;
            if role != "gm" && t.owner_id != account {
                return Err(ApiError(
                    StatusCode::FORBIDDEN,
                    "Cannot change another token's conditions.",
                ));
            }
            if ![
                "blinded",
                "charmed",
                "deafened",
                "frightened",
                "grappled",
                "incapacitated",
                "invisible",
                "paralyzed",
                "petrified",
                "poisoned",
                "prone",
                "restrained",
                "stunned",
                "unconscious",
                "exhaustion",
            ]
            .contains(&c.name.as_str())
                || c.rounds.is_some_and(|n| n == 0 || n > 10000)
                || c.expired
            {
                return Err(ApiError::invalid());
            }
            if s.conditions
                .iter()
                .any(|v| v.id == c.id && v.token_id != c.token_id)
            {
                return Err(ApiError::invalid());
            }
            if !s.running {
                s.scene_id = Some(t.scene_id.clone());
            }
            s.conditions.retain(|v| v.id != c.id);
            if s.conditions.len() >= 200 {
                return Err(ApiError::limited());
            }
            s.conditions.push(c.clone());
            log(
                conn,
                table,
                Some(&t),
                "public",
                ("condition", c.rounds.map(|n| n as i32), vec![], &c.name),
            )?;
        }
        CombatAction::RemoveCondition { condition_id } => {
            identifier(condition_id)?;
            let c = s
                .conditions
                .iter()
                .find(|v| &v.id == condition_id)
                .ok_or_else(ApiError::invalid)?;
            let t = board_token(conn, table, &c.token_id)?;
            if role != "gm" && t.owner_id != account {
                return Err(ApiError(
                    StatusCode::FORBIDDEN,
                    "Cannot change another token's conditions.",
                ));
            }
            log(
                conn,
                table,
                Some(&t),
                "public",
                ("conditionRemoved", None, vec![], &c.name),
            )?;
            s.conditions.retain(|v| &v.id != condition_id);
        }
        CombatAction::Roll {
            token_id,
            count,
            sides,
            modifier,
            secret,
            label,
        } => {
            let t = own(conn, table, account, token_id)?;
            if *secret {
                gm(role)?;
            }
            if !(-1000..=1000).contains(modifier) {
                return Err(ApiError::invalid());
            }
            text(label, 0, 100)?;
            let values = dice(*count, *sides)?;
            let total = values.iter().sum::<u32>() as i32 + modifier;
            log(
                conn,
                table,
                Some(&t),
                if *secret { "gm" } else { "public" },
                ("roll", Some(total), values, label),
            )?;
        }
    }
    store(conn, table, &s)
}
/// Called after a physical token removal, in the same board transaction.
pub(crate) fn remove_token(conn: &Connection, table: &str, id: &str) -> Result<()> {
    let mut s = state(conn, table)?;
    s.conditions.retain(|c| c.token_id != id);
    s.order.retain(|c| c.token_id != id);
    if s.current.as_deref() == Some(id) {
        s.current = s.order.first().map(|v| v.token_id.clone());
        if s.order.is_empty() {
            s.running = false;
        }
        reset_movement(conn, table, &s)?;
    }
    store(conn, table, &s)
}
type StoredVitals = (i32, i32, Option<i32>, bool, Option<String>, Option<String>);

pub(crate) fn projection(
    conn: &Connection,
    table: &str,
    account: &str,
    role: &str,
    tokens: &[BoardToken],
    active: &Option<String>,
) -> Result<(Option<CombatState>, Vec<VitalsView>, Vec<CombatLog>)> {
    let mut combat = state(conn, table)?;
    let visible = |id: &str| tokens.iter().any(|t| t.id == id);
    let available = role == "gm" || combat.scene_id.as_ref() == active.as_ref();
    combat.order.retain(|v| visible(&v.token_id));
    for v in &mut combat.order {
        if v.secret && role != "gm" {
            v.initiative = None;
        }
    }
    combat.conditions.retain(|c| visible(&c.token_id));
    if combat.current.as_deref().is_some_and(|id| !visible(id)) {
        combat.current = None;
    }
    let mut vitals = Vec::new();
    for t in tokens {
        let row:Option<StoredVitals>=conn.query_row("SELECT hp,max_hp,ac,show_hp,gm_note,status FROM table_token_vitals WHERE table_id=?1 AND token_id=?2",params![table,t.id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?;
        if let Some((hp, max_hp, ac, show_hp, note, status)) = row {
            let own = t.owner_id == account;
            vitals.push(VitalsView {
                token_id: t.id.clone(),
                show_hp,
                status,
                hp: if own || show_hp { Some(hp) } else { None },
                max_hp: if own || show_hp { Some(max_hp) } else { None },
                ac: if own { ac } else { None },
                gm_note: if own && role == "gm" { note } else { None },
            });
        }
    }
    let mut q=conn.prepare("SELECT id,token_id,audience,owner_id,data FROM table_combat_log WHERE table_id=?1 AND (scene_id IS NULL OR scene_id=?4 OR ?2='gm') AND (audience='public' OR (audience='gm' AND ?2='gm') OR (audience='owner' AND owner_id=?3)) ORDER BY id DESC LIMIT 200")?;
    let rows = q
        .query_map(params![table, role, account, active], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, String>(4)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut events = Vec::new();
    for (id, token, data) in rows {
        if token.as_deref().is_none_or(visible) {
            let mut event: CombatLog =
                serde_json::from_str(&data).map_err(|_| ApiError::internal())?;
            event.id = id;
            events.push(event);
        }
    }
    events.reverse();
    Ok((if available { Some(combat) } else { None }, vitals, events))
}
