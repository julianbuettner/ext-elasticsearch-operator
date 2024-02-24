use std::fmt::Display;

use serde::{ser::SerializeSeq, Deserialize, Serialize};

use crate::UserPermissions;

#[derive(Eq, PartialEq, Debug)]
pub struct Privileges {
    read: bool,
    write: bool,
    create: bool,
}

impl From<UserPermissions> for Privileges {
    fn from(value: UserPermissions) -> Self {
        match value {
            UserPermissions::Read => Privileges::new().enable_read(),
            UserPermissions::Write => Privileges::new().enable_read().enable_write(),
            UserPermissions::Create => Privileges::new()
                .enable_read()
                .enable_write()
                .enable_create(),
        }
    }
}

impl Default for Privileges {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for Privileges {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // e.g. Read, Write
        let arr = [
            ("read", self.read),
            ("write", self.write),
            ("create", self.create),
        ];
        let s: Vec<&'static str> = arr
            .iter()
            .filter(|(_, cond)| *cond)
            .map(|(name, _)| *name)
            .collect();
        write!(f, "{}", s.join(", "))
    }
}

impl Privileges {
    pub fn new() -> Self {
        Self {
            read: false,
            write: false,
            create: false,
        }
    }
    pub fn enable_read(mut self) -> Self {
        self.read = true;
        self
    }
    pub fn enable_write(mut self) -> Self {
        self.write = true;
        self
    }
    pub fn enable_create(mut self) -> Self {
        self.create = true;
        self
    }
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct IndexPermission {
    pub names: Vec<String>,
    pub privileges: Privileges,
}

impl Display for IndexPermission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let indices: Vec<String> = self.names.iter().map(ToString::to_string).collect();
        write!(f, "[{}] on [{}]", self.privileges, indices.join(", "))
    }
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct Role {
    pub indices: Vec<IndexPermission>,
}

impl Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let indices: Vec<String> = self.indices.iter().map(|x| x.to_string()).collect();
        write!(f, "{}", indices.join("; "))
    }
}

impl Serialize for Privileges {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let len = self.read as usize + self.write as usize + self.create as usize;
        let mut seq = serializer.serialize_seq(Some(len))?;
        if self.read {
            seq.serialize_element("read")?;
        }
        if self.write {
            seq.serialize_element("write")?;
        }
        if self.create {
            seq.serialize_element("create")?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for Privileges {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let permission_array: Vec<String> = Vec::<String>::deserialize(deserializer)?;
        let mut permissions = Privileges::new();
        for p in permission_array {
            match p.as_str() {
                "read" => permissions.read = true,
                "write" => permissions.write = true,
                "create" => permissions.create = true,
                other => {
                    return Err(serde::de::Error::invalid_value(
                        serde::de::Unexpected::Str(other),
                        &"Permissions must be read, write or create",
                    ))
                }
            }
        }
        Ok(permissions)
    }
}
