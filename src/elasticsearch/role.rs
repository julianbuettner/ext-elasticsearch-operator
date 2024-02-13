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

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct Role {
    pub indices: Vec<IndexPermission>,
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

#[derive(Deserialize)]
struct FakePermissions {
    identifiers: Vec<String>,
}

impl<'de> Deserialize<'de> for Privileges {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let fake_permissions: FakePermissions = FakePermissions::deserialize(deserializer)?;
        let mut permissions = Privileges::new();
        for p in fake_permissions.identifiers {
            match p.as_str() {
                "read" => permissions.read = true,
                "write" => permissions.write = true,
                "create" => permissions.create = true,
                other => {
                    return Err(serde::de::Error::invalid_value(
                        serde::de::Unexpected::Str(other),
                        &"read, write or create",
                    ))
                }
            }
        }
        Ok(permissions)
    }
}
