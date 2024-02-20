use std::{
    collections::{BTreeMap, HashMap},
    str::from_utf8,
};

use k8s_openapi::{
    api::core::v1::Secret,
    ByteString,
};
use kube::{
    api::{DeleteParams, PatchParams, PostParams}, Api, Client,
};
use log::{debug, info};
use passwords::PasswordGenerator;

use crate::{
    elasticsearch::{ElasticAdmin, IndexPermission, Role, User},
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
    let secret = match secret_api.get(&user.spec.secret_ref).await {
        Err(kube::Error::Api(err)) if err.code == 404 => {
            // TODO Set ownership of secret
            let mut secret = Secret::default();
            debug!("Secret {} does not exist, create.", user.spec.secret_ref);
            secret.metadata.name = Some(user.spec.secret_ref.clone());
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

pub async fn ensure_user_exists(
    user: &ElasticsearchUser,
    client: &Client,
    elastic: &ElasticAdmin,
) -> Result<(), OperatorError> {
    let secret = ensure_secret_existance_and_correctness(user, client, elastic).await?;
    // No unwrap should fail here, by ensure_secret_existance_and_correctness
    let username = from_utf8(&secret.data.as_ref().unwrap().get(SECRET_USER).unwrap().0).unwrap();
    let password = from_utf8(&secret.data.as_ref().unwrap().get(SECRET_PASS).unwrap().0).unwrap();
    // let user_elastic = elastic.clone_with_new_login(username, password);

    elastic
        .create_role(
            format!("role-{}", username),
            Role {
                indices: vec![IndexPermission {
                    names: user
                        .spec
                        .prefixes
                        .iter()
                        .map(|pre| format!("{}*", pre))
                        .collect(),
                    privileges: user.spec.permissions.into(),
                }],
            },
        )
        .await?;
    elastic
        .create_user(
            username,
            &User {
                password: Some(password.into()),
                roles: vec![format!("role-{}", username)],
                full_name: None,
                email: None,
                metadata: Some(HashMap::from([(
                    "created-by".to_string(),
                    "K8s Operator eeops".to_string(),
                )])),
            },
        )
        .await?;
    Ok(())
}

pub async fn delete_user(
    user: &ElasticsearchUser,
    client: &Client,
    elastic: &ElasticAdmin,
) -> Result<(), OperatorError> {
    let username = &user.spec.username;
    elastic.delete_user(&username).await?;
    elastic.delete_role(&username).await?;
    let secret_api: Api<Secret> = Api::default_namespaced(client.clone());
    secret_api.delete(user.spec.secret_ref.as_str(), &DeleteParams::default()).await?;
    Ok(())
}
