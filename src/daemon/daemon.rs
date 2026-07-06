use std::sync::{Arc, Mutex};
use std::collections::{HashMap, VecDeque};
use k8s_openapi::{api::core::v1::Pod, chrono};
use kube::{Api, Client, api::{ListParams, PatchParams, Patch, DeleteParams, PostParams, LogParams}};
use kube::config::Kubeconfig;
use k8s_openapi::api::core::v1::{Service, Node, Namespace, ConfigMap, Secret, Event, PersistentVolumeClaim};
use k8s_openapi::api::apps::v1::{Deployment, ReplicaSet, StatefulSet};
use k8s_openapi::api::networking::v1::Ingress;
use futures::StreamExt;
use futures::io::AsyncBufReadExt;

use crate::data::{
    ResourceKind, ResourceRow, ClusterInfo, NamespaceInfo, PodInfo, PodPhase, Severity, Provider,
    pod_row, deployment_row, replicaset_row, statefulset_row, service_row, ingress_row,
    node_row, namespace_row, configmap_row, secret_row, event_row, pvc_row
};

fn get_age_secs(time: &Option<k8s_openapi::apimachinery::pkg::apis::meta::v1::Time>) -> u64 {
    time.as_ref()
        .map(|ts| {
            let now = chrono::Utc::now();
            (now - ts.0).num_seconds().max(0) as u64
        })
        .unwrap_or(0)
}

#[derive(Clone)]
pub struct Daemon {
    pub inner: Arc<Mutex<DaemonInner>>,
}

pub struct DaemonInner {
    pub client: Client,
    pub active_context: String,
    pub clusters: Vec<ClusterInfo>,
    pub cache: HashMap<ResourceKind, Vec<ResourceRow>>,
    pub describe_cache: HashMap<(ResourceKind, Option<String>, String), String>,
    pub namespaces: Vec<NamespaceInfo>,
    pub pods: Vec<PodInfo>,
    pub current_log_queue: VecDeque<String>,
    pub log_stream_abort: Option<tokio::sync::oneshot::Sender<()>>,
}

impl Daemon {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let mut clusters = Vec::new();
        let mut active_context = String::new();
        if let Ok(config) = Kubeconfig::read() {
            active_context = config.current_context.clone().unwrap_or_default();
            for c in config.contexts {
                let provider = if c.name.contains("minikube") {
                    Provider::Minikube
                } else if c.name.contains("kind") {
                    Provider::Kind
                } else {
                    Provider::Kind
                };
                clusters.push(ClusterInfo {
                    name: c.name.clone(),
                    context: c.name.clone(),
                    provider,
                    k8s_version: "v1.31.0".to_string(),
                    node_count: 0,
                });
            }
        }
        
        if clusters.is_empty() {
            clusters.push(ClusterInfo {
                name: "default".to_string(),
                context: "default".to_string(),
                provider: Provider::Kind,
                k8s_version: "unknown".to_string(),
                node_count: 0,
            });
            active_context = "default".to_string();
        }
        
        let client = Client::try_default().await?;
        
        let inner = DaemonInner {
            client,
            active_context,
            clusters,
            cache: HashMap::new(),
            describe_cache: HashMap::new(),
            namespaces: Vec::new(),
            pods: Vec::new(),
            current_log_queue: VecDeque::new(),
            log_stream_abort: None,
        };
        
