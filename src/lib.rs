use kube::CustomResource;
use serde::{Deserialize, Serialize};
use std::cmp::PartialEq;

#[derive(CustomResource, Debug, Clone, Deserialize, Serialize, PartialEq)]
#[kube(
    derive = "PartialEq",
    namespaced,
    apiextensions = "v1beta1",
    group = "gonzalez.com",
    version = "v1",
    status = "EfsRequestStatus",
    shortname = "efs",
    printcolumn = r#"{
        "name":"Efs Name", 
        "type":"string", 
        "description":"name of efs volume", 
        "jsonPath":".spec.name"
    }"#,
    printcolumn = r#"{
        "name":"FsID", 
        "type":"string", 
        "description":"file_system_id of efs volume request", 
        "jsonPath":".status.file_system_id"
    }"#,
    printcolumn = r#"{
        "name":"Phase", 
        "type":"string", 
        "description":"phase of efs volume request", 
        "jsonPath":".status.condition.phase"
    }"#,
    printcolumn = r#"{
        "name":"Reason", 
        "type":"string", 
        "description":"reason for efs volume phase", 
        "jsonPath":".status.condition.reason"
    }"#
)]
pub struct EfsRequestSpec {
    pub name: String,
    pub owner: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq)]
pub struct EfsRequestStatus {
    pub file_system_id: Option<String>,
    pub condition: EfsRequestCondition,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "phase")]
pub enum EfsRequestCondition {
    Initialised,
    CreatingFileSystem,
    CreatingMountTargets,
    Success,
    Failed { reason: String },
}

impl Default for EfsRequestCondition {
    fn default() -> Self {
        EfsRequestCondition::Initialised
    }
}
