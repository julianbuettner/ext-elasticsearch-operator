#![deny(clippy::all)]
use std::{
    process::exit,
    time::{Duration, SystemTime},
};

use elasticsearch::ElasticAdmin;
use env::as_bool;
use error::OperatorError;
use futures::TryStreamExt;
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use kube::{
    api::{PatchParams, PostParams},
    runtime::{
        self,
        watcher::{self, Event},
    },
    Api, Client, CustomResourceExt,
};
use kube_derive::CustomResource;
use log::{debug, error, info, warn};
use reconciliation::{delete_user, ensure_user_exists};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::{select, time::sleep};

use crate::env::load_env;
pub mod elasticsearch;
mod env;
mod error;
mod reconciliation;

pub const KEEP_ANNOTATION: &str = "eeops.io/keep";
pub const PASSWORD_LENGTH: usize = 24;
pub const SECRET_USER: &str = "ELASTICSEARCH_USERNAME";
pub const SECRET_PASS: &str = "ELASTICSEARCH_PASSWORD";
pub const SECRET_URL: &str = "ELASTICSEARCH_URL";
pub const RESTART_SECONDS: u64 = 900; // reconcile everything every 15min

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
struct ElasticsearchUserSpec {
    secret_ref: String,
    username: String,
    prefixes: Vec<String>,
    permissions: UserPermissions,
}

/// Status object for Foo
#[derive(Deserialize, Serialize, Clone, Debug, Default, JsonSchema)]
pub struct ElasticSearchUserSpec {
    ok: bool,
    error_message: Option<String>,
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

async fn handle_user_event(
    user: Event<ElasticsearchUser>,
    client: &Client,
    elastic_admin: &ElasticAdmin,
) -> Result<(), OperatorError> {
    match user {
        Event::Restarted(users) => {
            for user in &users {
                ensure_user_exists(&user, client, elastic_admin).await?;
            }
            debug!("Restarted, reconciled {} users successfully.", users.len());
        }
        Event::Applied(user) => {
            ensure_user_exists(&user, client, elastic_admin).await?;
            info!(
                "Applied {}",
                user.metadata.name.as_ref().unwrap_or(&"<no name>".into())
            );
        }
        Event::Deleted(user) => {
            let keep: bool = user
                .metadata
                .annotations
                .as_ref()
                .and_then(|anno| {
                    anno.get(KEEP_ANNOTATION).map(|value| {
                        as_bool(value).unwrap_or_else(|| {
                            warn!("{} not a bool ({}). Keep user.", value, KEEP_ANNOTATION);
                            true
                        })
                    })
                })
                .unwrap_or(false);
            if !keep {
                delete_user(&user, client, elastic_admin).await?;
                info!(
                    "Deleting {}",
                    user.metadata.name.as_ref().unwrap_or(&"<no name>".into())
                );
            }
            info!(
                "Skipped deletion of {}",
                user.metadata.name.as_ref().unwrap_or(&"<no name>".into())
            );
        }
    }
    Ok(())
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

    // Start wating ElasticsearchUser CRDs
    loop {
        let elastic_users: Api<ElasticsearchUser> = Api::default_namespaced(client.clone());
        let watch = runtime::watcher(elastic_users, runtime::watcher::Config::default())
            .try_for_each(|user_event| async {
                let username = 0;
                if let Err(e) = handle_user_event(user_event, &client, &elastic_admin).await {
                    error!("Error while reconciling user {}: {}", username, e);
                } else {
                    debug!("The reconciliation of was successfull.");
                }
                Ok(())
            });

        let sleep = sleep(Duration::from_secs(RESTART_SECONDS));

        enum FirstDone {
            Watch(Result<(), watcher::Error>),
            Sleep,
        }

        let first = select! {
            watch_res = watch => FirstDone::Watch(watch_res),
            _ = sleep => FirstDone::Sleep,
        };

        let watch_result;
        if let FirstDone::Watch(wr) = first {
            watch_result = wr;
        } else {
            continue;
        }

        const OLD_SCHEMA: &str =
            "The stream failed to read CR. Has the CRD been updated and there are old entries?";
        match watch_result {
            Err(watcher::Error::InitialListFailed(kube::Error::SerdeError(_))) => {
                error!("{}", OLD_SCHEMA)
            }
            Err(watcher::Error::WatchFailed(kube::Error::SerdeError(_))) => {
                error!("{}", OLD_SCHEMA)
            }
            Err(e) => error!("The watch terminated: {}", e),
            Ok(_) => error!("Watch terminated gracefully, but should never terminate"),
        }
        break;
    }
    // Controller can't handle CR deletion??
    // let context = Arc::new(elastic_admin);
    // let config_maps: Api<ConfigMap> = Api::default_namespaced(client.clone());
    // Controller::for_stream(watch)
    //     // .owns(config_maps, watcher::Config::default())
    //     .run(reconcile, error_policy, context)
    //     .for_each(|res| async move {
    //         match res {
    //             Ok(o) => info!("Reconciled {:?}", o),
    //             Err(e) => warn!("Error reconciling {:?}", e),
    //         }
    //     })
    //     .await;
}
