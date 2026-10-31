//! Explicit GM-only public catalog lookups. No campaign text or arbitrary URLs leave the server.
use crate::{
    AppState,
    error::{ApiError, Result},
    library::{Attribution, Monster},
    vtt::{Distance, MapAsset},
};
use axum::{
    Json, Router,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
    routing::post,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{OpenApi, ToSchema};
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    kind: String,
    query: String,
    entry: Option<String>,
}
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub source: Option<Attribution>,
}
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct Reply {
    entries: Vec<Entry>,
    monster: Option<Monster>,
    asset: Option<MapAsset>,
    source: Option<Attribution>,
}
fn failure() -> ApiError {
    ApiError(
        StatusCode::BAD_GATEWAY,
        "Public catalog unavailable. Retry later or import a local file.",
    )
}
fn stat_block(data: &Value) -> String {
    // Labels below belong to the canonical English SRD copy, not the interface.
    let mut lines = vec![data["name"].as_str().unwrap_or_default().to_owned()];
    for (key, label) in [
        ("armor_class", "Armor Class"),
        ("hit_points", "Hit Points"),
        ("speed", "Speed"),
        ("senses", "Senses"),
        ("proficiencies", "Proficiencies"),
        ("damage_vulnerabilities", "Damage Vulnerabilities"),
        ("damage_resistances", "Damage Resistances"),
        ("damage_immunities", "Damage Immunities"),
        ("condition_immunities", "Condition Immunities"),
        ("size", "Size"),
        ("type", "Type"),
        ("alignment", "Alignment"),
        ("hit_dice", "Hit Dice"),
        ("strength", "Strength"),
        ("dexterity", "Dexterity"),
        ("constitution", "Constitution"),
        ("intelligence", "Intelligence"),
        ("wisdom", "Wisdom"),
        ("charisma", "Charisma"),
        ("languages", "Languages"),
        ("challenge_rating", "Challenge Rating"),
        ("xp", "XP"),
    ] {
        if !data[key].is_null() {
            let v = rule_text(&data[key]);
            lines.push(format!("{label}: {v}"));
        }
    }
    for (key, label) in [
        ("special_abilities", "Traits"),
        ("actions", "Actions"),
        ("reactions", "Reactions"),
        ("legendary_actions", "Legendary Actions"),
    ] {
        if let Some(entries) = data[key].as_array().filter(|a| !a.is_empty()) {
            lines.push(format!("\n{label}"));
            for entry in entries {
                lines.push(format!(
                    "{}: {}",
                    entry["name"].as_str().unwrap_or_default(),
                    entry["desc"].as_str().unwrap_or_default()
                ));
            }
        }
    }
    lines.join("\n")
}
fn rule_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().map(rule_text).collect::<Vec<_>>().join("; "),
        Value::Object(o) => {
            if let Some(name) = o.get("name").and_then(Value::as_str) {
                return name.into();
            }
            o.iter()
                .filter(|(k, _)| !["url", "index"].contains(&k.as_str()))
                .map(|(k, v)| format!("{}: {}", k.replace('_', " "), rule_text(v)))
                .collect::<Vec<_>>()
                .join("; ")
        }
        Value::Null => String::new(),
        _ => v.to_string(),
    }
}
async fn fetch(url: url::Url, limit: usize) -> Result<Vec<u8>> {
    if url.scheme() != "https"
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(ApiError::invalid());
    }
    let allowed = match url.host_str() {
        Some("www.dnd5eapi.co") => url.path().starts_with("/api/2014/monsters"),
        Some("commons.wikimedia.org") => url.path() == "/w/api.php",
        Some("upload.wikimedia.org") => url.path().starts_with("/wikipedia/commons/"),
        _ => false,
    };
    if !allowed {
        return Err(ApiError::invalid());
    }
    let client=reqwest::Client::builder().https_only(true).no_proxy().redirect(reqwest::redirect::Policy::none()).timeout(std::time::Duration::from_secs(12)).user_agent("Chronexus-Server/0.1.1 (explicit public catalog lookup; https://github.com/Acelith/Chronexus-Server)").build().map_err(|_|failure())?;
    let mut response = client.get(url).send().await.map_err(|_| failure())?;
    if !response.status().is_success()
        || response.content_length().is_some_and(|n| n > limit as u64)
    {
        return Err(failure());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| failure())? {
        if bytes.len() + chunk.len() > limit {
            return Err(failure());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
async fn json(url: url::Url) -> Result<Value> {
    serde_json::from_slice(&fetch(url, 2 * 1024 * 1024).await?).map_err(|_| failure())
}
fn url(value: &str) -> Result<url::Url> {
    url::Url::parse(value).map_err(|_| ApiError::invalid())
}
fn plain(value: &str) -> String {
    let mut tag = false;
    value
        .chars()
        .filter(|c| {
            if *c == '<' {
                tag = true;
                false
            } else if *c == '>' {
                tag = false;
                false
            } else {
                !tag
            }
        })
        .take(1000)
        .collect()
}
fn commons_entry(page: &Value) -> Option<(Entry, String)> {
    let info = page.get("imageinfo")?.get(0)?;
    let meta = info.get("extmetadata")?;
    let field = |k: &str| meta.get(k)?.get("value")?.as_str();
    let license = field("LicenseShortName")?;
    // A deliberately small initial allowlist; no non-commercial or unknown licences.
    if ![
        "CC0",
        "CC0 1.0",
        "Public domain",
        "CC BY 4.0",
        "CC BY-SA 4.0",
    ]
    .contains(&license)
        || meta.get("Copyrighted").is_none()
    {
        return None;
    }
    let image = info.get("url")?.as_str()?;
    let parsed = url::Url::parse(image).ok()?;
    if parsed.host_str() != Some("upload.wikimedia.org")
        || parsed.scheme() != "https"
        || !parsed.path().starts_with("/wikipedia/commons/")
    {
        return None;
    }
    let mime = info.get("mime")?.as_str()?;
    if !["image/png", "image/jpeg", "image/webp"].contains(&mime)
        || info.get("size")?.as_u64()? > 8 * 1024 * 1024
        || info.get("width")?.as_u64()? > 8192
        || info.get("height")?.as_u64()? > 8192
    {
        return None;
    }
    let id = page.get("pageid")?.as_u64()?.to_string();
    let source = Attribution {
        url: format!("https://commons.wikimedia.org/?curid={id}"),
        author: plain(field("Artist")?),
        license: license.into(),
        edition: "Wikimedia Commons".into(),
        original: image.into(),
    };
    Some((
        Entry {
            id,
            name: page
                .get("title")?
                .as_str()?
                .trim_start_matches("File:")
                .into(),
            source: Some(source),
        },
        mime.into(),
    ))
}
async fn lookup(state: &AppState, bearer: &str, table: &str, r: Request) -> Result<Reply> {
    let mut reply = Reply {
        entries: vec![],
        monster: None,
        asset: None,
        source: None,
    };
    if r.query.chars().count() > 100 {
        return Err(ApiError::invalid());
    }
    if r.kind == "monsters" {
        let suffix = if let Some(id) = &r.entry {
            if id.is_empty()
                || id.len() > 100
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            {
                return Err(ApiError::invalid());
            }
            format!("/{id}")
        } else {
            String::new()
        };
        let origin = format!("https://www.dnd5eapi.co/api/2014/monsters{suffix}");
        let data = json(url(&origin)?).await?;
        if r.entry.is_some() {
            let original = serde_json::to_string_pretty(&data).map_err(|_| failure())?;
            let speed = data["speed"]["walk"]
                .as_str()
                .and_then(|s| s.strip_suffix(" ft."))
                .and_then(|s| s.parse::<f64>().ok())
                .ok_or_else(failure)?;
            reply.monster=Some(Monster{id:uuid::Uuid::new_v4().to_string(),name:data["name"].as_str().ok_or_else(failure)?.into(),hp:i32::try_from(data["hit_points"].as_i64().ok_or_else(failure)?).map_err(|_|failure())?,ac:i32::try_from(data["armor_class"][0]["value"].as_i64().ok_or_else(failure)?).map_err(|_|failure())?,speed:Distance{value:speed,unit:"ft".into()},size:match data["size"].as_str(){Some("Tiny")=>0.5,Some("Small"|"Medium")=>1.0,Some("Large")=>2.0,Some("Huge")=>3.0,Some("Gargantuan")=>4.0,_=>return Err(failure())},stat_block:stat_block(&data),source:Some(Attribution{url:origin,author:"Wizards of the Coast LLC; API: 5e-bits".into(),license:"SRD 5.1 — CC BY 4.0 (https://creativecommons.org/licenses/by/4.0/); database MIT".into(),edition:"D&D 5e 2014 / SRD 5.1".into(),original}),portrait_id:None});
        } else {
            reply.entries = data["results"]
                .as_array()
                .ok_or_else(failure)?
                .iter()
                .filter(|v| {
                    v["name"]
                        .as_str()
                        .is_some_and(|n| n.to_lowercase().contains(&r.query.to_lowercase()))
                })
                .take(50)
                .filter_map(|v| {
                    Some(Entry {
                        id: v["index"].as_str()?.into(),
                        name: v["name"].as_str()?.into(),
                        source: None,
                    })
                })
                .collect();
        }
    } else if r.kind == "assets" {
        let mut endpoint = url("https://commons.wikimedia.org/w/api.php")?;
        {
            let mut q = endpoint.query_pairs_mut();
            q.extend_pairs([
                ("action", "query"),
                ("format", "json"),
                ("prop", "imageinfo"),
                ("iiprop", "url|mime|size|extmetadata"),
            ]);
            if let Some(id) = &r.entry {
                if id.is_empty() || id.len() > 20 || !id.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(ApiError::invalid());
                }
                q.append_pair("pageids", id);
            } else {
                if r.query.trim().is_empty() {
                    return Err(ApiError::invalid());
                }
                q.extend_pairs([
                    ("generator", "search"),
                    ("gsrnamespace", "6"),
                    ("gsrlimit", "20"),
                ]);
                q.append_pair("gsrsearch", &format!("{} filetype:bitmap", r.query));
            }
        }
        let data = json(endpoint).await?;
        if let Some(pages) = data["query"]["pages"].as_object() {
            for page in pages.values() {
                if let Some((entry, mime)) = commons_entry(page) {
                    if r.entry.is_some() {
                        let source = entry.source.ok_or_else(failure)?;
                        let bytes = fetch(url(&source.original)?, 8 * 1024 * 1024).await?;
                        let bearer = bearer.to_owned();
                        let table = table.to_owned();
                        reply.asset = Some(
                            state
                                .blocking(move |db| db.put_map(&bearer, &table, &bytes, &mime))
                                .await?,
                        );
                        reply.source = Some(source);
                    } else {
                        reply.entries.push(entry);
                    }
                }
            }
        }
    } else {
        return Err(ApiError::invalid());
    }
    Ok(reply)
}
pub(crate) fn router() -> Router<AppState> {
    Router::new().route("/api/v1/tables/{tableId}/catalog", post(catalog))
}
#[utoipa::path(post,path="/api/v1/tables/{tableId}/catalog",params(("tableId"=String,Path)),request_body=Request,security(("bearer"=[])),responses((status=200,body=Reply)),tag="table-catalog")]
async fn catalog(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(table): Path<String>,
    input: std::result::Result<Json<Request>, JsonRejection>,
) -> Result<Json<Reply>> {
    let bearer = crate::routes::token(&headers)?;
    let r = crate::routes::body(input)?;
    let b = bearer.clone();
    let t = table.clone();
    state
        .blocking(move |db| {
            let user = db.identity(&b)?;
            db.with(|c| {
                use rusqlite::{params,OptionalExtension};
                let tx=c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
                crate::vtt::gm(&crate::vtt::access(&tx,&t,&user.account_id)?)?;
                let now=crate::auth::now();
                tx.execute("DELETE FROM rate_buckets WHERE reset_at<=?1",[now])?;
                let key=crate::auth::digest(&format!("catalog:{}:{}",user.account_id,now/60));
                let attempts:Option<i64>=tx.query_row("SELECT attempts FROM rate_buckets WHERE key_hash=?1",[&key],|r|r.get(0)).optional()?;
                let count:i64=tx.query_row("SELECT count(*) FROM rate_buckets",[],|r|r.get(0))?;
                if attempts.unwrap_or(0)>=30 || (attempts.is_none()&&count>=10000){return Err(ApiError::limited());}
                tx.execute("INSERT INTO rate_buckets VALUES(?1,1,?2) ON CONFLICT(key_hash) DO UPDATE SET attempts=attempts+1",params![key,now+120])?;
                tx.commit()?;Ok(())
            })
        })
        .await?;
    let _permit = state
        .transfers
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::limited())?;
    tokio::time::timeout(
        std::time::Duration::from_secs(18),
        lookup(&state, &bearer, &table, r),
    )
    .await
    .map_err(|_| failure())?
    .map(Json)
}
#[derive(OpenApi)]
#[openapi(paths(catalog))]
pub(crate) struct CatalogContract;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn commons_keeps_attribution_and_refuses_unknown_licenses_remote_hosts_and_vectors() {
        let mut page = json!({"pageid":1,"title":"File:Synthetic.png","imageinfo":[{"url":"https://upload.wikimedia.org/wikipedia/commons/a/ab/Synthetic.png","mime":"image/png","width":64,"height":64,"size":100,"extmetadata":{"LicenseShortName":{"value":"CC0"},"Artist":{"value":"<a href='x'>Synthetic artist</a>"},"Copyrighted":{"value":"False"}}}]});
        let (entry, _) = commons_entry(&page).unwrap();
        assert_eq!(entry.source.unwrap().author, "Synthetic artist");
        page["imageinfo"][0]["extmetadata"]["LicenseShortName"]["value"] = json!("CC BY-NC 4.0");
        assert!(commons_entry(&page).is_none());
        page["imageinfo"][0]["extmetadata"]["LicenseShortName"]["value"] = json!("CC0");
        page["imageinfo"][0]["url"] = json!("https://127.0.0.1/private");
        assert!(commons_entry(&page).is_none());
        page["imageinfo"][0]["url"] =
            json!("https://upload.wikimedia.org/wikipedia/commons/a/ab/Synthetic.svg");
        page["imageinfo"][0]["mime"] = json!("image/svg+xml");
        assert!(commons_entry(&page).is_none());
    }
    #[test]
    fn stat_copy_retains_supplied_rules_without_computing_modifiers() {
        let value = json!({"name":"Synthetic","strength":13,"senses":{"darkvision":"60 ft."},"damage_immunities":["poison"],"actions":[{"name":"Test","desc":"Canonical action"}]});
        let text = stat_block(&value);
        assert!(text.contains("Strength: 13"));
        assert!(text.contains("darkvision: 60 ft."));
        assert!(text.contains("poison"));
        assert!(text.contains("Test: Canonical action"));
        assert!(!text.contains("+1"));
    }
}
