use std::sync::{Arc, Mutex};

use k8s_openapi::{api::core::v1::Pod, chrono};
use kube::{Api, Client, api::ListParams};

use crate::data::{PodInfo, PodPhase};

#[derive(Clone)]
pub struct Daemon {
    pub client: Client,
    pub pod_cache: Arc<Mutex<Vec<PodInfo>>>,
}

impl Daemon {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let client = Client::try_default().await?;
        let deamon = Self {
            client,
            pod_cache: Arc::new(Mutex::new(Vec::new())),
        };
        Ok(deamon)
    }
    pub async fn list_default_namespace_pods(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Infers configuration from ~/.kube/config or in-cluster environment

        let pods: Api<Pod> = Api::default_namespaced(self.client.clone());

        for p in pods.list(&Default::default()).await? {
            println!("Found Pod: {}", p.metadata.name.unwrap());
        }
        Ok(())
    }

    pub async fn fetch_pods(
        &self,
    ) -> Result<Vec<PodInfo>, Box<dyn std::error::Error + Send + Sync>> {
        let pods: Api<Pod> = Api::all(self.client.clone());

        let mut vec_of_podinfo = Vec::new();
        let lp = ListParams::default();

        for p in pods.list(&lp).await? {
            let name = p.metadata.name.clone().unwrap_or_default();
            let namespace = p.metadata.namespace.clone().unwrap_or_default();

            let node = p
                .spec
                .as_ref()
                .and_then(|s| s.node_name.clone())
                .unwrap_or_default();

            let ip = p
                .status
                .as_ref()
                .and_then(|s| s.pod_ip.clone())
                .unwrap_or_default();

            let phase = pod_phase(&p);

            let container_statuses = p
                .status
                .as_ref()
                .and_then(|s| s.container_statuses.as_ref());

            let ready_containers = container_statuses
                .map(|cs| cs.iter().filter(|c| c.ready).count() as u32)
                .unwrap_or(0);

            let total_containers = p
                .spec
                .as_ref()
                .map(|s| s.containers.len() as u32)
                .unwrap_or(0);

            let restarts = container_statuses
                .map(|cs| cs.iter().map(|c| c.restart_count.max(0) as u32).sum())
                .unwrap_or(0);

            let containers = p
                .spec
                .as_ref()
                .map(|s| {
                    s.containers
                        .iter()
                        .map(|c| c.name.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            let owner = p
                .metadata
                .owner_references
                .as_ref()
                .and_then(|o| o.first())
                .map(|o| o.name.clone())
                .unwrap_or_default();

            // TODO: calculate from creation_timestamp
            let age_secs = p
                .metadata
                .creation_timestamp
                .as_ref()
                .map(|ts| {
                    let now = chrono::Utc::now();
                    (now - ts.0).num_seconds().max(0) as u64
                })
                .unwrap_or(0);

            vec_of_podinfo.push(PodInfo {
                name,
                namespace,
                ready: (ready_containers, total_containers),
                phase,
                restarts,
                cpu_millicores: 0,
                mem_mib: 0,
                node,
                ip,
                containers,
                owner,
                age_secs,
            });
        }

        Ok(vec_of_podinfo)
    }

    pub fn pods(&self) -> Vec<PodInfo> {
        let pods = self.pod_cache.lock().unwrap().clone();
        pods
    }
}

fn pod_phase(pod: &Pod) -> PodPhase {
    // Terminating
    if pod.metadata.deletion_timestamp.is_some() {
        return PodPhase::Terminating;
    }

    // Container states
    if let Some(status) = &pod.status {
        if let Some(container_statuses) = &status.container_statuses {
            for cs in container_statuses {
                if let Some(state) = &cs.state {
                    if let Some(waiting) = &state.waiting {
                        match waiting.reason.as_deref() {
                            Some("CrashLoopBackOff") => {
                                return PodPhase::CrashLoopBackOff;
                            }
                            Some("ContainerCreating") => {
                                return PodPhase::ContainerCreating;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        match status.phase.as_deref() {
            Some("Running") => return PodPhase::Running,
            Some("Pending") => return PodPhase::Pending,
            Some("Succeeded") => return PodPhase::Succeeded,
            Some("Failed") => return PodPhase::Failed,
            _ => {}
        }
    }

    PodPhase::Pending
}
