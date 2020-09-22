use futures::StreamExt;
use kube::api::Meta;
use kube::api::PatchParams;
use kube::api::PostParams;
use kube::{
    api::{Api, ListParams},
    error::Error,
    Client,
};
use kube_derive::CustomResource;
use kube_runtime::controller::{Context, Controller, ReconcilerAction};
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;
use tokio::time::Duration;

use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1beta1::CustomResourceDefinition;

#[macro_use]
extern crate log;

#[derive(Error, Debug)]
#[error("{0}")]
enum ReconcileError {
    SerializationFailed(#[from] serde_json::Error),
    PatchStatusFailed(#[from] kube::Error),
}

#[derive(CustomResource, Debug, Clone, Deserialize, Serialize)]
#[kube(apiextensions = "v1beta1")]
#[kube(group = "gonzalez.com", version = "v1", namespaced)]
#[kube(status = "HelloStatus")]
struct HelloSpec {
    name: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct HelloStatus {
    message: String,
}

async fn reconcile(hello: Hello, ctx: Context<Client>) -> Result<ReconcilerAction, ReconcileError> {
    let client = ctx.get_ref().clone();
    let name = Meta::name(&hello);
    let namespace = Meta::namespace(&hello).expect("hello is namespaced");
    let hellos = Api::<Hello>::namespaced(client, &namespace);

    let new_status = serde_json::to_vec(&json!({
        "status": HelloStatus {
            message: format!("hello, {}", hello.spec.name)
        }
    }))
    .map_err(ReconcileError::SerializationFailed)?;

    hellos
        .patch_status(&name, &PatchParams::default(), new_status)
        .await
        .map_err(ReconcileError::PatchStatusFailed)?;

    info!("patched status for {}", name);

    Ok(ReconcilerAction {
        requeue_after: Some(Duration::from_secs(300)),
    })
}

fn error_policy(_error: &ReconcileError, _ctx: Context<Client>) -> ReconcilerAction {
    ReconcilerAction {
        requeue_after: Some(Duration::from_secs(60)),
    }
}

#[tokio::main]
async fn main() -> Result<(), kube::Error> {
    pretty_env_logger::init();

    let client = Client::try_default().await?;
    let context = Context::new(client.clone());

    // apply our own crd definition to the cluster if it's not already
    let crds = Api::<CustomResourceDefinition>::all(client.clone());
    match crds.create(&PostParams::default(), &Hello::crd()).await {
        Ok(_) => info!("created crd in cluster"),
        Err(Error::Api(response)) if response.code == 409 => (),
        Err(e) => return Err(e),
    };

    let hellos = Api::<Hello>::all(client);
    Controller::new(hellos, ListParams::default())
        .run(reconcile, error_policy, context)
        .for_each(|res| async move {
            match res {
                Ok(o) => info!("reconciled {:?}", o),
                Err(e) => error!("reconcile failed: {}", e),
            }
        })
        .await;
    Ok(())
}
