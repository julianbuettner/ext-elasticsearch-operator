#![deny(clippy::all)]
use std::{
    process::exit,
    sync::Arc,
    time::{Duration, SystemTime},
};

use elasticsearch::ElasticAdmin;
use error::OperatorError;
use futures_util::StreamExt;
use k8s_openapi::{
    api::core::v1::Secret,
    apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition,
};
use kube::{
    api::{PatchParams, PostParams},
    runtime::{
        controller::Action,
        finalizer::{self, Event},
        watcher, Controller,
    },
    Api, Client, CustomResourceExt, ResourceExt,
};
use kube_derive::CustomResource;
use log::{debug, error, info, warn};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    env::load_env,
    reconciliation::{apply_user, cleanup_user},
};
pub mod elasticsearch;
mod env;
mod error;
mod reconciliation;

pub const KEEP_ANNOTATION: &str = "eeops.io/keep";
pub const PASSWORD_LENGTH: usize = 24;
pub const SECRET_USER: &str = "ELASTICSEARCH_USERNAME";
pub const SECRET_PASS: &str = "ELASTICSEARCH_PASSWORD";
pub const SECRET_URL: &str = "ELASTICSEARCH_URL";
pub const REQUEUE_SECONDS: u64 = 900; // reconcile everything every 15min

#[derive(Deserialize, Serialize, Clone, Copy, Debug, JsonSchema)]
enum UserPermissions {
    Read,
    Write,
    Create,
}

/// Annotate with "eeops.io/keep": "true" to keep elastic search users.
#[derive(CustomResource, Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "eeops.io",
    version = "v1",
    kind = "ElasticsearchUser",
    namespaced
)]
#[kube(status = "ElasticSearchUserStatus")]
#[serde(rename_all = "camelCase")]
struct ElasticsearchUserSpec {
    secret_ref: String,
    username: String,
    prefixes: Vec<String>,
    permissions: UserPermissions,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ElasticSearchUserStatus {
    ok: bool,
    error_message: Option<String>,
}

impl ElasticSearchUserStatus {
    pub fn ok() -> Self {
        Self {
            ok: true,
            error_message: None,
        }
    }
    pub fn err(msg: impl ToString) -> Self {
        Self {
            ok: false,
            error_message: Some(msg.to_string()),
        }
    }
}

fn get_log_level() -> Result<log::LevelFilter, String> {
    let var = std::env::var("LOGLEVEL").map(|e| e.to_lowercase());
    let var = var.as_ref().map(|x| x.as_str());
    match var {
        Err(_) => Err("".to_string()),
        Ok("trace") => Ok(log::LevelFilter::Trace),
        Ok("debug") => Ok(log::LevelFilter::Debug),
        Ok("info") => Ok(log::LevelFilter::Info),
        Ok("warn") | Ok("warning") => Ok(log::LevelFilter::Warn),
        Ok("error") => Ok(log::LevelFilter::Error),
        Ok(unknown) => Err(unknown.to_string()),
    }
}

fn setup_logger() -> Result<(), fern::InitError> {
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{} {} {}] {}",
                humantime::format_rfc3339_seconds(SystemTime::now()),
                record.level(),
                record.target(),
                message
            ))
        })
        .filter(|event| event.target().starts_with("ext_elasticsearch_operator"))
        .level(get_log_level().unwrap_or(log::LevelFilter::Debug))
        .chain(std::io::stdout())
        .apply()?;
    Ok(())
}

async fn load_elastic_search() -> ElasticAdmin {
    let env = load_env();
    if let Err(e) = env {
        error!("Error loading environment: {}", e);
        exit(1);
    }
    let env = env.unwrap();
    info!("Starting External Elasticsearch Operator.");
    let el = ElasticAdmin::new(
        &env.url,
        env.username,
        env.password,
        env.skip_tls_cert_verify,
    );
    if let Err(e) = el.connection_ok().await {
        error!("Error while checking ElasticSearch connection: {}.", e);
        exit(1);
    }
    el
}

pub struct Context {
    pub client: Client,
    pub elastic: ElasticAdmin,
}

