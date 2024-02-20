use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default, Debug, Eq, PartialEq)]
pub struct User {
    pub password: Option<String>,
    pub roles: Vec<String>,
    pub full_name: Option<String>,
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(skip_deserializing)]
    pub metadata: Option<HashMap<String, String>>,
}
