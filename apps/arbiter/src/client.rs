use crate::{
    config::Config,
    error::{Error, Result},
};
use arbiter_protocol::{Claim, Completion, Job, Registration, RequestSignature, SigningKey};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::{Client, Method};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use url::Url;
use uuid::Uuid;

#[derive(Clone)]
pub struct Gateway {
    http: Client,
    root: Url,
    node: Uuid,
    deployment: Uuid,
    instance: Uuid,
    key: Arc<SigningKey>,
    token: Arc<String>,
}

impl Gateway {
    pub fn new(config: &Config, instance: Uuid) -> Result<Self> {
        let mut builder = Client::builder()
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none());
        if let Some(ca) = &config.gateway_ca {
            builder = builder.add_root_certificate(
                reqwest::Certificate::from_pem(ca).map_err(|_| Error::Configuration)?,
            );
        }
        let http = builder.build().map_err(|_| Error::Configuration)?;
        Ok(Self {
            http,
            root: config.gateway.clone(),
            node: config.node_id,
            deployment: config.deployment_id,
            instance,
            key: Arc::new(config.signing_key.clone()),
            token: Arc::new(config.token.clone()),
        })
    }

    async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Vec<u8>,
    ) -> Result<T> {
        let now: i64 = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::Gateway)?
            .as_millis()
            .try_into()
            .map_err(|_| Error::Gateway)?;
        let mut signed = RequestSignature {
            deployment_id: self.deployment,
            node_id: self.node,
            instance_id: self.instance,
            request_id: Uuid::new_v4(),
            issued_at_ms: now,
            expires_at_ms: now + 60_000,
            signature: String::new(),
        };
        arbiter_protocol::sign(&self.key, &mut signed, method.as_str(), path, &body);
        let header =
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&signed).map_err(|_| Error::Gateway)?);
        let mut response = self
            .http
            .request(method, self.root.join(path).map_err(|_| Error::Gateway)?)
            .bearer_auth(&*self.token)
            .header("X-Node-Signature", header)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await
            .map_err(|_| Error::Gateway)?;
        if !response.status().is_success() {
            return Err(if matches!(response.status().as_u16(), 401 | 403 | 409) {
                Error::Ownership
            } else {
                Error::Gateway
            });
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Gateway)? {
            if bytes.len() + chunk.len() > arbiter_protocol::BODY_LIMIT {
                return Err(Error::Gateway);
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| Error::Gateway)
    }

    async fn post<T: Serialize>(&self, path: &str, body: &T) -> Result<()> {
        let _: serde_json::Value = self
            .request(
                Method::POST,
                path,
                serde_json::to_vec(body).map_err(|_| Error::Gateway)?,
            )
            .await?;
        Ok(())
    }

    pub async fn register(
        &self,
        capacity: usize,
        session_capacity: usize,
        sequence: i64,
    ) -> Result<()> {
        self.post(
            "/node/v1/register",
            &Registration {
                instance_id: self.instance,
                capacity: capacity as u16,
                session_capacity: session_capacity as u16,
                sequence,
            },
        )
        .await
    }

    pub async fn heartbeat(
        &self,
        capacity: usize,
        session_capacity: usize,
        sequence: i64,
    ) -> Result<()> {
        self.post(
            "/node/v1/heartbeat",
            &Registration {
                instance_id: self.instance,
                capacity: capacity as u16,
                session_capacity: session_capacity as u16,
                sequence,
            },
        )
        .await
    }

    pub async fn jobs(&self) -> Result<Vec<Job>> {
        self.request(Method::GET, "/node/v1/jobs", Vec::new()).await
    }

    pub async fn job(&self, id: Uuid) -> Result<Job> {
        self.request(Method::GET, &format!("/node/v1/jobs/{id}"), Vec::new())
            .await
    }

    pub async fn claim(&self, job: &Job) -> Result<Job> {
        self.request(
            Method::POST,
            &format!("/node/v1/jobs/{}/claim", job.id),
            serde_json::to_vec(&Claim {
                job_id: job.id,
                instance_id: self.instance,
                generation: job.generation,
            })
            .map_err(|_| Error::Gateway)?,
        )
        .await
    }

    pub async fn renew(&self, job: &Job) -> Result<()> {
        self.post(
            &format!("/node/v1/jobs/{}/renew", job.id),
            &Claim {
                job_id: job.id,
                instance_id: self.instance,
                generation: job.generation,
            },
        )
        .await
    }

    pub fn sign_completion(&self, result: &mut Completion) -> Result<()> {
        if result.signature.is_empty() {
            result.deployment_id = self.deployment;
            result.node_id = self.node;
            result.instance_id = self.instance;
            arbiter_protocol::sign_result(result, &self.key);
        } else {
            if result.deployment_id != self.deployment
                || result.node_id != self.node
                || result.instance_id != self.instance
            {
                return Err(Error::Ownership);
            }
            arbiter_protocol::verify_result(result, &arbiter_protocol::public_key(&self.key))
                .map_err(|_| Error::Ownership)?;
        }
        Ok(())
    }

    pub async fn complete(&self, result: &Completion) -> Result<()> {
        self.post(&format!("/node/v1/jobs/{}/results", result.job_id), result)
            .await
    }
}
