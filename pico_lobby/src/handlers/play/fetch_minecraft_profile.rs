use crate::configuration::lobby::NpcSkinConfig;
use minecraft_packets::login::Property;
use minecraft_protocol::prelude::Uuid;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::time::Duration;
use tracing::warn;

/// `User-Agent` sent with every Mojang API request.
///
/// Mojang's edge can serve an anti-bot/challenge page (which is *not* JSON) to
/// clients that omit a `User-Agent`; reqwest sends none by default. An explicit
/// agent keeps us on the JSON happy path.
const USER_AGENT: &str = concat!("PicoLobby/", env!("CARGO_PKG_VERSION"));

/// Hard cap on how long a single Mojang request may take, so a hanging request
/// never stalls server startup.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Failure modes when talking to Mojang's APIs.
///
/// Each variant carries enough context (status code, body snippet) to explain
/// *why* a skin failed to resolve, instead of reqwest's opaque
/// "error decoding response body".
#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("could not build HTTP client: {0}")]
    Client(String),
    #[error("request failed: {0}")]
    Request(String),
    #[error("Mojang returned HTTP {status} (body: {body})")]
    Status { status: u16, body: String },
    #[error("could not decode Mojang response as JSON: {error} (body: {body})")]
    Decode { error: String, body: String },
}

#[derive(Deserialize, Clone)]
pub struct ProfileProperty {
    name: String,
    value: String,
    signature: Option<String>,
}

#[derive(Deserialize)]
pub struct Profile {
    properties: Vec<ProfileProperty>,
}

impl Profile {
    pub fn try_get_textures(&self) -> Option<ProfileProperty> {
        self.properties
            .iter()
            .find(|prop| prop.name == "textures")
            .cloned()
    }
}

/// Build the shared HTTP client used for all Mojang lookups.
///
/// Sets an explicit `User-Agent` and timeout; see [`USER_AGENT`].
fn http_client() -> Result<reqwest::Client, ProfileError> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|error| ProfileError::Client(error.to_string()))
}

/// Perform a GET and deserialize the JSON body, surfacing the real cause on
/// failure.
///
/// Unlike `reqwest::Response::json`, this checks the HTTP status *before*
/// decoding, so a rate-limit (`429`), throttle, or anti-bot page is reported as
/// a [`ProfileError::Status`] with a body snippet rather than collapsing into a
/// misleading "error decoding response body".
async fn get_json<T: DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
) -> Result<T, ProfileError> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| ProfileError::Request(error.to_string()))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| ProfileError::Request(error.to_string()))?;

    if !status.is_success() {
        return Err(ProfileError::Status {
            status: status.as_u16(),
            body: snippet(&body),
        });
    }

    serde_json::from_str::<T>(&body).map_err(|error| ProfileError::Decode {
        error: error.to_string(),
        body: snippet(&body),
    })
}

/// Truncate a response body to a short, log-friendly snippet.
fn snippet(body: &str) -> String {
    const MAX: usize = 200;
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "<empty>".to_string();
    }
    match trimmed.char_indices().nth(MAX) {
        Some((end, _)) => format!("{}…", &trimmed[..end]),
        None => trimmed.to_string(),
    }
}

pub async fn fetch_minecraft_profile(uuid: Uuid) -> Result<Profile, ProfileError> {
    let client = http_client()?;
    let uuid_str = uuid.to_string().replace('-', "");
    let url = format!(
        "https://sessionserver.mojang.com/session/minecraft/profile/{uuid_str}?unsigned=false"
    );
    get_json::<Profile>(&client, &url).await
}

impl From<ProfileProperty> for Property {
    fn from(p: ProfileProperty) -> Self {
        Self::new(p.name, p.value, p.signature)
    }
}

/// Resolve the textures [`Property`] for a configured NPC skin.
///
/// `Texture` skins are applied directly with no network access. `Player` skins
/// are resolved from Mojang's session servers at startup. Any failure is logged
/// and yields `None`, so the NPC simply spawns with the default skin instead of
/// blocking server startup.
pub async fn resolve_npc_skin(npc_id: &str, skin: &NpcSkinConfig) -> Option<Property> {
    match skin {
        NpcSkinConfig::Texture { value, signature } => {
            Some(Property::textures(value.as_str(), signature.as_deref()))
        }
        NpcSkinConfig::Player { player } => resolve_player_skin(npc_id, player).await,
    }
}

async fn resolve_player_skin(npc_id: &str, player: &str) -> Option<Property> {
    match try_resolve_player_skin(player).await {
        Ok(property) => Some(property),
        Err(reason) => {
            warn!(
                "failed to resolve skin for NPC '{npc_id}': {reason}; spawning with default skin"
            );
            None
        }
    }
}

async fn try_resolve_player_skin(player: &str) -> Result<Property, String> {
    let uuid = match Uuid::parse_str(player) {
        Ok(uuid) => uuid,
        Err(_) => fetch_uuid_by_name(player)
            .await
            .map_err(|error| format!("could not resolve player '{player}': {error}"))?,
    };

    let profile = fetch_minecraft_profile(uuid)
        .await
        .map_err(|error| error.to_string())?;
    let textures = profile
        .try_get_textures()
        .ok_or_else(|| format!("player '{player}' has no textures"))?;
    Ok(textures.into())
}

#[derive(Deserialize)]
struct NameLookupResponse {
    id: String,
}

async fn fetch_uuid_by_name(name: &str) -> Result<Uuid, ProfileError> {
    let client = http_client()?;
    let url = format!("https://api.mojang.com/users/profiles/minecraft/{name}");
    let response = get_json::<NameLookupResponse>(&client, &url).await?;
    Uuid::parse_str(&response.id).map_err(|error| ProfileError::Decode {
        error: error.to_string(),
        body: response.id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn texture_skin_resolves_offline_without_network() {
        let skin = NpcSkinConfig::Texture {
            value: "dGV4dHVyZXM=".to_string(),
            signature: Some("c2lnbmF0dXJl".to_string()),
        };

        let property = resolve_npc_skin("npc", &skin)
            .await
            .expect("texture skin resolves");

        assert!(property.is_textures());
        assert_eq!(property.value(), "dGV4dHVyZXM=");
        assert_eq!(property.signature().as_deref(), Some("c2lnbmF0dXJl"));
    }

    #[test]
    fn snippet_reports_empty_body() {
        assert_eq!(snippet("   \n  "), "<empty>");
    }

    #[test]
    fn snippet_truncates_long_body() {
        let body = "x".repeat(500);
        let out = snippet(&body);
        assert!(out.ends_with('…'));
        assert_eq!(out.chars().filter(|c| *c == 'x').count(), 200);
    }

    #[test]
    fn snippet_keeps_short_body_verbatim() {
        assert_eq!(snippet("  {\"error\":\"x\"}  "), "{\"error\":\"x\"}");
    }

    #[test]
    fn status_error_shows_code_and_body() {
        let err = ProfileError::Status {
            status: 429,
            body: "<empty>".to_string(),
        };
        assert_eq!(err.to_string(), "Mojang returned HTTP 429 (body: <empty>)");
    }
}
