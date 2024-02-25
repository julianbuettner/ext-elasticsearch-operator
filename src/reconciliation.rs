use std::{
    collections::{BTreeMap, HashMap},
    str::from_utf8,
};

use k8s_openapi::{
    api::core::v1::Secret, apimachinery::pkg::apis::meta::v1::OwnerReference, ByteString,
};
use kube::{
    api::{PatchParams, PostParams},
    Api, Client, ResourceExt,
};
use log::{debug, info};
use passwords::PasswordGenerator;

use crate::{
    elasticsearch::{ElasticAdmin, ElasticError, IndexPermission, Role, User},
    error::OperatorError,
    ElasticsearchUser, PASSWORD_LENGTH, SECRET_PASS, SECRET_URL, SECRET_USER,
};

fn generate_password() -> String {
    let pg = PasswordGenerator {
        length: PASSWORD_LENGTH,
        numbers: true,
        lowercase_letters: true,
        uppercase_letters: true,
        symbols: false,
        spaces: false,
        exclude_similar_characters: false,
        strict: true,
    };
    pg.generate_one().unwrap()
}

fn parse_bytes(b: &[u8]) -> Option<&str> {
    from_utf8(b).ok()
}

async fn ensure_secret_existance_and_correctness(
    user: &ElasticsearchUser,
    client: &Client,
    elastic: &ElasticAdmin,
) -> Result<Secret, OperatorError> {
    // TODO user secret.string_data
    let secret_api: Api<Secret> = Api::default_namespaced(client.clone());
    let ownership = OwnerReference {
        api_version: "eeops.io/v1".into(),
        name: user.name_any(),
        uid: user.uid().unwrap_or("".into()),
        kind: "ElasticsearchUser".into(),
        controller: None,
        block_owner_deletion: None,
    };
    let secret = match secret_api.get(&user.spec.secret_ref).await {
        Err(kube::Error::Api(err)) if err.code == 404 => {
            // TODO Set ownership of secret
            let mut secret = Secret::default();
            debug!("Secret {} does not exist, create.", user.spec.secret_ref);
            secret.metadata.name = Some(user.spec.secret_ref.clone());
            *secret.owner_references_mut() = vec![ownership];
            secret.data = Some(BTreeMap::from([
                (
                    SECRET_USER.to_string(),
                    ByteString(user.spec.username.clone().into_bytes()),
                ),
                (
                    SECRET_PASS.to_string(),
                    ByteString(generate_password().into()),
                ),
                (
                    SECRET_URL.to_string(),
                    ByteString(elastic.url.clone().into_bytes()),
                ),
            ]));
            secret_api.create(&PostParams::default(), &secret).await?;
            Ok(secret)
        }
        Err(e) => Err(e),
        Ok(mut secret) => {
            let mut value_changed = false;
            if secret.data.is_none() {
                secret.data = Some(BTreeMap::new());
                value_changed = true;
            }
            *secret.owner_references_mut() = vec![ownership];
            if secret.data.as_ref().unwrap().get(SECRET_URL)
                != Some(&ByteString(elastic.url.clone().into_bytes()))
            {
                info!(
                    "Secret {} had URL {}. Set to {}, as configured in the operator.",
                    user.spec.secret_ref,
                    secret
                        .data
                        .as_ref()
                        .unwrap()
                        .get(SECRET_URL)
                        .map(|b| parse_bytes(&b.0).unwrap_or("<undefined>"))
                        .unwrap_or("<binary>"),
                    elastic.url,
                );
                secret.data.as_mut().unwrap().insert(
                    SECRET_URL.to_string(),
                    ByteString(elastic.url.clone().into_bytes()),
                );
                value_changed = true;
            }
            if secret.data.as_ref().unwrap().get(SECRET_USER)
                != Some(&ByteString(user.spec.username.clone().into_bytes()))
            {
                info!(
                    "Secret {} had user {}. Set to {}, as specified in CR {}.",
                    user.spec.secret_ref,
                    secret
                        .data
                        .as_ref()
                        .unwrap()
                        .get(SECRET_USER)
                        .map(|b| parse_bytes(&b.0).unwrap_or("<undefined>"))
                        .unwrap_or("<binary>"),
                    user.spec.username,
                    user.metadata
                        .name
                        .as_ref()
                        .unwrap_or(&"<no name set>".into()),
                );
                secret.data.as_mut().unwrap().insert(
                    SECRET_USER.to_string(),
                    ByteString(user.spec.username.clone().into_bytes()),
                );
                value_changed = true;
            }
            if secret.data.as_ref().unwrap().get(SECRET_PASS).is_none() {
                info!(
                    "Secret {} was missing a password. Set a random one. (CR {}).",
                    user.spec.secret_ref,
                    user.metadata
                        .name
                        .as_ref()
                        .unwrap_or(&"<no name set>".to_string()),
                );
                secret.data.as_mut().unwrap().insert(
                    SECRET_USER.to_string(),
                    ByteString(generate_password().into_bytes()),
                );
                value_changed = true;
            }
            if value_changed {
                secret_api
                    .patch(
                        &user.spec.secret_ref,
                        &PatchParams::default(),
                        &kube::api::Patch::Apply(secret.clone()),
                    )
                    .await?;
            }
            Ok(secret)
        }
    }?;
    Ok(secret)
}

