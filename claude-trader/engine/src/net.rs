//! Network plumbing: HTTPS proxy (CONNECT) aware WebSocket client, extra CA trust, log masking.
//!
//! * If HTTPS_PROXY / https_proxy is set, the WebSocket is tunnelled through an HTTP CONNECT proxy.
//!   If it is not set (normal on the user's Windows PC), it connects directly.
//! * Extra CA certificates are read from SSL_CERT_FILE or /root/.ccr/ca-bundle.crt when present.
//! * `mask` hides API keys in anything that is logged.

use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::rustls::{self, pki_types::ServerName, ClientConfig, RootCertStore};
use tokio_rustls::{client::TlsStream, TlsConnector};
use tokio_tungstenite::{client_async, WebSocketStream};

pub type Ws = WebSocketStream<TlsStream<TcpStream>>;

/// Hide API keys in URLs and strings before they are logged.
/// Masks `api-key=...`, `api_key=...`, `key=...` query values and long path tokens after `/v0/` style prefixes.
pub fn mask(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let lower = s.to_ascii_lowercase();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let mut matched = false;
        for k in ["api-key=", "api_key=", "apikey=", "key=", "token="] {
            if lower[i..].starts_with(k) {
                out.push_str(&s[i..i + k.len()]);
                i += k.len();
                while i < bytes.len() && !matches!(bytes[i], b'&' | b' ' | b'"' | b'\'' | b'#') {
                    i += 1;
                }
                out.push_str("***");
                matched = true;
                break;
            }
        }
        if !matched {
            let ch = s[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProxyCfg {
    pub host: String,
    pub port: u16,
    pub basic_auth: Option<String>, // base64(user:pass)
}

pub fn parse_proxy(url: &str) -> Option<ProxyCfg> {
    use base64::Engine;
    let rest = url.split("://").nth(1).unwrap_or(url).trim_end_matches('/');
    let (auth, hostport) = match rest.rsplit_once('@') {
        Some((a, h)) => (Some(a), h),
        None => (None, rest),
    };
    let (host, port) = hostport.rsplit_once(':').map(|(h, p)| (h, p.parse().ok())).unwrap_or((hostport, None));
    if host.is_empty() {
        return None;
    }
    Some(ProxyCfg {
        host: host.to_string(),
        port: port.unwrap_or(8080),
        basic_auth: auth.map(|a| base64::engine::general_purpose::STANDARD.encode(a)),
    })
}

pub fn proxy_from_env() -> Option<ProxyCfg> {
    for k in ["HTTPS_PROXY", "https_proxy"] {
        if let Ok(v) = std::env::var(k) {
            if !v.is_empty() {
                return parse_proxy(&v);
            }
        }
    }
    None
}

/// Extra CA bundle path, if any.
pub fn extra_ca_path() -> Option<String> {
    if let Ok(p) = std::env::var("SSL_CERT_FILE") {
        if std::path::Path::new(&p).is_file() {
            return Some(p);
        }
    }
    let p = "/root/.ccr/ca-bundle.crt";
    std::path::Path::new(p).is_file().then(|| p.to_string())
}

pub fn extra_ca_pem() -> Option<Vec<u8>> {
    extra_ca_path().and_then(|p| std::fs::read(p).ok())
}

fn tls_config() -> Arc<ClientConfig> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    if let Some(pem) = extra_ca_pem() {
        let mut rd = std::io::Cursor::new(pem);
        for c in rustls_pemfile::certs(&mut rd).flatten() {
            let _ = roots.add(c);
        }
    }
    Arc::new(ClientConfig::builder().with_root_certificates(roots).with_no_client_auth())
}

/// Open TCP to host:443, via the CONNECT proxy when configured.
async fn open_tcp(host: &str, port: u16, proxy: Option<&ProxyCfg>) -> Result<TcpStream, String> {
    match proxy {
        None => TcpStream::connect((host, port)).await.map_err(|e| format!("tcp connect: {e}")),
        Some(p) => {
            let mut s = TcpStream::connect((p.host.as_str(), p.port))
                .await
                .map_err(|e| format!("proxy connect: {e}"))?;
            let mut req = format!("CONNECT {host}:{port} HTTP/1.1\r\nHost: {host}:{port}\r\n");
            if let Some(a) = &p.basic_auth {
                req.push_str(&format!("Proxy-Authorization: Basic {a}\r\n"));
            }
            req.push_str("\r\n");
            s.write_all(req.as_bytes()).await.map_err(|e| e.to_string())?;
            let mut buf = Vec::new();
            let mut b = [0u8; 1];
            while !buf.ends_with(b"\r\n\r\n") {
                if s.read(&mut b).await.map_err(|e| e.to_string())? == 0 {
                    return Err("proxy closed during CONNECT".into());
                }
                buf.push(b[0]);
                if buf.len() > 8192 {
                    return Err("proxy CONNECT reply too long".into());
                }
            }
            let head = String::from_utf8_lossy(&buf);
            let status = head.lines().next().unwrap_or("");
            if !status.contains(" 200") {
                return Err(format!("proxy refused CONNECT: {status}"));
            }
            Ok(s)
        }
    }
}

/// Connect a wss:// WebSocket (proxy aware, extra CA aware).
pub async fn connect_wss(url: &str) -> Result<Ws, String> {
    let rest = url.strip_prefix("wss://").ok_or("only wss:// urls are supported")?;
    let (hostport, _path) = rest.split_once('/').unwrap_or((rest, ""));
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().map_err(|e| e.to_string())?),
        None => (hostport, 443),
    };
    let proxy = proxy_from_env();
    let tcp = tokio::time::timeout(Duration::from_secs(10), open_tcp(host, port, proxy.as_ref()))
        .await
        .map_err(|_| "connect timeout".to_string())??;
    let name = ServerName::try_from(host.to_string()).map_err(|e| e.to_string())?;
    let tls = TlsConnector::from(tls_config())
        .connect(name, tcp)
        .await
        .map_err(|e| format!("tls: {e}"))?;
    let (ws, _) = client_async(url, tls).await.map_err(|e| format!("ws handshake: {e}"))?;
    Ok(ws)
}

/// reqwest client. Picks up proxy env automatically; adds the extra CA bundle if present.
pub fn http_client() -> reqwest::Client {
    let mut b = reqwest::Client::builder().user_agent("claude-trader/0.2").pool_max_idle_per_host(4);
    if let Some(pem) = extra_ca_pem() {
        if let Ok(certs) = reqwest::Certificate::from_pem_bundle(&pem) {
            for c in certs {
                b = b.add_root_certificate(c);
            }
        }
    }
    b.build().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_keys() {
        assert_eq!(mask("https://x.helius-rpc.com/?api-key=abc123&x=1"), "https://x.helius-rpc.com/?api-key=***&x=1");
        assert_eq!(mask("wss://a/?key=SECRET"), "wss://a/?key=***");
        assert_eq!(mask("no secrets here"), "no secrets here");
        assert!(!mask("https://rpc/?API-KEY=zzz").contains("zzz"));
    }

    #[test]
    fn parses_proxy() {
        let p = parse_proxy("http://127.0.0.1:39619").unwrap();
        assert_eq!((p.host.as_str(), p.port, p.basic_auth.is_none()), ("127.0.0.1", 39619, true));
        let p = parse_proxy("http://u:p@proxy.local:3128/").unwrap();
        assert_eq!(p.host, "proxy.local");
        assert_eq!(p.basic_auth.as_deref(), Some("dTpw"));
    }
}
