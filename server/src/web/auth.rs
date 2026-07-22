//! OIDC user resolver: upsert users from their OIDC identity into our `users`
//! table and load memberships. Installed globally via `set_user_resolver`.

#![cfg(feature = "server")]

use std::sync::Arc;

use async_trait::async_trait;
use plan_ai_auth::{OrgMembership, UserResolver, WebUser};
use sqlx::PgPool;
use uuid::Uuid;

pub use plan_ai_auth::{build_auth_layers, login_page, logout_handler, require_auth};

pub struct PgUserResolver {
    pool: PgPool,
    admin_emails: Vec<String>,
}

impl PgUserResolver {
    pub fn new(pool: PgPool, admin_emails: Vec<String>) -> Arc<Self> {
        Arc::new(Self { pool, admin_emails })
    }
}

/// Build and install the resolver from config.
pub fn install_resolver(pool: PgPool) {
    let admin_emails = crate::config::config()
        .auth
        .as_ref()
        .map(|a| a.admin_emails.clone())
        .unwrap_or_default();
    plan_ai_auth::set_user_resolver(PgUserResolver::new(pool, admin_emails));
}

#[async_trait]
impl UserResolver for PgUserResolver {
    async fn resolve_user(
        &self,
        email: &str,
        name: Option<&str>,
        _admin_emails: &[String],
        auto_join_orgs: &[String],
    ) -> Result<WebUser, anyhow::Error> {
        let is_admin = self.admin_emails.iter().any(|e| e == email);
        let display_name = name.unwrap_or("");

        let (id, email, name, is_admin) = sqlx::query_as::<_, (Uuid, String, String, bool)>(
            "INSERT INTO users (email, name, is_admin) VALUES ($1,$2,$3) \
             ON CONFLICT (email) DO UPDATE SET name = EXCLUDED.name, \
               is_admin = users.is_admin OR $3 \
             RETURNING id, email, name, is_admin",
        )
        .bind(email)
        .bind(display_name)
        .bind(is_admin)
        .fetch_one(&self.pool)
        .await?;

        for org_name in auto_join_orgs {
            if let Some(org_id) = sqlx::query_scalar::<_, Uuid>("SELECT id FROM organizations WHERE name = $1")
                .bind(org_name)
                .fetch_optional(&self.pool)
                .await?
            {
                sqlx::query(
                    "INSERT INTO organization_members (organization_id, user_id, role) \
                     VALUES ($1,$2,'read') ON CONFLICT DO NOTHING",
                )
                .bind(org_id)
                .bind(id)
                .execute(&self.pool)
                .await?;
            }
        }

        let org_memberships = sqlx::query_as::<_, (Uuid, String)>(
            "SELECT organization_id, role FROM organization_members WHERE user_id = $1",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|(org_id, role)| OrgMembership { org_id, role })
        .collect();

        Ok(WebUser { id, email, name, is_admin, org_memberships, impersonating_from: None })
    }

    async fn load_user_by_id(&self, id: Uuid) -> Result<Option<WebUser>, anyhow::Error> {
        let Some((id, email, name, is_admin)) =
            sqlx::query_as::<_, (Uuid, String, String, bool)>(
                "SELECT id, email, name, is_admin FROM users WHERE id = $1",
            )
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
        else {
            return Ok(None);
        };

        let org_memberships = sqlx::query_as::<_, (Uuid, String)>(
            "SELECT organization_id, role FROM organization_members WHERE user_id = $1",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|(org_id, role)| OrgMembership { org_id, role })
        .collect();

        Ok(Some(WebUser { id, email, name, is_admin, org_memberships, impersonating_from: None }))
    }
}
