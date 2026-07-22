//! Bearer-token authenticator for the plan-ai-api-mcp surface. Tokens live in
//! the `tokens` table; the bearer is sha256-hashed and looked up.

use std::collections::HashSet;

use async_trait::async_trait;
use plan_ai_api_mcp::{ApiError, Authenticator, OrgSet, Principal};
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
        let hash = hash_token(bearer);
        let row = sqlx::query_as::<_, (String, Option<Uuid>)>(
            "SELECT kind, organization_id FROM tokens \
             WHERE token_hash = $1 AND NOT revoked \
               AND (expires_at IS NULL OR expires_at > now())",
        )
        .bind(&hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
        .ok_or_else(|| ApiError::unauthorized("invalid or expired token"))?;

        let (kind, org) = row;
        match kind.as_str() {
            "admin" => Ok(Principal::admin("token:admin")),
            "api" => match org {
                Some(oid) => {
                    let one = HashSet::from([oid]);
                    Ok(Principal {
                        admin: false,
                        read_orgs: OrgSet::Only(one.clone()),
                        write_orgs: OrgSet::Only(one),
                        scopes: serde_json::Value::Null,
                        subject: format!("token:api:{oid}"),
                    })
                }
                None => Ok(Principal {
                    admin: false,
                    read_orgs: OrgSet::All,
                    write_orgs: OrgSet::Only(HashSet::new()),
                    scopes: serde_json::Value::Null,
                    subject: "token:api:global-ro".into(),
                }),
            },
            other => Err(ApiError::forbidden(format!(
                "token kind '{other}' is not permitted on this API"
            ))),
        }
    }
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
}
