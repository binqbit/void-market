use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Identifies an immutable object in the assigned node's protected local store.
/// It is not a filesystem path, download token, scalar hash or plaintext payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRef {
    pub id: Uuid,
    pub digest: [u8; 32],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Registration {
    pub instance_id: Uuid,
    pub capacity: u16,
    pub sequence: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeProvision {
    pub deployment_id: Uuid,
    pub node_id: Uuid,
    pub name: String,
    pub public_key: String,
    pub token_file: String,
}

/// Worker lifecycle only. Completion is not proof verification or auction acceptance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Queued,
    Running,
    Waiting,
    ResultPending,
    Completed,
    Interrupted,
    Failed,
    Unavailable,
}

impl JobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Waiting => "waiting",
            Self::ResultPending => "result_pending",
            Self::Completed => "completed",
            Self::Interrupted => "interrupted",
            Self::Failed => "failed",
            Self::Unavailable => "unavailable",
        }
    }

    pub fn terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Interrupted | Self::Failed | Self::Unavailable
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    pub id: Uuid,
    pub node_id: Uuid,
    pub input: ArtifactRef,
    pub status: JobStatus,
    pub generation: i64,
    pub instance_id: Option<Uuid>,
    pub response: Option<ArtifactRef>,
    pub output: Option<ArtifactRef>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Claim {
    pub job_id: Uuid,
    pub instance_id: Uuid,
    pub generation: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Completion {
    pub deployment_id: Uuid,
    pub node_id: Uuid,
    pub instance_id: Uuid,
    pub signature: String,
    pub id: Uuid,
    pub job_id: Uuid,
    pub generation: i64,
    pub status: JobStatus,
    pub output: Option<ArtifactRef>,
    pub consumed_response: Option<Uuid>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobInput {
    pub job_id: Uuid,
    pub request_id: Uuid,
    pub response: ArtifactRef,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewJob {
    pub id: Uuid,
    pub node_id: Uuid,
    pub input: ArtifactRef,
}
