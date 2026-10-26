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
        Ok(Self {
            listen,
            database: get("CHRONEXUS_DATABASE", "data/chronexus.sqlite3").into(),
            public_origin,
            authority,
            trusted_proxies,
            loopback_http,
        })
    }
}
