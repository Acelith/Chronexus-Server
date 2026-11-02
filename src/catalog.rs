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
    /// Inert game-icons.net vector for the client to rasterize into a token image.
    svg: Option<String>,
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
        Some("api.github.com") => url.path() == ICON_INDEX_PATH,
        Some("raw.githubusercontent.com") => url.path().starts_with(ICON_RAW_PREFIX),
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
const ICON_INDEX_PATH: &str = "/repos/game-icons/icons/git/trees/master";
const ICON_RAW_PREFIX: &str = "/game-icons/icons/master/";
/// Contributor folders the game-icons.net licence file marks as CC0; the rest are CC BY 3.0.
const ICON_CC0: [&str; 2] = ["viscious-speed", "zeromancer"];
/// The explicitly fetched icon index, kept in memory so repeated searches stay local.
static ICON_INDEX: std::sync::Mutex<Option<(std::time::Instant, std::sync::Arc<Vec<String>>)>> =
    std::sync::Mutex::new(None);
fn icon_id(id: &str) -> bool {
    let mut parts = id.split('/');
    let ok = |p: Option<&str>| {
        p.is_some_and(|p| {
            !p.is_empty()
                && p.len() <= 80
                && p.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
    };
    // Badges are site decorations, not credited contributor artwork.
    ok(parts.next()) && ok(parts.next()) && parts.next().is_none() && !id.starts_with("badges/")
}
fn icon_paths(data: &Value) -> Vec<String> {
    data["tree"]
        .as_array()
        .map(|tree| {
            tree.iter()
                .filter_map(|v| v["path"].as_str()?.strip_suffix(".svg"))
                .filter(|id| icon_id(id))
                .take(10_000)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}
fn icon_entry(id: &str) -> Entry {
    let (author, file) = id.split_once('/').unwrap_or_default();
    let license = if ICON_CC0.contains(&author) {
        "CC0 1.0 (https://creativecommons.org/publicdomain/zero/1.0/)"
    } else {
        "CC BY 3.0 (https://creativecommons.org/licenses/by/3.0/)"
    };
    Entry {
        id: id.into(),
        name: file.replace('-', " "),
        source: Some(Attribution {
            url: format!("https://game-icons.net/1x1/{id}.html"),
            author: format!("{author} — game-icons.net"),
            license: license.into(),
            edition: "game-icons.net; recoloured and cropped into a round token".into(),
            original: format!("https://raw.githubusercontent.com{ICON_RAW_PREFIX}{id}.svg"),
        }),
    }
}
/// Accepts only the plain path-based vectors game-icons.net publishes: anything that could
/// load external content or carry script is refused even though the client draws it as an image.
fn inert_svg(bytes: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(bytes).ok()?.trim();
    let lower = text.to_ascii_lowercase();
    let external = [
        "<script",
        "href",
        "<image",
        "<foreignobject",
        "url(",
        "<style",
    ]
    .iter()
    .any(|bad| lower.contains(bad));
    let handler = lower.split_whitespace().any(|word| word.starts_with("on"));
    (text.starts_with("<svg") && text.ends_with("</svg>") && !external && !handler)
        .then(|| text.to_owned())
}
async fn icons(r: &Request, reply: &mut Reply) -> Result<()> {
    if let Some(id) = &r.entry {
        if !icon_id(id) {
            return Err(ApiError::invalid());
        }
        let entry = icon_entry(id);
        let source = entry.source.ok_or_else(failure)?;
        reply.svg =
            Some(inert_svg(&fetch(url(&source.original)?, 256 * 1024).await?).ok_or_else(failure)?);
        reply.source = Some(source);
        return Ok(());
    }
    let query = r.query.trim().to_lowercase();
    if query.is_empty() {
        return Err(ApiError::invalid());
    }
    let cached = ICON_INDEX
        .lock()
        .map_err(|_| ApiError::internal())?
        .as_ref()
        .filter(|(at, _)| at.elapsed() < std::time::Duration::from_secs(6 * 3600))
        .map(|(_, paths)| paths.clone());
    let paths = match cached {
        Some(paths) => paths,
        None => {
            let index = url(&format!(
                "https://api.github.com{ICON_INDEX_PATH}?recursive=1"
            ))?;
            let paths = std::sync::Arc::new(icon_paths(&json(index).await?));
            if paths.is_empty() {
                return Err(failure());
            }
            *ICON_INDEX.lock().map_err(|_| ApiError::internal())? =
                Some((std::time::Instant::now(), paths.clone()));
            paths
        }
    };
    let words: Vec<&str> = query.split_whitespace().collect();
    reply.entries = paths
        .iter()
        .filter(|id| {
            let name = id.split_once('/').map_or("", |(_, n)| n);
            words.iter().all(|w| name.contains(w))
        })
        .take(60)
        .map(|id| icon_entry(id))
        .collect();
    Ok(())
}
async fn lookup(state: &AppState, bearer: &str, table: &str, r: Request) -> Result<Reply> {
    let mut reply = Reply {
        entries: vec![],
        monster: None,
        asset: None,
        source: None,
        svg: None,
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
    } else if r.kind == "icons" {
        icons(&r, &mut reply).await?;
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
    fn icon_index_keeps_contributor_vectors_with_their_licence() {
        let tree = json!({"tree":[
            {"path":"lorc/dragon-head.svg"},{"path":"zeromancer/goblin.svg"},
            {"path":"badges/award.svg"},{"path":"lorc"},{"path":"license.txt"},
            {"path":"../etc/passwd.svg"},{"path":"lorc/sub/deep.svg"},{"path":"Lorc/Upper.svg"}]});
        assert_eq!(icon_paths(&tree), ["lorc/dragon-head", "zeromancer/goblin"]);
        let entry = icon_entry("lorc/dragon-head");
        assert_eq!(entry.name, "dragon head");
        let source = entry.source.unwrap();
        assert!(source.license.starts_with("CC BY 3.0"));
        assert_eq!(
            source.original,
            "https://raw.githubusercontent.com/game-icons/icons/master/lorc/dragon-head.svg"
        );
        assert!(
            icon_entry("zeromancer/goblin")
                .source
                .unwrap()
                .license
                .starts_with("CC0")
        );
        assert!(!icon_id("lorc/../x") && !icon_id("lorc") && !icon_id("badges/award"));
    }
    /// Real network check: `cargo test --lib -- --ignored icon_catalog_live`.
    #[tokio::test]
    #[ignore = "contacts api.github.com and raw.githubusercontent.com"]
    async fn icon_catalog_live() {
        let request = |query: &str, entry: Option<&str>| Request {
            kind: "icons".into(),
            query: query.into(),
            entry: entry.map(Into::into),
        };
        let empty = || Reply {
            entries: vec![],
            monster: None,
            asset: None,
            source: None,
            svg: None,
        };
        let mut found = empty();
        icons(&request("goblin", None), &mut found).await.unwrap();
        let goblin = found
            .entries
            .iter()
            .find(|e| e.id == "delapouite/goblin-head")
            .expect("goblin head icon");
        let mut vector = empty();
        icons(&request("", Some(&goblin.id)), &mut vector)
            .await
            .unwrap();
        assert!(vector.svg.unwrap().starts_with("<svg"));
        assert!(vector.source.unwrap().license.starts_with("CC BY 3.0"));
    }
    #[test]
    fn icon_vectors_must_be_inert() {
        let plain = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512"><path d="M0 0h512v512H0z"/></svg>"#;
        assert!(inert_svg(plain.as_bytes()).is_some());
        for bad in [
            r#"<svg><script>alert(1)</script></svg>"#,
            r#"<svg><image href="https://example.org/x.png"/></svg>"#,
            r#"<svg onload="x()"><path d="M0 0"/></svg>"#,
            r#"<svg><path style="fill:url(#a)"/></svg>"#,
            r#"<html><svg></svg></html>"#,
        ] {
            assert!(inert_svg(bad.as_bytes()).is_none(), "{bad}");
        }
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