async fn reconcile(
    user: Arc<ElasticsearchUser>,
    context: Arc<Context>,
) -> Result<Action, finalizer::Error<OperatorError>> {
    let api: Api<ElasticsearchUser> = Api::default_namespaced(context.client.clone());

    let rec = |event: Event<ElasticsearchUser>| async {
        let api: Api<ElasticsearchUser> = Api::default_namespaced(context.client.clone());

        match event {
            Event::Cleanup(user) => cleanup_user(&user, &context.client, &context.elastic).await?,
            Event::Apply(user) => {
                let result = apply_user(&user, &context.client, &context.elastic).await;
                let mut user = (*user).clone();
                match result {
                    Ok(_) => user.status = Some(ElasticSearchUserStatus::ok()),
                    Err(e) => user.status = Some(ElasticSearchUserStatus::err(e)),
                }
                let pp = PostParams::default();
                api.replace_status(
                    user.name_any().as_str(),
                    &pp,
                    serde_json::to_vec(&user).expect("Serde JSON failed to serialize status"),
                )
                .await?;
            }
        }

        Ok(Action::requeue(Duration::from_secs(REQUEUE_SECONDS)))
    };
    finalizer::finalizer(&api, "ExtElasticOp", user.clone(), rec).await
}

fn error_policy(
    _user: Arc<ElasticsearchUser>,
    _error: &finalizer::Error<OperatorError>,
    _context: Arc<Context>,
) -> Action {
    Action::requeue(Duration::from_secs(REQUEUE_SECONDS))
}

#[tokio::main]
async fn main() {
    setup_logger().expect("Unable to setup logger.");
    match get_log_level() {
        Ok(l) => info!("Loglevel set to {}.", l),
        Err(empty) if empty.is_empty() => info!("LOGLEVEL not set, fall back to debug."),
        Err(other) => warn!(
            "Loglevel \"{}\" unknown [trace, debug, info, warn, error]. Fall back to debug.",
            other
        ),
    }
    let elastic_admin = load_elastic_search().await;
    info!("Connection to Elasticsearch established, credentials for superuser are working.");

    let client = Client::try_default().await;
    if let Err(e) = client {
        error!("Error connecting to kubernetes: {}", e);
        exit(1);
    }
    let client = client.unwrap();
    info!("Connection to Kubernetes API established.");

    let crds: Api<CustomResourceDefinition> = Api::all(client.clone());
    match crds
        .create(&PostParams::default(), &ElasticsearchUser::crd())
        .await
    {
        Ok(_) => info!("ElasticsearchUser CRD created/updates successfully"),
        Err(kube::Error::Api(ae)) if ae.code == 409 => {
            let patch_params = PatchParams::apply("eeops_field_manager").force();
            if let Err(e) = crds
                .patch(
                    ElasticsearchUser::crd_name(),
                    &patch_params,
                    &kube::api::Patch::Apply(ElasticsearchUser::crd()),
                )
                .await
            {
                warn!(
                    "Could not patch already existing CRD ElasticsearchUser: {}",
                    e
                );
                warn!(
                    "If problems persist, consider deleting the CRD and restarting this operator."
                );
            }
            info!(
                "Successfully patched existing CRD {}",
                ElasticsearchUser::crd_name()
            );
        }
        Err(e) => {
            error!("Error posting ElasticsearchUser CRD: {}", e);
            exit(1);
        }
    }

    let elastic_users: Api<ElasticsearchUser> = Api::default_namespaced(client.clone());
    let secret_api: Api<Secret> = Api::default_namespaced(client.clone());
    let context = Arc::new(Context {
        elastic: elastic_admin,
        client,
    });
    Controller::new(elastic_users, watcher::Config::default())
        .shutdown_on_signal()
        .owns(secret_api, watcher::Config::default())
        .run(reconcile, error_policy, context)
        .for_each(|res| async move {
            match res {
                Ok(o) => debug!("Reconciled ElasticsearchUser {:?}", o.0.name),
                Err(e) => debug!("Reconcile ElasticsearchUser failed: {:?}", e),
            }
        })
        .await;
}
