#[macro_use]
extern crate log;

use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1beta1::CustomResourceDefinition;
use kube::{
    api::{Api, ListParams, Meta, PatchParams, PostParams},
    error::Error,
    Client,
};
use kube_runtime::controller::{Context, Controller, ReconcilerAction};
use rusoto_efs::Efs;

use rusoto_core::{Region, RusotoError};
use rusoto_efs::{CreateFileSystemRequest, EfsClient, Tag};

use futures::StreamExt;
use serde_json::json;
use thiserror::Error;
use tokio::time::Duration;

use kube_controller_rs::{EfsRequest, EfsRequestCondition, EfsRequestStatus};

#[derive(Error, Debug)]
#[error("{0}")]
enum ReconcileError {
    SerializationFailed(#[from] serde_json::Error),
    PatchStatusFailed(#[from] kube::Error),
}

struct Data {
    client: Client,
    efs_client: EfsClient,
}

fn fmt_aws_error<E>(e: RusotoError<E>) -> String
where
    E: std::error::Error + 'static,
{
    match e {
        RusotoError::Unknown(h) => h.body_as_str().to_string(),
        _ => e.to_string(),
    }
}

async fn create_efs(efs_request: &EfsRequest, efs_client: &EfsClient) -> EfsRequestStatus {
    let tags = vec![
        Tag {
            key: String::from("Name"),
            value: efs_request.spec.name.clone(),
        },
        Tag {
            key: String::from("Owner"),
            value: efs_request.spec.owner.clone(),
        },
    ];

    let create_request = CreateFileSystemRequest {
        creation_token: String::from("test"),
        tags: Some(tags),
        ..CreateFileSystemRequest::default()
    };

    info!(
        "creating aws file system {} for {}",
        efs_request.spec.name, efs_request.spec.owner
    );

    let result = efs_client.create_file_system(create_request).await;

    match result {
        Ok(fd) => EfsRequestStatus {
            condition: EfsRequestCondition::CreatingFileSystem,
            file_system_id: Some(fd.file_system_id),
        },
        Err(e) => EfsRequestStatus {
            condition: EfsRequestCondition::Failed {
                reason: fmt_aws_error(e),
            },
            file_system_id: None,
        },
    }
}

async fn reconcile(
    efs_request: EfsRequest,
    ctx: Context<Data>,
) -> Result<ReconcilerAction, ReconcileError> {
    // get some info before we handle the request
    let k8s_client = ctx.get_ref().client.clone();
    let efs_client = ctx.get_ref().efs_client.clone();

    let name = Meta::name(&efs_request);
    let namespace = Meta::namespace(&efs_request).expect("EfsRequest is namespaced");

    let efs_requests = Api::<EfsRequest>::namespaced(k8s_client, &namespace);

    let new_status: EfsRequestStatus = if let Some(ref status) = efs_request.status {
        match status.condition {
            EfsRequestCondition::Initialised => create_efs(&efs_request, &efs_client).await,
            EfsRequestCondition::CreatingFileSystem => status.clone(),
            EfsRequestCondition::CreatingMountTargets => EfsRequestStatus::default(),
            EfsRequestCondition::Success => status.clone(),
            EfsRequestCondition::Failed { reason: _ } => status.clone(),
        }
    } else {
        // no status let's make one
        EfsRequestStatus::default()
    };

    if Some(new_status.clone()) != efs_request.status {
        let new_status_patch = serde_json::to_vec(&json!({ "status": new_status }))
            .map_err(ReconcileError::SerializationFailed)?;

        debug!(
            "patching status with new status {:#?} for efs {}/{}",
            new_status, namespace, name
        );
        efs_requests
            .patch_status(&name, &PatchParams::default(), new_status_patch)
            .await
            .map_err(ReconcileError::PatchStatusFailed)?;
    } else {
        info!("no change for efs {}/{}", namespace, name);
    }

    Ok(ReconcilerAction {
        requeue_after: Some(Duration::from_secs(20)),
    })
}

fn error_policy(_error: &ReconcileError, _ctx: Context<Data>) -> ReconcilerAction {
    ReconcilerAction {
        requeue_after: Some(Duration::from_secs(60)),
    }
}

#[tokio::main]
async fn main() -> Result<(), kube::Error> {
    pretty_env_logger::init();

    let client = Client::try_default().await?;
    let context = Context::new(Data {
        client: client.clone(),
        efs_client: EfsClient::new(Region::UsEast1),
    });

    // apply our own crd definition to the cluster if it's not already
    let crds = Api::<CustomResourceDefinition>::all(client.clone());
    match crds
        .create(&PostParams::default(), &EfsRequest::crd())
        .await
    {
        Ok(_) => info!("created crd in cluster"),
        Err(Error::Api(response)) if response.code == 409 => (),
        Err(e) => return Err(e),
    };

    let efs_requests = Api::<EfsRequest>::all(client);
    Controller::new(efs_requests, ListParams::default())
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
