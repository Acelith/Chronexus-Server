use ipnet::IpNet;
use std::{collections::HashMap, net::SocketAddr, path::PathBuf};
use url::Url;

#[derive(Clone)]
pub struct Config {
    pub listen: SocketAddr,
    pub database: PathBuf,
    pub public_origin: String,
    pub authority: String,
    pub trusted_proxies: Vec<IpNet>,
    pub loopback_http: bool,
    /// User names with the operator role (service status and account metadata, never
    /// campaign content). Set by whoever controls the host configuration.
    pub operators: Vec<String>,
    /// Optional server AI (M25.10); `None` unless the operator configured a runtime.
    pub ai: Option<AiConfig>,
}

/// Server AI settings. Every value is explicit operator configuration; nothing is
/// downloaded or started by the server itself.
#[derive(Clone, Debug)]
pub struct AiConfig {
    /// Runtime base URL on the internal network, plain HTTP (e.g. `http://ai:11434`).
    pub url: Url,
    /// Allowed models: name and, when pinned, the runtime digest that must match.
    pub models: Vec<(String, Option<String>)>,
    /// Generations running at the same time.
    pub concurrency: usize,
    /// Requests allowed to wait for a free slot; more are refused, never queued silently.
    pub queue: usize,
    /// Questions per account per hour.
    pub per_user_hour: usize,
    pub timeout_seconds: u64,
}
fn parse_ai(get: impl Fn(&str, &str) -> String) -> std::result::Result<Option<AiConfig>, String> {
    let raw = get("CHRONEXUS_AI_URL", "");
    if raw.trim().is_empty() {
        return Ok(None);
    }
    let url = Url::parse(raw.trim()).map_err(|_| "CHRONEXUS_AI_URL must be a URL.")?;
    if url.scheme() != "http"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
    {
        return Err(
            "CHRONEXUS_AI_URL must be the internal runtime origin, e.g. http://ai:11434.".into(),
        );
    }
    let models: Vec<(String, Option<String>)> = get("CHRONEXUS_AI_MODELS", "")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|entry| {
            let (name, digest) = match entry.split_once('@') {
                Some((n, d)) => (n, Some(d.to_ascii_lowercase())),
                None => (entry, None),
            };
            let name_ok = (1..=128).contains(&name.len())
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._:-/".contains(c));
            let digest_ok = digest
                .as_ref()
                .is_none_or(|d| d.len() == 64 && d.bytes().all(|b| b.is_ascii_hexdigit()));
            if name_ok && digest_ok {
                Ok((name.to_string(), digest))
            } else {
                Err(
                    "CHRONEXUS_AI_MODELS lists model names, optionally name@sha256-digest."
                        .to_string(),
                )
            }
        })
        .collect::<std::result::Result<_, _>>()?;
    if models.is_empty() {
        return Err("Set CHRONEXUS_AI_MODELS to the models allowed on this server.".into());
    }
    let number = |key: &str, default: &str, max: u64| -> std::result::Result<u64, String> {
        get(key, default)
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|n| (1..=max).contains(n))
            .ok_or_else(|| format!("{key} must be a number between 1 and {max}."))
    };
    Ok(Some(AiConfig {
        url,
        models,
        concurrency: number("CHRONEXUS_AI_CONCURRENCY", "1", 8)? as usize,
        queue: number("CHRONEXUS_AI_QUEUE", "4", 64)? as usize,
        per_user_hour: number("CHRONEXUS_AI_PER_USER_HOUR", "30", 1000)? as usize,
        timeout_seconds: number("CHRONEXUS_AI_TIMEOUT_SECONDS", "120", 900)?,
    }))
}

impl Config {
    pub fn from_env() -> std::result::Result<Self, String> {
        Self::parse(std::env::vars().collect())
    }
    pub fn parse(env: HashMap<String, String>) -> std::result::Result<Self, String> {
        let get =
            |key: &str, default: &str| env.get(key).cloned().unwrap_or_else(|| default.into());
        let listen: SocketAddr = get("CHRONEXUS_LISTEN", "127.0.0.1:8080")
            .parse()
            .map_err(|_| "CHRONEXUS_LISTEN must be an IP address and port.")?;
        if listen.port() == 0 {
            return Err("CHRONEXUS_LISTEN must use a nonzero port.".into());
        }
        let origin = get("CHRONEXUS_PUBLIC_ORIGIN", "");
        let url = Url::parse(&origin)
            .map_err(|_| "Set CHRONEXUS_PUBLIC_ORIGIN to the exact HTTPS origin.")?;
        if url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(
                "Public origin must not contain credentials, a path, query or fragment.".into(),
            );
        }
        let loopback_http = match get("CHRONEXUS_ALLOW_LOOPBACK_HTTP", "false").as_str() {
            "true" => true,
            "false" => false,
            _ => return Err("CHRONEXUS_ALLOW_LOOPBACK_HTTP must be true or false.".into()),
        };
        let origin_loopback = url.host_str().is_some_and(|h| {
            h == "localhost"
                || h.trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        });
        if loopback_http {
            if !listen.ip().is_loopback() || !origin_loopback || url.scheme() != "http" {
                return Err(
                    "Development HTTP requires a loopback listener and HTTP loopback origin."
                        .into(),
                );
            }
        } else if url.scheme() != "https" {
            return Err("Public origin must use HTTPS.".into());
        }
        let trusted_proxies: Vec<IpNet> = get("CHRONEXUS_TRUSTED_PROXIES", "")
            .split(',')
            .filter(|s| !s.trim().is_empty())
            .map(|s| {
                s.trim()
                    .parse()
                    .or_else(|_| s.trim().parse::<std::net::IpAddr>().map(IpNet::from))
                    .map_err(|_| "Invalid trusted proxy IP or CIDR.".to_string())
            })
            .collect::<std::result::Result<_, _>>()?;
        if trusted_proxies.iter().any(|net| net.prefix_len() == 0) {
            return Err("Trusting every address as a proxy is forbidden.".into());
        }
        if !loopback_http && trusted_proxies.is_empty() {
            return Err("Configure CHRONEXUS_TRUSTED_PROXIES for the HTTPS reverse proxy.".into());
        }
        let public_origin = url.origin().ascii_serialization();
        let authority = public_origin.split_once("://").unwrap().1.to_string();
        let operators: Vec<String> = get("CHRONEXUS_OPERATORS", "")
            .split(',')
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .map(|s| {
                let valid = (3..=64).contains(&s.len())
                    && s.chars()
                        .all(|c| c.is_ascii_alphanumeric() || ['.', '_', '-'].contains(&c));
                if valid {
                    Ok(s)
                } else {
                    Err("CHRONEXUS_OPERATORS must list user names separated by commas.".to_string())
                }
            })
            .collect::<std::result::Result<_, _>>()?;
        let ai = parse_ai(get)?;
        Ok(Self {
            listen,
            database: get("CHRONEXUS_DATABASE", "data/chronexus.sqlite3").into(),
            public_origin,
            authority,
            trusted_proxies,
            loopback_http,
            operators,
            ai,
        })
    }
}
