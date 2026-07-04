//! Plain data shapes rendered by the UI. All fields are owned/cloned so the
//! UI layer never has to know whether they came from mock data or a real
//! cluster.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceKind {
    Pods,
    Deployments,
    ReplicaSets,
    StatefulSets,
    Services,
    Ingresses,
    Nodes,
    Namespaces,
    ConfigMaps,
    Secrets,
    Events,
    Pvcs,
}

impl ResourceKind {
    pub const ALL: [ResourceKind; 12] = [
        ResourceKind::Pods,
        ResourceKind::Deployments,
        ResourceKind::ReplicaSets,
        ResourceKind::StatefulSets,
        ResourceKind::Services,
        ResourceKind::Ingresses,
        ResourceKind::Nodes,
        ResourceKind::Namespaces,
        ResourceKind::ConfigMaps,
        ResourceKind::Secrets,
        ResourceKind::Events,
        ResourceKind::Pvcs,
    ];

    pub fn title(self) -> &'static str {
        match self {
            ResourceKind::Pods => "Pods",
            ResourceKind::Deployments => "Deployments",
            ResourceKind::ReplicaSets => "ReplicaSets",
            ResourceKind::StatefulSets => "StatefulSets",
            ResourceKind::Services => "Services",
            ResourceKind::Ingresses => "Ingresses",
            ResourceKind::Nodes => "Nodes",
            ResourceKind::Namespaces => "Namespaces",
            ResourceKind::ConfigMaps => "ConfigMaps",
            ResourceKind::Secrets => "Secrets",
            ResourceKind::Events => "Events",
            ResourceKind::Pvcs => "PersistentVolumeClaims",
        }
    }

    /// Aliases usable in the `:` command bar, e.g. `:po`, `:deploy`.
    pub fn aliases(self) -> &'static [&'static str] {
        match self {
            ResourceKind::Pods => &["po", "pod", "pods"],
            ResourceKind::Deployments => &["deploy", "deployment", "deployments"],
            ResourceKind::ReplicaSets => &["rs", "replicaset", "replicasets"],
            ResourceKind::StatefulSets => &["sts", "statefulset", "statefulsets"],
            ResourceKind::Services => &["svc", "service", "services"],
            ResourceKind::Ingresses => &["ing", "ingress", "ingresses"],
            ResourceKind::Nodes => &["no", "node", "nodes"],
            ResourceKind::Namespaces => &["ns", "namespace", "namespaces"],
            ResourceKind::ConfigMaps => &["cm", "configmap", "configmaps"],
            ResourceKind::Secrets => &["secret", "secrets"],
            ResourceKind::Events => &["ev", "event", "events"],
            ResourceKind::Pvcs => &["pvc", "pvcs"],
        }
    }

    pub fn from_alias(s: &str) -> Option<ResourceKind> {
        let s = s.trim().to_ascii_lowercase();
        ResourceKind::ALL
            .into_iter()
            .find(|k| k.aliases().contains(&s.as_str()))
    }

    pub fn namespaced(self) -> bool {
        !matches!(
            self,
            ResourceKind::Nodes | ResourceKind::Namespaces | ResourceKind::Events
        )
    }

    /// Kinds the `Ctrl+N` quick-create flow knows how to scaffold. Excludes
    /// controller-managed kinds (ReplicaSets), infra-level kinds (Nodes),
    /// and system-generated kinds (Events).
    pub fn creatable(self) -> bool {
        matches!(
            self,
            ResourceKind::Pods
                | ResourceKind::Deployments
                | ResourceKind::StatefulSets
                | ResourceKind::Services
                | ResourceKind::Ingresses
                | ResourceKind::ConfigMaps
                | ResourceKind::Secrets
                | ResourceKind::Namespaces
                | ResourceKind::Pvcs
        )
    }

    pub fn columns(self) -> &'static [&'static str] {
        match self {
            ResourceKind::Pods => &[
                "NAMESPACE", "NAME", "READY", "STATUS", "RESTARTS", "CPU", "MEM", "NODE", "AGE",
            ],
            ResourceKind::Deployments => {
                &["NAMESPACE", "NAME", "READY", "UP-TO-DATE", "AVAILABLE", "AGE"]
            }
            ResourceKind::ReplicaSets => {
                &["NAMESPACE", "NAME", "DESIRED", "CURRENT", "READY", "AGE"]
            }
            ResourceKind::StatefulSets => &["NAMESPACE", "NAME", "READY", "AGE"],
            ResourceKind::Services => &[
                "NAMESPACE", "NAME", "TYPE", "CLUSTER-IP", "EXTERNAL-IP", "PORTS", "AGE",
            ],
            ResourceKind::Ingresses => &[
                "NAMESPACE", "NAME", "CLASS", "HOSTS", "ADDRESS", "PORTS", "AGE",
            ],
            ResourceKind::Nodes => &[
                "NAME", "STATUS", "ROLES", "VERSION", "CPU", "MEM", "AGE",
            ],
            ResourceKind::Namespaces => &["NAME", "STATUS", "AGE"],
            ResourceKind::ConfigMaps => &["NAMESPACE", "NAME", "DATA", "AGE"],
            ResourceKind::Secrets => &["NAMESPACE", "NAME", "TYPE", "DATA", "AGE"],
            ResourceKind::Events => &[
                "NAMESPACE", "TYPE", "REASON", "OBJECT", "MESSAGE", "COUNT", "AGE",
            ],
            ResourceKind::Pvcs => &[
                "NAMESPACE", "NAME", "STATUS", "VOLUME", "CAPACITY", "STORAGECLASS", "AGE",
            ],
        }
    }
}

/// Coarse severity used to color a status cell. Kept deliberately small so
/// the UI has one place to map meaning -> color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Good,
    Warn,
    Bad,
    Neutral,
}