        Ok(Self { inner: Arc::new(Mutex::new(inner)) })
    }

    pub fn pods(&self) -> Vec<PodInfo> {
        let inner = self.inner.lock().unwrap();
        inner.pods.clone()
    }

    pub fn pod_by_name(&self, namespace: &str, name: &str) -> Option<PodInfo> {
        let inner = self.inner.lock().unwrap();
        inner.pods.iter().find(|p| p.namespace == namespace && p.name == name).cloned()
    }

    pub fn rows(&self, kind: ResourceKind, _ns: Option<&str>) -> Vec<ResourceRow> {
        let inner = self.inner.lock().unwrap();
        inner.cache.get(&kind).cloned().unwrap_or_default()
    }

    pub fn namespaces(&self) -> Vec<NamespaceInfo> {
        let inner = self.inner.lock().unwrap();
        inner.namespaces.clone()
    }

    pub fn clusters(&self) -> Vec<ClusterInfo> {
        let inner = self.inner.lock().unwrap();
        inner.clusters.clone()
    }

    pub fn cluster(&self) -> ClusterInfo {
        let inner = self.inner.lock().unwrap();
        inner.clusters.iter()
            .find(|c| c.context == inner.active_context)
            .cloned()
            .unwrap_or_else(|| {
                ClusterInfo {
                    name: "unknown".to_string(),
                    context: "unknown".to_string(),
                    provider: Provider::Kind,
                    k8s_version: "unknown".to_string(),
                    node_count: 0,
                }
            })
    }

    pub fn cluster_usage(&self) -> (u8, u8) {
        let pod_count = self.pods().len();
        let cpu = (10 + (pod_count * 2) % 40) as u8;
        let mem = (20 + (pod_count * 3) % 50) as u8;
        (cpu, mem)
    }

    pub fn active_context(&self) -> String {
        let inner = self.inner.lock().unwrap();
        inner.active_context.clone()
    }

    pub fn select_cluster(&self, idx: usize) {
        let mut inner = self.inner.lock().unwrap();
        if idx >= inner.clusters.len() { return; }
        let context = inner.clusters[idx].context.clone();
        inner.active_context = context.clone();
        
        let opt = kube::config::KubeConfigOptions {
            context: Some(context),
            ..Default::default()
        };
        if let Ok(config) = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                kube::Config::from_kubeconfig(&opt).await
            })
        }) {
            if let Ok(client) = kube::Client::try_from(config) {
                inner.client = client;
                inner.cache.clear();
                inner.describe_cache.clear();
                inner.namespaces.clear();
                inner.pods.clear();
            }
        }
    }

    pub fn tick(&self) {}

    pub fn describe(&self, kind: ResourceKind, namespace: Option<&str>, name: &str) -> String {
        let mut inner = self.inner.lock().unwrap();
        let ns_owned = namespace.map(|s| s.to_string());
        let name_owned = name.to_string();
        let key = (kind, ns_owned.clone(), name_owned.clone());
        if let Some(desc) = inner.describe_cache.get(&key) {
            return desc.clone();
        }
        
        inner.describe_cache.insert(key.clone(), "Loading...".to_string());
        
        let client = inner.client.clone();
        let daemon_clone = self.clone();
        tokio::spawn(async move {
            let desc = fetch_live_yaml(&client, kind, ns_owned.as_deref(), &name_owned).await;
            let mut inner = daemon_clone.inner.lock().unwrap();
            inner.describe_cache.insert(key, desc);
        });
        
        "Loading...".to_string()
    }

    pub fn start_log_stream(&self, namespace: &str, pod: &str, container: &str) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(abort) = inner.log_stream_abort.take() {
            let _ = abort.send(());
        }
        inner.current_log_queue.clear();
        
        let client = inner.client.clone();
        let (abort_tx, mut abort_rx) = tokio::sync::oneshot::channel::<()>();
        inner.log_stream_abort = Some(abort_tx);
        
        let daemon_clone = self.clone();
        let ns = namespace.to_string();
        let p = pod.to_string();
        let c = container.to_string();
        
        tokio::spawn(async move {
            let api: Api<Pod> = Api::namespaced(client, &ns);
            let lp = LogParams {
                container: Some(c),
                follow: true,
                tail_lines: Some(100),
                ..Default::default()
            };
            
            if let Ok(stream) = api.log_stream(&p, &lp).await {
                let mut lines = stream.lines();
                while let Some(line_res) = lines.next().await {
                    tokio::select! {
                        _ = &mut abort_rx => {
                            break;
                        }
                        else => {
                            if let Ok(line) = line_res {
                                let mut inner = daemon_clone.inner.lock().unwrap();
                                inner.current_log_queue.push_back(line);
                                if inner.current_log_queue.len() > 500 {
                                    inner.current_log_queue.pop_front();
                                }
                            } else {
                                break;
                            }
                        }
                    }
                }
            }
        });
    }

    pub fn next_log_line(&self, _container: &str, _seq: u64) -> String {
        let mut inner = self.inner.lock().unwrap();
        inner.current_log_queue.pop_front().unwrap_or_default()
    }

    pub fn create_default(&self, kind: ResourceKind, namespace: Option<&str>, name: &str) -> Result<(), String> {
        let inner = self.inner.lock().unwrap();
        let client = inner.client.clone();
        let ns = namespace.map(|s| s.to_string());
        let name_str = name.to_string();
        
        tokio::spawn(async move {
            let res = perform_create(&client, kind, ns.as_deref(), &name_str).await;
            if let Err(e) = res {
                eprintln!("Failed to create {}: {}", name_str, e);
            }
        });
        Ok(())
    }

    pub fn delete(&self, kind: ResourceKind, namespace: Option<&str>, name: &str) -> Result<(), String> {
        let inner = self.inner.lock().unwrap();
        let client = inner.client.clone();
        let ns = namespace.map(|s| s.to_string());
        let name_str = name.to_string();
        
        tokio::spawn(async move {
            let res = perform_delete(&client, kind, ns.as_deref(), &name_str).await;
            if let Err(e) = res {
                eprintln!("Failed to delete {}: {}", name_str, e);
            }
        });
        Ok(())
    }

    pub fn current_edit_value(&self, kind: ResourceKind, namespace: Option<&str>, name: &str) -> Option<String> {
        let inner = self.inner.lock().unwrap();
        let rows = inner.cache.get(&kind)?;
        let row = rows.iter().find(|r| r.name == name && r.namespace.as_deref() == namespace)?;
        match kind {
            ResourceKind::Deployments | ResourceKind::StatefulSets => {
                let cell = row.cells.get(2)?;
                let parts: Vec<&str> = cell.split('/').collect();
                parts.get(1).map(|s| s.to_string())
            }
            ResourceKind::Services => row.cells.get(5).cloned(),
            ResourceKind::Ingresses => row.cells.get(3).cloned(),
            ResourceKind::ConfigMaps => row.cells.get(2).cloned(),
            ResourceKind::Secrets => row.cells.get(3).cloned(),
            ResourceKind::Pvcs => row.cells.get(4).cloned(),
            _ => None,
        }
    }

    pub fn apply_edit(&self, kind: ResourceKind, namespace: Option<&str>, name: &str, value: &str) -> Result<(), String> {
        let inner = self.inner.lock().unwrap();
        let client = inner.client.clone();
        let ns = namespace.map(|s| s.to_string());
        let name_str = name.to_string();
        let val_str = value.to_string();
        
        tokio::spawn(async move {
            let res = perform_edit(&client, kind, ns.as_deref(), &name_str, &val_str).await;
            if let Err(e) = res {
                eprintln!("Failed to edit {}: {}", name_str, e);
            }
        });
        Ok(())
    }

    pub async fn poll_all(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = {
            let inner = self.inner.lock().unwrap();
            inner.client.clone()
        };
        
        let lp = ListParams::default();

        // 1. Namespaces
        let mut ns_list = Vec::new();
        if let Ok(list) = Api::<Namespace>::all(client.clone()).list(&lp).await {
            for item in list {
                ns_list.push(namespace_row(&item));
            }
        }
        
        let namespace_infos: Vec<NamespaceInfo> = ns_list.iter().map(|r| {
            NamespaceInfo {
                name: r.name.clone(),
                status: "Active",
                age_secs: 0,
            }
        }).collect();
        
        // 2. Pods
        let mut pods_list = Vec::new();
        let mut pod_rows = Vec::new();
        if let Ok(list) = Api::<Pod>::all(client.clone()).list(&lp).await {
            for p in list {
                let name = p.metadata.name.clone().unwrap_or_default();
                let namespace = p.metadata.namespace.clone().unwrap_or_default();
                let node = p.spec.as_ref().and_then(|s| s.node_name.clone()).unwrap_or_default();
                let ip = p.status.as_ref().and_then(|s| s.pod_ip.clone()).unwrap_or_default();
                let phase = pod_phase(&p);
                let container_statuses = p.status.as_ref().and_then(|s| s.container_statuses.as_ref());
                let ready_containers = container_statuses.map(|cs| cs.iter().filter(|c| c.ready).count() as u32).unwrap_or(0);
                let total_containers = p.spec.as_ref().map(|s| s.containers.len() as u32).unwrap_or(0);
                let restarts = container_statuses.map(|cs| cs.iter().map(|c| c.restart_count.max(0) as u32).sum()).unwrap_or(0);
                let containers = p.spec.as_ref().map(|s| s.containers.iter().map(|c| c.name.clone()).collect()).unwrap_or_default();
                let owner = p.metadata.owner_references.as_ref().and_then(|o| o.first()).map(|o| o.name.clone()).unwrap_or_default();
                let age_secs = get_age_secs(&p.metadata.creation_timestamp);
                
                let p_info = PodInfo {
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
                };
                pods_list.push(p_info.clone());
                pod_rows.push(pod_row(&p_info));
            }
        }
        
        // 3. Deployments
        let mut deploy_rows = Vec::new();
        if let Ok(list) = Api::<Deployment>::all(client.clone()).list(&lp).await {
            for item in list {
                deploy_rows.push(deployment_row(&item));
            }
        }
        
        // 4. ReplicaSets
        let mut rs_rows = Vec::new();
        if let Ok(list) = Api::<ReplicaSet>::all(client.clone()).list(&lp).await {
            for item in list {
                rs_rows.push(replicaset_row(&item));
            }
        }
        
        // 5. StatefulSets
        let mut sts_rows = Vec::new();
        if let Ok(list) = Api::<StatefulSet>::all(client.clone()).list(&lp).await {
            for item in list {
                sts_rows.push(statefulset_row(&item));
            }
        }
        
        // 6. Services
        let mut svc_rows = Vec::new();
        if let Ok(list) = Api::<Service>::all(client.clone()).list(&lp).await {
            for item in list {
                svc_rows.push(service_row(&item));
            }
        }
        
        // 7. Ingresses
        let mut ingress_rows = Vec::new();
        if let Ok(list) = Api::<Ingress>::all(client.clone()).list(&lp).await {
            for item in list {
                ingress_rows.push(ingress_row(&item));
            }
        }
        
        // 8. Nodes
        let mut node_rows = Vec::new();
        if let Ok(list) = Api::<Node>::all(client.clone()).list(&lp).await {
            for item in list {
                node_rows.push(node_row(&item));
            }
        }
        
        // 9. ConfigMaps
        let mut cm_rows = Vec::new();
        if let Ok(list) = Api::<ConfigMap>::all(client.clone()).list(&lp).await {
            for item in list {
                cm_rows.push(configmap_row(&item));
            }
        }
        
        // 10. Secrets
        let mut secret_rows = Vec::new();
        if let Ok(list) = Api::<Secret>::all(client.clone()).list(&lp).await {
            for item in list {
                secret_rows.push(secret_row(&item));
            }
        }
        
        // 11. Events
        let mut event_rows = Vec::new();
        if let Ok(list) = Api::<Event>::all(client.clone()).list(&lp).await {
            for item in list {
                event_rows.push(event_row(&item));
            }
        }
        
        // 12. PVCs
        let mut pvc_rows = Vec::new();
        if let Ok(list) = Api::<PersistentVolumeClaim>::all(client.clone()).list(&lp).await {
            for item in list {
                pvc_rows.push(pvc_row(&item));
            }
        }
        
        let mut inner = self.inner.lock().unwrap();
        inner.pods = pods_list;
        inner.namespaces = namespace_infos;
        
        inner.cache.insert(ResourceKind::Pods, pod_rows);
        inner.cache.insert(ResourceKind::Deployments, deploy_rows);
        inner.cache.insert(ResourceKind::ReplicaSets, rs_rows);
        inner.cache.insert(ResourceKind::StatefulSets, sts_rows);
        inner.cache.insert(ResourceKind::Services, svc_rows);
        inner.cache.insert(ResourceKind::Ingresses, ingress_rows);
        inner.cache.insert(ResourceKind::Nodes, node_rows);
        inner.cache.insert(ResourceKind::Namespaces, ns_list);
        inner.cache.insert(ResourceKind::ConfigMaps, cm_rows);
        inner.cache.insert(ResourceKind::Secrets, secret_rows);
        inner.cache.insert(ResourceKind::Events, event_rows);
        inner.cache.insert(ResourceKind::Pvcs, pvc_rows);
        
        Ok(())
    }
}

