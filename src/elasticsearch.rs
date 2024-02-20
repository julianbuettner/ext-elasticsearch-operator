mod error;
mod role;
mod user;
use std::{fmt::Display, time::Duration};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use log::{debug, warn};
use reqwest::{
    header::{self, HeaderMap, HeaderValue},
    Client,
};
use serde::Deserialize;

pub use error::ElasticError;
pub use role::{IndexPermission, Privileges, Role};
pub use user::User;

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct ElasticAdmin {
    pub url: String,
    client: Client,
    skip_verify: bool,
}

fn username_password_to_basic(username: impl Display, password: impl Display) -> String {
    let basic_auth_b64 =
        STANDARD.encode(format!("{}:{}", username, password));
    format!("Basic {}", basic_auth_b64)
}

impl ElasticAdmin {
    pub fn new(
        url: &str,
        username: impl ToString,
        password: impl ToString,
        skip_verify: bool,
    ) -> Self {
        let url = url.trim_end_matches('/');
        let mut default_header_map = HeaderMap::new();
        default_header_map.insert(
            "Content-Type",
            HeaderValue::from_str("Application/Json").unwrap(),
        );
        let mut auth_value = HeaderValue::from_str(&username_password_to_basic(
            username.to_string(),
            password.to_string(),
        ))
        .unwrap();
        auth_value.set_sensitive(true);
        default_header_map.insert(header::AUTHORIZATION, auth_value);
        Self {
            url: url.to_string(),
            client: Client::builder()
                .timeout(Duration::from_millis(5_000))
                .danger_accept_invalid_certs(skip_verify)
                .default_headers(default_header_map)
                .user_agent(format!("ext-elasticsearch-operator/{}", VERSION))
                .build()
                .expect("Unexpected error in building HTTP Client"),
            skip_verify,
        }
    }
    pub fn clone_with_new_login(&self, username: impl Display, password: impl Display) -> Self {
        // TODO reuse Client?
        Self::new(&self.url, username, password, self.skip_verify)
    }
    fn format_url(&self, uri: impl std::fmt::Display) -> String {
        format!("{}{}", self.url, uri)
    }
    pub async fn connection_ok(&self) -> Result<(), ElasticError> {
        let req = self
            .client
            .get(self.format_url("/_security/_authenticate"))
            .send();

        let res = req.await?;
        if res.status().as_u16() == 401 {
            return Err(ElasticError::WrongCredentials);
        }

        #[derive(Deserialize, Debug)]
        struct UserDetails {
            roles: Vec<String>,
        }

        let body: UserDetails = res.json().await?;
        if !body.roles.contains(&"superuser".into()) {
            return Err(ElasticError::NotSuperuser);
        }
        Ok(())
    }
    /// Create a role. If the role already exists
    /// (identified by name), the permissions are
    /// overwritten. This way, we don't need a seperate
    /// put or patch.
    pub async fn create_role(&self, name: impl Display, role: Role) -> Result<(), ElasticError> {
        let res = self
            .client
            .post(self.format_url(format!("/_security/role/{}", name)))
            .json(&role)
            .send()
            .await?;
        debug!("Status code creating role {}: {}", name, res.status());
        Ok(())
    }
    pub async fn delete_role(&self, name: impl Display) -> Result<(), ElasticError> {
        let res = self
            .client
            .delete(self.format_url(format!("/_security/role/{}", name)))
            .send()
            .await?;
        debug!("Status code of deleting role {}: {}", name, res.status());
        if res.status().as_u16() == 404 {
            return Err(ElasticError::RoleNotfound(format!("{}", name)));
        }
        if !res.status().is_success() {
            return Err(ElasticError::Custom(format!(
                "Error deleting role: {}",
                res.text().await?
            )));
        }
        Ok(())
    }
    pub async fn get_role(&self, name: impl Display) -> Result<Role, ElasticError> {
        let res = self
            .client
            .get(self.format_url(format!("/_security/role/{}", name)))
            .send()
            .await?;
        Ok(res.json().await?)
    }
    pub async fn create_user(
        &self,
        username: impl Display,
        user: &User,
    ) -> Result<(), ElasticError> {
        let res = self
            .client
            .post(self.format_url(format!("/_security/user/{}", username)))
            .json(user)
            .send()
            .await?;
        debug!("Status code creating user {}: {}", username, res.status());
        if !res.status().is_success() {
            return Err(ElasticError::Custom(format!(
                "Error creating user {}: {}",
                username,
                res.text().await?
            )));
        }
        Ok(())
    }
    pub async fn get_user(&self, username: impl Display, user: &User) -> Result<(), ElasticError> {
        let res = self
            .client
            .get(self.format_url(format!("/_security/user/{}", username)))
            .json(user)
            .send()
            .await?;
        if res.status().as_u16() == 404 {
            return Err(ElasticError::UserNotfound(username.to_string()));
        }
        if !res.status().is_success() {
            return Err(ElasticError::Custom(format!(
                "Error getting user {}: {}",
                username,
                res.text().await?
            )));
        }
        warn!("USER: {:?}", res.text().await?);
        Ok(())
    }
    pub async fn delete_user(&self, name: impl Display) -> Result<(), ElasticError> {
        let res = self
            .client
            .delete(self.format_url(format!("/_security/user/{}", name)))
            .send()
            .await?;
        debug!("Status code of deleting user {}: {}", name, res.status());
        if res.status().as_u16() == 404 {
            return Err(ElasticError::UserNotfound(format!("{}", name)));
        }
        if !res.status().is_success() {
            return Err(ElasticError::Custom(format!(
                "Error deleting user: {}",
                res.text().await?
            )));
        }
        Ok(())
    }
    
}
