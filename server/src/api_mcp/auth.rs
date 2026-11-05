//! Bearer-token authenticator for the plan-ai-api-mcp surface. Tokens live in
//! the `tokens` table; the bearer is sha256-hashed and looked up.
//!
//! [`BearerAuthenticator`] tries that first and, when `[zitadel_bearer]` is
//! configured, falls back to Zitadel introspection: a signed-in user's Zitadel
//! access token acts as that wg-vpng user ([`VpnUsers`]), created on first use
//! as a first web sign-in would.

use std::collections::HashSet;

use std::sync::Arc;

use async_trait::async_trait;
use plan_ai_api_mcp::zitadel::{ClientAuth, Identity, IdentityMapper, IntrospectionConfig, ZitadelAuthenticator};
use plan_ai_api_mcp::{ApiError, Authenticator, OrgSet, Principal};
use plan_ai_auth::{ProviderMeta, SignIn, SignInError, groups_in_claims};
use sqlx::PgPool;
use uuid::Uuid;

pub struct TokenAuthenticator {
    pool: PgPool,
}

impl TokenAuthenticator {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Hex sha256 of a bearer token — the value stored in `tokens.token_hash`.
pub fn hash_token(bearer: &str) -> String {
    use sha2::{Digest, Sha256};
    hex_encode(&Sha256::digest(bearer.as_bytes()))
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[async_trait]
impl Authenticator for TokenAuthenticator {
    async fn authenticate(&self, bearer: &str) -> Result<Principal, ApiError> {
        self.lookup(bearer)
            .await?
            .ok_or_else(|| ApiError::unauthorized("invalid or expired token"))
    }
}

impl TokenAuthenticator {
    /// The principal of a live `tokens` row with this bearer; `None` when there is none.
    pub async fn lookup(&self, bearer: &str) -> Result<Option<Principal>, ApiError> {
        let hash = hash_token(bearer);
        let row = sqlx::query_as::<_, (String, Option<Uuid>)>(
            "SELECT kind, organization_id FROM tokens \
             WHERE token_hash = $1 AND NOT revoked \
               AND (expires_at IS NULL OR expires_at > now())",
        )
        .bind(&hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
        let Some((kind, org)) = row else {
            return Ok(None);
        };
        match kind.as_str() {
            "admin" => Ok(Some(Principal::admin("token:admin"))),
            "api" => match org {
                Some(oid) => {
                    let one = HashSet::from([oid]);
                    Ok(Some(Principal {
                        admin: false,
                        read_orgs: OrgSet::Only(one.clone()),
                        write_orgs: OrgSet::Only(one),
                        scopes: serde_json::Value::Null,
                        subject: format!("token:api:{oid}"),
                    }))
                }
                None => Ok(Some(Principal {
                    admin: false,
                    read_orgs: OrgSet::All,
                    write_orgs: OrgSet::Only(HashSet::new()),
                    scopes: serde_json::Value::Null,
                    subject: "token:api:global-ro".into(),
                })),
            },
            other => Err(ApiError::forbidden(format!(
                "token kind '{other}' is not permitted on this API"
            ))),
        }
    }
}

/// The API's authenticator: wg-vpng's own tokens first, then — when
/// configured — Zitadel access tokens of signed-in users.
pub struct BearerAuthenticator {
    tokens: TokenAuthenticator,
    zitadel: Option<ZitadelAuthenticator>,
}

impl BearerAuthenticator {
    pub fn new(pool: PgPool, cfg: Option<&crate::config::ZitadelBearerConfig>) -> Self {
        let zitadel = cfg.map(|z| {
            let introspection = IntrospectionConfig {
                issuer: z.issuer.clone(),
                introspection_endpoint: None,
                client: ClientAuth::Basic { client_id: z.client_id.clone(), client_secret: z.client_secret.clone() },
                project_id: z.project_id.clone(),
                cache_secs: 60,
            };
            let users = Arc::new(VpnUsers::new(pool.clone(), z));
            tracing::info!(issuer = %z.issuer, project_id = %z.project_id, auto_create = z.auto_create,
                "API accepts Zitadel bearer tokens");
            ZitadelAuthenticator::new(introspection, users).unwrap_or_else(|e| panic!("[zitadel_bearer]: {e}"))
        });
        Self { tokens: TokenAuthenticator::new(pool), zitadel }
    }
}

#[async_trait]
impl Authenticator for BearerAuthenticator {
    async fn authenticate(&self, bearer: &str) -> Result<Principal, ApiError> {
        if let Some(p) = self.tokens.lookup(bearer).await? {
            return Ok(p);
        }
        let Some(zitadel) = &self.zitadel else {
            return Err(ApiError::unauthorized("invalid or expired token"));
        };
        // Zitadel unreachable or refusing our credentials is, to the caller, a
        // bearer nobody could verify: 401, not a 500.
        zitadel.authenticate(bearer).await.map_err(|e| match e {
            ApiError::Internal(why) => {
                tracing::warn!(reason = %why, "zitadel bearer could not be verified");
                ApiError::unauthorized("invalid or expired token (not a wg-vpng token, and Zitadel could not verify it)")
            }
            other => other,
        })
    }
}

/// Maps a Zitadel identity to the wg-vpng user with its email, creating it
/// through the web sign-in (`ProviderMeta::sign_in`: its access rules, its
/// account creation) when it has none — as plan-ai-cloud's console does.
pub struct VpnUsers {
    pool: PgPool,
    sign_up: Option<ProviderMeta>,
    liveliness_days: Option<u32>,
}

impl VpnUsers {
    pub fn new(pool: PgPool, cfg: &crate::config::ZitadelBearerConfig) -> Self {
        let auth = crate::config::config().auth.as_ref();
        let provider = provider_for(&cfg.issuer, auth);
        Self {
            pool,
            liveliness_days: provider.liveliness_days,
            sign_up: cfg.auto_create.then_some(provider),
        }
    }

    async fn sign_up(&self, email: &str, identity: &Identity) -> Result<Uuid, ApiError> {
        let Some(provider) = &self.sign_up else {
            return Err(ApiError::forbidden(format!("{email} has no wg-vpng account — sign in to it once first")));
        };
        if identity.email_verified != Some(true) {
            return Err(ApiError::forbidden(format!(
                "{email} has no wg-vpng account, and none is made for an address Zitadel does not call verified"
            )));
        }
        let claims = serde_json::Value::Object(identity.claims.clone());
        let sign_in = SignIn {
            email,
            name: identity.name.as_deref(),
            groups: provider.groups_claim.as_deref().map(|c| groups_in_claims(&claims, c)).unwrap_or_default(),
            zitadel_org: identity.org_id.as_deref(),
        };
        let resolver = crate::web::auth::PgUserResolver::new(self.pool.clone(), provider.admin_emails.clone());
        match provider.sign_in(resolver.as_ref(), &sign_in).await {
            Ok(user) => {
                tracing::info!(email, user_id = %user.id, zitadel_user_id = %identity.user_id,
                    "wg-vpng account created for a Zitadel bearer token");
                Ok(user.id)
            }
            Err(SignInError::Refused(why)) => Err(ApiError::forbidden(format!(
                "{email} has no wg-vpng account, and signing in would be refused: {why}"
            ))),
            Err(SignInError::Failed(e)) => Err(ApiError::internal(format!("could not create the account for {email}: {e:#}"))),
        }
    }
}

/// The `[[auth.providers]]` entry with `issuer`, or that issuer with `allow_all`.
fn provider_for(issuer: &str, auth: Option<&plan_ai_auth::AuthConfig>) -> ProviderMeta {
    auth.and_then(|a| a.providers.iter().map(|p| ProviderMeta::from_config(p, a)).find(|p| p.has_issuer(issuer)))
        .unwrap_or_else(|| ProviderMeta {
            slug: "zitadel_bearer".into(),
            name: "Zitadel bearer".into(),
            issuer: Some(issuer.into()),
            allow_all: true,
            allowed_domains: vec![],
            allowed_emails: vec![],
            auto_join_orgs: vec![],
            admin_emails: auth.map(|a| a.admin_emails.clone()).unwrap_or_default(),
            groups_claim: None,
            liveliness_days: None,
            zitadel_org: None,
        })
}

#[async_trait]
impl IdentityMapper for VpnUsers {
    async fn principal(&self, identity: &Identity) -> Result<Principal, ApiError> {
        let email = identity.email.as_deref().filter(|e| !e.is_empty()).ok_or_else(|| {
            ApiError::unauthorized("the token carries no email; request it with the `email` scope")
        })?;
        if identity.email_verified == Some(false) {
            return Err(ApiError::forbidden(format!("{email} is not verified in Zitadel")));
        }
        let existing = crate::store::get_user_by_email(&self.pool, email).await.map_err(|e| ApiError::internal(e.to_string()))?;
        let id = match existing {
            Some(u) if u.banned => return Err(ApiError::forbidden(format!("{email} is banned"))),
            Some(u) => {
                // Using the API is activity, as signing in is: it keeps the account live.
                crate::store::record_login(&self.pool, u.id, self.liveliness_days.or(Some(30)))
                    .await
                    .map_err(|e| ApiError::internal(e.to_string()))?;
                u.id
            }
            None => self.sign_up(email, identity).await?,
        };
        let user = crate::store::get_user(&self.pool, id)
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?
            .ok_or_else(|| ApiError::internal("the account vanished"))?;
        Ok(user_principal(&user, identity.actor.as_ref().map(|a| a.sub.as_str())))
    }
}

/// A signed-in user's principal: admin as in the web UI, otherwise limited to
/// their own devices (`scopes.user_id`, see [`acting_user`]).
pub fn user_principal(user: &crate::store::User, actor: Option<&str>) -> Principal {
    Principal {
        admin: user.is_admin,
        read_orgs: OrgSet::Only(HashSet::new()),
        write_orgs: OrgSet::Only(HashSet::new()),
        scopes: serde_json::json!({ "user_id": user.id, "email": user.email, "actor": actor }),
        subject: match actor {
            Some(a) => format!("{} (act: {a})", user.email),
            None => user.email.clone(),
        },
    }
}

/// The user a non-admin principal acts as, whose own devices are all it may
/// touch. `None` for admins and for admin-minted `tokens` rows.
pub fn acting_user(p: &Principal) -> Option<(Uuid, String)> {
    if p.admin {
        return None;
    }
    let id = p.scopes.get("user_id")?.as_str()?.parse().ok()?;
    let email = p.scopes.get("email")?.as_str()?.to_string();
    Some((id, email))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_hex_sha256() {
        // sha256("admin") — matches the seed used in the NixOS test.
        assert_eq!(
            hash_token("admin"),
            "8c6976e5b5410415bde908bd4dee15dfb167a9c873fc4bb8a81f6f2ab448a918"
        );
    }

    #[test]
    fn a_user_acts_as_themselves_and_an_admin_as_admin() {
        let mut user = crate::store::User {
            id: Uuid::from_u128(7),
            email: "a@plan.ai".into(),
            name: "A".into(),
            is_admin: false,
            banned: false,
            access_revoked: false,
        };
        let p = user_principal(&user, Some("chat-connector"));
        assert_eq!(acting_user(&p), Some((Uuid::from_u128(7), "a@plan.ai".into())));
        assert_eq!(p.subject, "a@plan.ai (act: chat-connector)");
        user.is_admin = true;
        assert_eq!(acting_user(&user_principal(&user, None)), None);
        // Admin-minted API tokens keep their unrestricted reads.
        assert_eq!(acting_user(&Principal::admin("token:admin")), None);
    }
}