async fn fetch_live_yaml(client: &Client, kind: ResourceKind, ns: Option<&str>, name: &str) -> String {
    let yaml_res = match kind {
        ResourceKind::Pods => {
            let api: Api<Pod> = Api::namespaced(client.clone(), ns.unwrap_or("default"));
            api.get(name).await.map(|o| serde_yaml::to_string(&o))
        }
        ResourceKind::Deployments => {
            let api: Api<Deployment> = Api::namespaced(client.clone(), ns.unwrap_or("default"));
            api.get(name).await.map(|o| serde_yaml::to_string(&o))
        }
        ResourceKind::ReplicaSets => {
            let api: Api<ReplicaSet> = Api::namespaced(client.clone(), ns.unwrap_or("default"));
            api.get(name).await.map(|o| serde_yaml::to_string(&o))
        }
        ResourceKind::StatefulSets => {
            let api: Api<StatefulSet> = Api::namespaced(client.clone(), ns.unwrap_or("default"));
            api.get(name).await.map(|o| serde_yaml::to_string(&o))
        }
        ResourceKind::Services => {
            let api: Api<Service> = Api::namespaced(client.clone(), ns.unwrap_or("default"));
            api.get(name).await.map(|o| serde_yaml::to_string(&o))
        }
        ResourceKind::Ingresses => {
            let api: Api<Ingress> = Api::namespaced(client.clone(), ns.unwrap_or("default"));
            api.get(name).await.map(|o| serde_yaml::to_string(&o))
        }
        ResourceKind::Nodes => {
            let api: Api<Node> = Api::all(client.clone());
            api.get(name).await.map(|o| serde_yaml::to_string(&o))
        }
        ResourceKind::Namespaces => {
            let api: Api<Namespace> = Api::all(client.clone());
            api.get(name).await.map(|o| serde_yaml::to_string(&o))
        }
        ResourceKind::ConfigMaps => {
            let api: Api<ConfigMap> = Api::namespaced(client.clone(), ns.unwrap_or("default"));
            api.get(name).await.map(|o| serde_yaml::to_string(&o))
        }
        ResourceKind::Secrets => {
            let api: Api<Secret> = Api::namespaced(client.clone(), ns.unwrap_or("default"));
            api.get(name).await.map(|o| serde_yaml::to_string(&o))
        }
        ResourceKind::Events => {
            let api: Api<Event> = Api::namespaced(client.clone(), ns.unwrap_or("default"));
            api.get(name).await.map(|o| serde_yaml::to_string(&o))
        }
        ResourceKind::Pvcs => {
            let api: Api<PersistentVolumeClaim> = Api::namespaced(client.clone(), ns.unwrap_or("default"));
            api.get(name).await.map(|o| serde_yaml::to_string(&o))
        }
    };
    
    match yaml_res {
        Ok(Ok(yaml)) => yaml,
        Ok(Err(e)) => format!("Error serializing: {}", e),
        Err(e) => format!("Error describing {}: {}", name, e),
    }
}