pub fn format_age(secs: u64) -> String {
    const MIN: u64 = 60;
    const HOUR: u64 = 60 * MIN;
    const DAY: u64 = 24 * HOUR;
    if secs >= DAY {
        let d = secs / DAY;
        let h = (secs % DAY) / HOUR;
        if h > 0 {
            format!("{}d{}h", d, h)
        } else {
            format!("{}d", d)
        }
    } else if secs >= HOUR {
        let h = secs / HOUR;
        let m = (secs % HOUR) / MIN;
        format!("{}h{}m", h, m)
    } else if secs >= MIN {
        format!("{}m", secs / MIN)
    } else {
        format!("{}s", secs)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Minikube,
    Kind,
}

impl Provider {
    pub fn label(self) -> &'static str {
        match self {
            Provider::Minikube => "minikube",
            Provider::Kind => "kind",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClusterInfo {
    pub name: String,
    pub context: String,
    pub provider: Provider,
    pub k8s_version: String,
    pub node_count: usize,
}

#[derive(Debug, Clone)]
pub struct NamespaceInfo {
    pub name: String,
    pub status: &'static str,
    pub age_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PodPhase {
    Running,
    Pending,
    Succeeded,
    Failed,
    CrashLoopBackOff,
    ContainerCreating,
    Terminating,
}

impl PodPhase {
    pub fn label(self) -> &'static str {
        match self {
            PodPhase::Running => "Running",
            PodPhase::Pending => "Pending",
            PodPhase::Succeeded => "Completed",
            PodPhase::Failed => "Error",
            PodPhase::CrashLoopBackOff => "CrashLoopBackOff",
            PodPhase::ContainerCreating => "ContainerCreating",
            PodPhase::Terminating => "Terminating",
        }
    }

    pub fn severity(self) -> Severity {
        match self {
            PodPhase::Running | PodPhase::Succeeded => Severity::Good,
            PodPhase::Pending | PodPhase::ContainerCreating | PodPhase::Terminating => {
                Severity::Warn
            }
            PodPhase::Failed | PodPhase::CrashLoopBackOff => Severity::Bad,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PodInfo {
    pub name: String,
    pub namespace: String,
    pub ready: (u32, u32),
    pub phase: PodPhase,
    pub restarts: u32,
    pub cpu_millicores: u32,
    pub mem_mib: u32,
    pub node: String,
    pub ip: String,
    pub containers: Vec<String>,
    pub owner: String,
    pub age_secs: u64,
}

#[derive(Debug, Clone)]
pub struct DeploymentInfo {
    pub name: String,
    pub namespace: String,
    pub ready: (u32, u32),
    pub up_to_date: u32,
    pub available: u32,
    pub age_secs: u64,
}

#[derive(Debug, Clone)]
pub struct ReplicaSetInfo {
    pub name: String,
    pub namespace: String,
    pub owner_deployment: String,
    pub desired: u32,
    pub current: u32,
    pub ready: u32,
    pub age_secs: u64,
}

#[derive(Debug, Clone)]
pub struct StatefulSetInfo {
    pub name: String,
    pub namespace: String,
    pub ready: (u32, u32),
    pub age_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceType {
    ClusterIP,
    NodePort,
    LoadBalancer,
}

impl ServiceType {
    pub fn label(self) -> &'static str {
        match self {
            ServiceType::ClusterIP => "ClusterIP",
            ServiceType::NodePort => "NodePort",
            ServiceType::LoadBalancer => "LoadBalancer",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServiceInfo {
    pub name: String,
    pub namespace: String,
    pub svc_type: ServiceType,
    pub cluster_ip: String,
    pub external_ip: String,
    pub ports: String,
    pub selector: String,
    pub age_secs: u64,
}

#[derive(Debug, Clone)]
pub struct IngressInfo {
    pub name: String,
    pub namespace: String,
    pub class: String,
    pub hosts: String,
    pub address: String,
    pub ports: String,
    pub age_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeStatus {
    Ready,
    NotReady,
}

impl NodeStatus {
    pub fn label(self) -> &'static str {
        match self {
            NodeStatus::Ready => "Ready",
            NodeStatus::NotReady => "NotReady",
        }
    }
    pub fn severity(self) -> Severity {
        match self {
            NodeStatus::Ready => Severity::Good,
            NodeStatus::NotReady => Severity::Bad,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub name: String,
    pub status: NodeStatus,
    pub roles: String,
    pub version: String,
    pub cpu_pct: u8,
    pub mem_pct: u8,
    pub age_secs: u64,
}

#[derive(Debug, Clone)]
pub struct ConfigMapInfo {
    pub name: String,
    pub namespace: String,
    pub data_count: u32,
    pub age_secs: u64,
}

#[derive(Debug, Clone)]
pub struct SecretInfo {
    pub name: String,
    pub namespace: String,
    pub secret_type: String,
    pub data_count: u32,
    pub age_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    Normal,
    Warning,
}

impl EventType {
    pub fn label(self) -> &'static str {
        match self {
            EventType::Normal => "Normal",
            EventType::Warning => "Warning",
        }
    }
    pub fn severity(self) -> Severity {
        match self {
            EventType::Normal => Severity::Neutral,
            EventType::Warning => Severity::Warn,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EventInfo {
    pub namespace: String,
    pub event_type: EventType,
    pub reason: String,
    pub object: String,
    pub message: String,
    pub count: u32,
    pub age_secs: u64,
}

#[derive(Debug, Clone)]
pub struct PvcInfo {
    pub name: String,
    pub namespace: String,
    pub status: &'static str,
    pub volume: String,
    pub capacity: String,
    pub storage_class: String,
    pub age_secs: u64,
}
