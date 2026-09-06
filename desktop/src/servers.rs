//! Saved remote music servers (Subsonic/Navidrome + Jellyfin) and the
//! translation from credentials to the daemon's browse-URL schemes
//! (`navidrome://…`, `jellyfin://…` — resolved by rockboxd's BrowseService).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedServer {
    pub kind: String, // "subsonic" | "jellyfin"
    pub name: String,
    pub url: String,
    pub username: String,
    pub password: String,
}

fn file() -> Option<PathBuf> {
    dirs::home_dir().map(|h| {
        h.join(".config")
            .join("rockbox.org")
            .join("desktop_servers.json")
    })
}

pub fn load() -> Vec<SavedServer> {
    let Some(path) = file() else { return vec![] };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(list: &[SavedServer]) {
    if let Some(path) = file() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(list) {
            let _ = std::fs::write(path, json);
        }
    }
}

/// Same unreserved set as crates/{navidrome,jellyfin}::percent_encode.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3 / 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// Builds the daemon browse-root URL for a server, authenticating if needed.
pub async fn root_url(server: &SavedServer) -> Result<String, String> {
    let base = server.url.trim_end_matches('/');
    match server.kind.as_str() {
        "jellyfin" => {
            let (token, user_id) = jellyfin_authenticate(base, &server.username, &server.password)
                .await
                .ok_or_else(|| format!("Jellyfin login failed for {}", server.url))?;
            let with_creds = format!("{base}?X-Jellyfin-Token={token}&userId={user_id}");
            Ok(format!("jellyfin://{}", percent_encode(&with_creds)))
        }
        _ => {
            // Subsonic token auth: token = md5(password + salt).
            let salt = format!(
                "{:x}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_nanos()
            );
            let salt = &salt[..salt.len().min(12)];
            let token = format!("{:x}", md5::compute(format!("{}{}", server.password, salt)));
            let with_creds = format!(
                "{base}?nd_user={}&nd_token={token}&nd_salt={salt}",
                server.username
            );
            Ok(format!("navidrome://{}", percent_encode(&with_creds)))
        }
    }
}

/// Jellyfin /Users/AuthenticateByName → (access_token, user_id).
/// Mirrors crates/jellyfin::authenticate (not depended on directly to keep
/// this crate outside the workspace dependency graph).
async fn jellyfin_authenticate(
    base_url: &str,
    username: &str,
    password: &str,
) -> Option<(String, String)> {
    #[derive(Deserialize)]
    struct AuthUser {
        #[serde(rename = "Id")]
        id: String,
    }
    #[derive(Deserialize)]
    struct AuthResponse {
        #[serde(rename = "AccessToken")]
        access_token: String,
        #[serde(rename = "User")]
        user: AuthUser,
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;
    let url = format!(
        "{}/Users/AuthenticateByName",
        base_url.trim_end_matches('/')
    );
    let body = serde_json::json!({ "Username": username, "Pw": password });
    let resp = client
        .post(&url)
        .header(
            "X-Emby-Authorization",
            r#"MediaBrowser Client="Rockbox", Device="Rockbox", DeviceId="rockbox-desktop", Version="1.0""#,
        )
        .json(&body)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        tracing::warn!("jellyfin auth {url}: HTTP {}", resp.status());
        return None;
    }
    let auth = resp.json::<AuthResponse>().await.ok()?;
    Some((auth.access_token, auth.user.id))
}
