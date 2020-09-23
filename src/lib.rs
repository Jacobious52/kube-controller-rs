use kube_derive::CustomResource;
use serde::{Deserialize, Serialize};

#[derive(CustomResource, Debug, Clone, Deserialize, Serialize)]
#[kube(apiextensions = "v1beta1")]
#[kube(group = "gonzalez.com", version = "v1", namespaced)]
#[kube(status = "HelloStatus")]
pub struct HelloSpec {
    pub name: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct HelloStatus {
    pub message: String,
}