pub async fn apply_user(
    user: &ElasticsearchUser,
    client: &Client,
    elastic: &ElasticAdmin,
) -> Result<(), OperatorError> {
    let secret = ensure_secret_existance_and_correctness(user, client, elastic).await?;
    // No unwrap should fail here, by ensure_secret_existance_and_correctness
    let username = from_utf8(&secret.data.as_ref().unwrap().get(SECRET_USER).unwrap().0).unwrap();
    let password = from_utf8(&secret.data.as_ref().unwrap().get(SECRET_PASS).unwrap().0).unwrap();
    // let user_elastic = elastic.clone_with_new_login(username, password);

    let target_role = Role {
        indices: vec![IndexPermission {
            names: user
                .spec
                .prefixes
                .iter()
                .map(|pre| format!("{}*", pre))
                .collect(),
            privileges: user.spec.permissions.into(),
        }],
    };
    let role_name = format!("role-{}", username);
    let target_user = User {
        password: Some(password.into()),
        roles: vec![role_name.clone()],
        full_name: None,
        email: None,
        metadata: Some(HashMap::from([(
            "created-by".to_string(),
            "K8s Operator eeops".to_string(),
        )])),
    };

    match elastic.get_role(role_name.as_str()).await? {
        None => {
            info!("Created role {} {}", role_name, target_role);
            elastic.create_role(role_name, &target_role).await?;
        }
        Some(role) if role == target_role => (),
        Some(old) => {
            info!("Update role {} from {} to {}", role_name, old, target_role);
            elastic.create_role(role_name, &target_role).await?;
        }
    };

    match elastic.get_user(username).await? {
        None => {
            info!("Create user {}", username);
            elastic.create_user(username, &target_user).await?;
        }
        Some(old_user) => match target_user.delta_string(&old_user) {
            None => (),
            Some(description) => {
                info!("Update user {}: {}", username, description);
                elastic.create_user(username, &target_user).await?;
            }
        },
    };

    let user_elastic = elastic.clone_with_new_login(username, password);
    match user_elastic.get_self().await {
        Err(ElasticError::WrongCredentials) => {
            info!("Update credentials of user {}", username);
            elastic.create_user(username, &target_user).await?;
        }
        Ok(_) => (),
        Err(e) => Err(e)?,
    }

    Ok(())
}

pub async fn cleanup_user(
    user: &ElasticsearchUser,
    _client: &Client,
    elastic: &ElasticAdmin,
) -> Result<(), OperatorError> {
    let username = &user.spec.username;
    let role_name = format!("role-{}", username);
    if elastic.delete_user(&username).await? {
        info!("Deleted user {}", username);
    }
    if elastic.delete_role(&role_name).await? {
        info!("Deleted role {}", username);
    }
    // Secret gets deleted automatically due to correctly set
    // ownership
    Ok(())
}