fn pod_phase(pod: &Pod) -> PodPhase {
    if pod.metadata.deletion_timestamp.is_some() {
        return PodPhase::Terminating;
    }

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

async fn perform_create(client: &Client, kind: ResourceKind, ns: Option<&str>, name: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let value = match kind {
        ResourceKind::Pods => serde_json::json!({
            "apiVersion": "v1",
            "kind": "Pod",
            "metadata": { "name": name },
            "spec": {
                "containers": [{
                    "name": "nginx",
                    "image": "nginx:alpine"
                }]
            }
        }),
        ResourceKind::Deployments => serde_json::json!({
            "apiVersion": "apps/v1",
            "kind": "Deployment",
            "metadata": { "name": name },
            "spec": {
                "replicas": 1,
                "selector": { "matchLabels": { "app": name } },
                "template": {
                    "metadata": { "labels": { "app": name } },
                    "spec": {
                        "containers": [{
                            "name": "nginx",
                            "image": "nginx:alpine"
                        }]
                    }
                }
            }
        }),
        ResourceKind::StatefulSets => serde_json::json!({
            "apiVersion": "apps/v1",
            "kind": "StatefulSet",
            "metadata": { "name": name },
            "spec": {
                "serviceName": name,
                "replicas": 1,
                "selector": { "matchLabels": { "app": name } },
                "template": {
                    "metadata": { "labels": { "app": name } },
                    "spec": {
                        "containers": [{
                            "name": "nginx",
                            "image": "nginx:alpine"
                        }]
                    }
                }
            }
        }),
        ResourceKind::Services => serde_json::json!({
            "apiVersion": "v1",
            "kind": "Service",
            "metadata": { "name": name },
            "spec": {
                "selector": { "app": name },
                "ports": [{ "port": 80, "targetPort": 80 }]
            }
        }),
        ResourceKind::ConfigMaps => serde_json::json!({
            "apiVersion": "v1",
            "kind": "ConfigMap",
            "metadata": { "name": name },
            "data": { "key1": "value1" }
        }),
        ResourceKind::Secrets => serde_json::json!({
            "apiVersion": "v1",
            "kind": "Secret",
            "metadata": { "name": name },
            "type": "Opaque",
            "data": { "key1": "dmFsdWUx" }
        }),
        ResourceKind::Pvcs => serde_json::json!({
            "apiVersion": "v1",
            "kind": "PersistentVolumeClaim",
            "metadata": { "name": name },
            "spec": {
                "accessModes": ["ReadWriteOnce"],
                "resources": {
                    "requests": { "storage": "1Gi" }
                }
            }
        }),
        _ => return Err("uncreatable resource type".into()),
    };

    match kind {
        ResourceKind::Pods => { Api::<Pod>::namespaced(client.clone(), ns.unwrap_or("default")).create(&PostParams::default(), &serde_json::from_value(value)?).await?; }
        ResourceKind::Deployments => { Api::<Deployment>::namespaced(client.clone(), ns.unwrap_or("default")).create(&PostParams::default(), &serde_json::from_value(value)?).await?; }
        ResourceKind::StatefulSets => { Api::<StatefulSet>::namespaced(client.clone(), ns.unwrap_or("default")).create(&PostParams::default(), &serde_json::from_value(value)?).await?; }
        ResourceKind::Services => { Api::<Service>::namespaced(client.clone(), ns.unwrap_or("default")).create(&PostParams::default(), &serde_json::from_value(value)?).await?; }
        ResourceKind::ConfigMaps => { Api::<ConfigMap>::namespaced(client.clone(), ns.unwrap_or("default")).create(&PostParams::default(), &serde_json::from_value(value)?).await?; }
        ResourceKind::Secrets => { Api::<Secret>::namespaced(client.clone(), ns.unwrap_or("default")).create(&PostParams::default(), &serde_json::from_value(value)?).await?; }
        ResourceKind::Pvcs => { Api::<PersistentVolumeClaim>::namespaced(client.clone(), ns.unwrap_or("default")).create(&PostParams::default(), &serde_json::from_value(value)?).await?; }
        _ => {}
    }
    Ok(())
}

async fn perform_delete(client: &Client, kind: ResourceKind, ns: Option<&str>, name: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let dp = DeleteParams::default();
    match kind {
        ResourceKind::Pods => { Api::<Pod>::namespaced(client.clone(), ns.unwrap_or("default")).delete(name, &dp).await?; }
        ResourceKind::Deployments => { Api::<Deployment>::namespaced(client.clone(), ns.unwrap_or("default")).delete(name, &dp).await?; }
        ResourceKind::ReplicaSets => { Api::<ReplicaSet>::namespaced(client.clone(), ns.unwrap_or("default")).delete(name, &dp).await?; }
        ResourceKind::StatefulSets => { Api::<StatefulSet>::namespaced(client.clone(), ns.unwrap_or("default")).delete(name, &dp).await?; }
        ResourceKind::Services => { Api::<Service>::namespaced(client.clone(), ns.unwrap_or("default")).delete(name, &dp).await?; }
        ResourceKind::Ingresses => { Api::<Ingress>::namespaced(client.clone(), ns.unwrap_or("default")).delete(name, &dp).await?; }
        ResourceKind::Nodes => { Api::<Node>::all(client.clone()).delete(name, &dp).await?; }
        ResourceKind::Namespaces => { Api::<Namespace>::all(client.clone()).delete(name, &dp).await?; }
        ResourceKind::ConfigMaps => { Api::<ConfigMap>::namespaced(client.clone(), ns.unwrap_or("default")).delete(name, &dp).await?; }
        ResourceKind::Secrets => { Api::<Secret>::namespaced(client.clone(), ns.unwrap_or("default")).delete(name, &dp).await?; }
        ResourceKind::Events => { Api::<Event>::namespaced(client.clone(), ns.unwrap_or("default")).delete(name, &dp).await?; }
        ResourceKind::Pvcs => { Api::<PersistentVolumeClaim>::namespaced(client.clone(), ns.unwrap_or("default")).delete(name, &dp).await?; }
    }
    Ok(())
}

async fn perform_edit(client: &Client, kind: ResourceKind, ns: Option<&str>, name: &str, value: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let pp = PatchParams::apply("k8s-tui");
    match kind {
        ResourceKind::Deployments => {
            let replicas: i32 = value.parse()?;
            let patch = serde_json::json!({
                "spec": { "replicas": replicas }
            });
            Api::<Deployment>::namespaced(client.clone(), ns.unwrap_or("default")).patch(name, &pp, &Patch::Merge(&patch)).await?;
        }
        ResourceKind::StatefulSets => {
            let replicas: i32 = value.parse()?;
            let patch = serde_json::json!({
                "spec": { "replicas": replicas }
            });
            Api::<StatefulSet>::namespaced(client.clone(), ns.unwrap_or("default")).patch(name, &pp, &Patch::Merge(&patch)).await?;
        }
        ResourceKind::Services => {
            let parts: Vec<&str> = value.split(':').collect();
            let port: i32 = parts[0].parse()?;
            let target_port = if parts.len() > 1 { parts[1].parse().unwrap_or(port) } else { port };
            let patch = serde_json::json!({
                "spec": {
                    "ports": [{ "port": port, "targetPort": target_port }]
                }
            });
            Api::<Service>::namespaced(client.clone(), ns.unwrap_or("default")).patch(name, &pp, &Patch::Merge(&patch)).await?;
        }
        ResourceKind::Ingresses => {
            let patch = serde_json::json!({
                "spec": {
                    "rules": [{ "host": value }]
                }
            });
            Api::<Ingress>::namespaced(client.clone(), ns.unwrap_or("default")).patch(name, &pp, &Patch::Merge(&patch)).await?;
        }
        ResourceKind::ConfigMaps => {
            let count: usize = value.parse()?;
            let mut data = serde_json::Map::new();
            for i in 1..=count {
                data.insert(format!("key{}", i), serde_json::Value::String("value".to_string()));
            }
            let patch = serde_json::json!({ "data": data });
            Api::<ConfigMap>::namespaced(client.clone(), ns.unwrap_or("default")).patch(name, &pp, &Patch::Merge(&patch)).await?;
        }
        ResourceKind::Secrets => {
            let count: usize = value.parse()?;
            let mut data = serde_json::Map::new();
            for i in 1..=count {
                data.insert(format!("key{}", i), serde_json::Value::String("dmFsdWUx".to_string()));
            }
            let patch = serde_json::json!({ "data": data });
            Api::<Secret>::namespaced(client.clone(), ns.unwrap_or("default")).patch(name, &pp, &Patch::Merge(&patch)).await?;
        }
        ResourceKind::Pvcs => {
            let patch = serde_json::json!({
                "spec": {
                    "resources": {
                        "requests": { "storage": value }
                    }
                }
            });
            Api::<PersistentVolumeClaim>::namespaced(client.clone(), ns.unwrap_or("default")).patch(name, &pp, &Patch::Merge(&patch)).await?;
        }
        _ => {}
    }
    Ok(())
}
