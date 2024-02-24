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

impl User {
    pub fn is_same(&self, old: &Self) -> bool {
        self.roles == old.roles && self.full_name == old.full_name && self.email == old.email
    }
    pub fn delta_string(&self, old: &Self) -> Option<String> {
        let mut diffs: Vec<String> = Vec::new();
        if self.roles != old.roles {
            diffs.push(format!(
                "[Roles {} => {}]",
                self.roles.join(", "),
                old.roles.join(", ")
            ));
        }
        if self.full_name != old.full_name {
            diffs.push(format!(
                "[Name {} => {}]",
                old.full_name.as_ref().unwrap_or(&"<undefined>".into()),
                self.full_name.as_ref().unwrap_or(&"<undefined>".into()),
            ));
        }
        if self.email != old.email {
            diffs.push(format!(
                "[Email {} => {}]",
                old.email.as_ref().unwrap_or(&"<undefined>".into()),
                self.email.as_ref().unwrap_or(&"<undefined>".into()),
            ));
        }
        if diffs.is_empty() {
            None
        } else {
            Some(diffs.join(" "))
        }
    }
}
