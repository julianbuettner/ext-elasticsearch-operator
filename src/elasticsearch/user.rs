use std::collections::HashMap;

use serde::Serialize;

#[derive(Serialize, Default)]
pub struct User {
    pub password: Option<String>,
    pub roles: Vec<String>,
    pub full_name: Option<String>,
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}
