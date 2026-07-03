//! Generates believable multi-cluster mock data and jitters it over time so
//! the TUI feels alive without ever talking to a real cluster.

use rand::seq::SliceRandom;
use rand::Rng;

use super::model::*;

const HASH_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";

fn hash(rng: &mut impl Rng, len: usize) -> String {
    (0..len)
        .map(|_| HASH_CHARS[rng.gen_range(0..HASH_CHARS.len())] as char)
        .collect()
}

#[derive(Clone, Copy)]
enum WorkloadKind {
    Deployment,
    StatefulSet,
}

struct AppSpec {
    name: &'static str,
    namespace: &'static str,
    replicas: u32,
    image: &'static str,
    kind: WorkloadKind,
    port: Option<u16>,
}

const APPS: &[AppSpec] = &[
    AppSpec { name: "frontend", namespace: "default", replicas: 3, image: "web:1.4", kind: WorkloadKind::Deployment, port: Some(80) },
    AppSpec { name: "api-gateway", namespace: "default", replicas: 2, image: "gateway:2.1", kind: WorkloadKind::Deployment, port: Some(8080) },
    AppSpec { name: "auth-service", namespace: "default", replicas: 2, image: "auth:1.0", kind: WorkloadKind::Deployment, port: Some(9000) },
    AppSpec { name: "payment-service", namespace: "default", replicas: 2, image: "payment:3.2", kind: WorkloadKind::Deployment, port: Some(9100) },
    AppSpec { name: "notification-worker", namespace: "default", replicas: 2, image: "worker:1.1", kind: WorkloadKind::Deployment, port: None },
    AppSpec { name: "redis", namespace: "default", replicas: 1, image: "redis:7", kind: WorkloadKind::StatefulSet, port: Some(6379) },
    AppSpec { name: "postgres", namespace: "default", replicas: 1, image: "postgres:16", kind: WorkloadKind::StatefulSet, port: Some(5432) },
    AppSpec { name: "prometheus-server", namespace: "monitoring", replicas: 1, image: "prometheus:2.51", kind: WorkloadKind::Deployment, port: Some(9090) },
    AppSpec { name: "grafana", namespace: "monitoring", replicas: 1, image: "grafana:10.4", kind: WorkloadKind::Deployment, port: Some(3000) },
    AppSpec { name: "alertmanager", namespace: "monitoring", replicas: 1, image: "alertmanager:0.27", kind: WorkloadKind::Deployment, port: Some(9093) },
    AppSpec { name: "ingress-nginx-controller", namespace: "ingress-nginx", replicas: 1, image: "ingress-nginx:1.10", kind: WorkloadKind::Deployment, port: Some(443) },
];

const NAMESPACES: &[&str] = &[
    "default",
    "kube-system",
    "kube-public",
    "kube-node-lease",
    "monitoring",
    "ingress-nginx",
];

#[derive(Default)]
struct ClusterData {
    namespaces: Vec<NamespaceInfo>,
    nodes: Vec<NodeInfo>,
    pods: Vec<PodInfo>,
    deployments: Vec<DeploymentInfo>,
    replicasets: Vec<ReplicaSetInfo>,
    statefulsets: Vec<StatefulSetInfo>,
    services: Vec<ServiceInfo>,
    ingresses: Vec<IngressInfo>,
    configmaps: Vec<ConfigMapInfo>,
    secrets: Vec<SecretInfo>,
    events: Vec<EventInfo>,
    pvcs: Vec<PvcInfo>,
}

pub struct ResourceRow {
    pub namespace: Option<String>,
    pub name: String,
    pub cells: Vec<String>,
    pub status_col: Option<usize>,
    pub severity: Severity,
}

pub struct MockBackend {
    pub clusters: Vec<ClusterInfo>,
    pub active: usize,
    data: Vec<ClusterData>,
    clock: u64,
}

impl MockBackend {
    pub fn new() -> Self {
        let mut rng = rand::thread_rng();
        let specs: [(&str, &str, Provider, usize, &str); 3] = [
            ("minikube", "minikube", Provider::Minikube, 1, "v1.31.0"),
            ("kind-dev", "kind-kind-dev", Provider::Kind, 2, "v1.30.2"),
            ("kind-staging", "kind-kind-staging", Provider::Kind, 4, "v1.30.2"),
        ];

        let mut clusters = Vec::new();
        let mut data = Vec::new();
        for (name, context, provider, node_count, version) in specs {
            clusters.push(ClusterInfo {
                name: name.to_string(),
                context: context.to_string(),
                provider,
                k8s_version: version.to_string(),
                node_count,
            });
            data.push(build_cluster_data(&mut rng, provider, node_count));
        }

        MockBackend {
            clusters,
            active: 0,
            data,
            clock: 0,
        }
    }

    pub fn cluster(&self) -> &ClusterInfo {
        &self.clusters[self.active]
    }

    pub fn select_cluster(&mut self, idx: usize) {
        if idx < self.clusters.len() {
            self.active = idx;
        }
    }

    fn cur(&self) -> &ClusterData {
        &self.data[self.active]
    }

    pub fn namespaces(&self) -> &[NamespaceInfo] {
        &self.cur().namespaces
    }

    pub fn pod_by_name(&self, namespace: &str, name: &str) -> Option<&PodInfo> {
        self.cur()
            .pods
            .iter()
            .find(|p| p.namespace == namespace && p.name == name)
    }

    pub fn nodes(&self) -> &[NodeInfo] {
        &self.cur().nodes
    }

    /// Aggregate CPU/Mem usage across nodes in the active cluster, for the
    /// header gauges.
    pub fn cluster_usage(&self) -> (u8, u8) {
        let nodes = self.nodes();
        if nodes.is_empty() {
            return (0, 0);
        }
        let cpu = nodes.iter().map(|n| n.cpu_pct as u32).sum::<u32>() / nodes.len() as u32;
        let mem = nodes.iter().map(|n| n.mem_pct as u32).sum::<u32>() / nodes.len() as u32;
        (cpu as u8, mem as u8)
    }

    /// Advance mock state by one tick (call roughly every 1-2s).
    pub fn tick(&mut self) {
        self.clock += 1;
        let mut rng = rand::thread_rng();
        let d = &mut self.data[self.active];

        for ns in d.namespaces.iter_mut() {
            ns.age_secs += 2;
        }
        for n in d.nodes.iter_mut() {
            n.age_secs += 2;
            jitter_pct(&mut rng, &mut n.cpu_pct);
            jitter_pct(&mut rng, &mut n.mem_pct);
        }
        for p in d.pods.iter_mut() {
            p.age_secs += 2;
            jitter_u32(&mut rng, &mut p.cpu_millicores, 5, 20, 400);
            jitter_u32(&mut rng, &mut p.mem_mib, 8, 16, 800);

            match p.phase {
                PodPhase::ContainerCreating if rng.gen_bool(0.2) => {
                    p.phase = PodPhase::Running;
                    p.ready = (p.ready.1, p.ready.1);
                }
                PodPhase::Pending if rng.gen_bool(0.05) => {
                    p.phase = PodPhase::ContainerCreating;
                }
                PodPhase::Running if rng.gen_bool(0.004) => {
                    p.phase = PodPhase::Terminating;
                    p.ready.0 = 0;
                }
                PodPhase::Terminating if rng.gen_bool(0.3) => {
                    p.phase = PodPhase::ContainerCreating;
                }
                PodPhase::CrashLoopBackOff => {
                    if rng.gen_bool(0.15) {
                        p.restarts += 1;
                        p.ready.0 = 0;
                    } else if rng.gen_bool(0.5) {
                        p.ready.0 = p.ready.1;
                    } else {
                        p.ready.0 = 0;
                    }
                }
                _ => {}
            }
        }
        for dep in d.deployments.iter_mut() {
            dep.age_secs += 2;
        }
        for rs in d.replicasets.iter_mut() {
            rs.age_secs += 2;
        }
        for sts in d.statefulsets.iter_mut() {
            sts.age_secs += 2;
        }
        for s in d.services.iter_mut() {
            s.age_secs += 2;
        }
        for i in d.ingresses.iter_mut() {
            i.age_secs += 2;
        }
        for c in d.configmaps.iter_mut() {
            c.age_secs += 2;
        }
        for s in d.secrets.iter_mut() {
            s.age_secs += 2;
        }
        for e in d.events.iter_mut() {
            e.age_secs += 2;
        }
        for p in d.pvcs.iter_mut() {
            p.age_secs += 2;
        }

        if rng.gen_bool(0.08) {
            if let Some(pod) = d.pods.choose(&mut rng) {
                let (reason, msg, ty) = random_event_template(&mut rng, &pod.name);
                d.events.insert(
                    0,
                    EventInfo {
                        namespace: pod.namespace.clone(),
                        event_type: ty,
                        reason: reason.to_string(),
                        object: format!("Pod/{}", pod.name),
                        message: msg,
                        count: rng.gen_range(1..4),
                        age_secs: 0,
                    },
                );
                if d.events.len() > 200 {
                    d.events.truncate(200);
                }
            }
        }
    }

    pub fn rows(&self, kind: ResourceKind, ns_filter: Option<&str>) -> Vec<ResourceRow> {
        let d = self.cur();
        let keep_ns = |ns: &str| ns_filter.map(|f| f == ns).unwrap_or(true);
        match kind {
            ResourceKind::Pods => {
                let mut items: Vec<_> = d.pods.iter().filter(|p| keep_ns(&p.namespace)).collect();
                items.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));
                items
                    .into_iter()
                    .map(|p| ResourceRow {
                        namespace: Some(p.namespace.clone()),
                        name: p.name.clone(),
                        cells: vec![
                            p.namespace.clone(),
                            p.name.clone(),
                            format!("{}/{}", p.ready.0, p.ready.1),
                            p.phase.label().to_string(),
                            p.restarts.to_string(),
                            format!("{}m", p.cpu_millicores),
                            format!("{}Mi", p.mem_mib),
                            p.node.clone(),
                            format_age(p.age_secs),
                        ],
                        status_col: Some(3),
                        severity: p.phase.severity(),
                    })
                    .collect()
            }
            ResourceKind::Deployments => {
                let mut items: Vec<_> = d.deployments.iter().filter(|x| keep_ns(&x.namespace)).collect();
                items.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));
                items
                    .into_iter()
                    .map(|x| ResourceRow {
                        namespace: Some(x.namespace.clone()),
                        name: x.name.clone(),
                        cells: vec![
                            x.namespace.clone(),
                            x.name.clone(),
                            format!("{}/{}", x.ready.0, x.ready.1),
                            x.up_to_date.to_string(),
                            x.available.to_string(),
                            format_age(x.age_secs),
                        ],
                        status_col: Some(2),
                        severity: sev(x.ready.0 == x.ready.1),
                    })
                    .collect()
            }
            ResourceKind::ReplicaSets => {
                let mut items: Vec<_> = d.replicasets.iter().filter(|x| keep_ns(&x.namespace)).collect();
                items.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));
                items
                    .into_iter()
                    .map(|x| ResourceRow {
                        namespace: Some(x.namespace.clone()),
                        name: x.name.clone(),
                        cells: vec![
                            x.namespace.clone(),
                            x.name.clone(),
                            x.desired.to_string(),
                            x.current.to_string(),
                            x.ready.to_string(),
                            format_age(x.age_secs),
                        ],
                        status_col: Some(4),
                        severity: sev(x.ready == x.desired),
                    })
                    .collect()
            }
            ResourceKind::StatefulSets => {
                let mut items: Vec<_> = d.statefulsets.iter().filter(|x| keep_ns(&x.namespace)).collect();
                items.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));
                items
                    .into_iter()
                    .map(|x| ResourceRow {
                        namespace: Some(x.namespace.clone()),
                        name: x.name.clone(),
                        cells: vec![
                            x.namespace.clone(),
                            x.name.clone(),
                            format!("{}/{}", x.ready.0, x.ready.1),
                            format_age(x.age_secs),
                        ],
                        status_col: Some(2),
                        severity: sev(x.ready.0 == x.ready.1),
                    })
                    .collect()
            }
            ResourceKind::Services => {
                let mut items: Vec<_> = d.services.iter().filter(|x| keep_ns(&x.namespace)).collect();
                items.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));
                items
                    .into_iter()
                    .map(|x| ResourceRow {
                        namespace: Some(x.namespace.clone()),
                        name: x.name.clone(),
                        cells: vec![
                            x.namespace.clone(),
                            x.name.clone(),
                            x.svc_type.label().to_string(),
                            x.cluster_ip.clone(),
                            x.external_ip.clone(),
                            x.ports.clone(),
                            format_age(x.age_secs),
                        ],
                        status_col: None,
                        severity: Severity::Neutral,
                    })
                    .collect()
            }
            ResourceKind::Ingresses => {
                let mut items: Vec<_> = d.ingresses.iter().filter(|x| keep_ns(&x.namespace)).collect();
                items.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));
                items
                    .into_iter()
                    .map(|x| ResourceRow {
                        namespace: Some(x.namespace.clone()),
                        name: x.name.clone(),
                        cells: vec![
                            x.namespace.clone(),
                            x.name.clone(),
                            x.class.clone(),
                            x.hosts.clone(),
                            x.address.clone(),
                            x.ports.clone(),
                            format_age(x.age_secs),
                        ],
                        status_col: None,
                        severity: Severity::Neutral,
                    })
                    .collect()
            }
            ResourceKind::Nodes => {
                let mut items: Vec<_> = d.nodes.iter().collect();
                items.sort_by(|a, b| a.name.cmp(&b.name));
                items
                    .into_iter()
                    .map(|x| ResourceRow {
                        namespace: None,
                        name: x.name.clone(),
                        cells: vec![
                            x.name.clone(),
                            x.status.label().to_string(),
                            x.roles.clone(),
                            x.version.clone(),
                            format!("{}%", x.cpu_pct),
                            format!("{}%", x.mem_pct),
                            format_age(x.age_secs),
                        ],
                        status_col: Some(1),
                        severity: x.status.severity(),
                    })
                    .collect()
            }
            ResourceKind::Namespaces => {
                let mut items: Vec<_> = d.namespaces.iter().collect();
                items.sort_by(|a, b| a.name.cmp(&b.name));
                items
                    .into_iter()
                    .map(|x| ResourceRow {
                        namespace: None,
                        name: x.name.clone(),
                        cells: vec![x.name.clone(), x.status.to_string(), format_age(x.age_secs)],
                        status_col: Some(1),
                        severity: Severity::Good,
                    })
                    .collect()
            }
            ResourceKind::ConfigMaps => {
                let mut items: Vec<_> = d.configmaps.iter().filter(|x| keep_ns(&x.namespace)).collect();
                items.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));
                items
                    .into_iter()
                    .map(|x| ResourceRow {
                        namespace: Some(x.namespace.clone()),
                        name: x.name.clone(),
                        cells: vec![
                            x.namespace.clone(),
                            x.name.clone(),
                            x.data_count.to_string(),
                            format_age(x.age_secs),
                        ],
                        status_col: None,
                        severity: Severity::Neutral,
                    })
                    .collect()
            }
            ResourceKind::Secrets => {
                let mut items: Vec<_> = d.secrets.iter().filter(|x| keep_ns(&x.namespace)).collect();
                items.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));
                items
                    .into_iter()
                    .map(|x| ResourceRow {
                        namespace: Some(x.namespace.clone()),
                        name: x.name.clone(),
                        cells: vec![
                            x.namespace.clone(),
                            x.name.clone(),
                            x.secret_type.clone(),
                            x.data_count.to_string(),
                            format_age(x.age_secs),
                        ],
                        status_col: None,
                        severity: Severity::Neutral,
                    })
                    .collect()
            }
            ResourceKind::Events => {
                let mut items: Vec<_> = d.events.iter().filter(|x| keep_ns(&x.namespace)).collect();
                items.sort_by(|a, b| a.age_secs.cmp(&b.age_secs));
                items
                    .into_iter()
                    .map(|x| ResourceRow {
                        namespace: Some(x.namespace.clone()),
                        name: x.object.clone(),
                        cells: vec![
                            x.namespace.clone(),
                            x.event_type.label().to_string(),
                            x.reason.clone(),
                            x.object.clone(),
                            x.message.clone(),
                            x.count.to_string(),
                            format_age(x.age_secs),
                        ],
                        status_col: Some(1),
                        severity: x.event_type.severity(),
                    })
                    .collect()
            }
            ResourceKind::Pvcs => {
                let mut items: Vec<_> = d.pvcs.iter().filter(|x| keep_ns(&x.namespace)).collect();
                items.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));
                items
                    .into_iter()
                    .map(|x| ResourceRow {
                        namespace: Some(x.namespace.clone()),
                        name: x.name.clone(),
                        cells: vec![
                            x.namespace.clone(),
                            x.name.clone(),
                            x.status.to_string(),
                            x.volume.clone(),
                            x.capacity.clone(),
                            x.storage_class.clone(),
                            format_age(x.age_secs),
                        ],
                        status_col: Some(2),
                        severity: sev(x.status == "Bound"),
                    })
                    .collect()
            }
        }
    }

    pub fn describe(&self, kind: ResourceKind, namespace: Option<&str>, name: &str) -> String {
        let d = self.cur();
        match kind {
            ResourceKind::Pods => d
                .pods
                .iter()
                .find(|p| Some(p.namespace.as_str()) == namespace && p.name == name)
                .map(|p| {
                    let events: String = d
                        .events
                        .iter()
                        .filter(|e| e.object == format!("Pod/{}", p.name))
                        .take(6)
                        .map(|e| {
                            format!(
                                "  {:<8} {:<18} {}\n",
                                e.event_type.label(),
                                e.reason,
                                e.message
                            )
                        })
                        .collect();
                    format!(
                        "Name:           {}\nNamespace:      {}\nNode:           {}\nIP:             {}\nStatus:         {}\nReady:          {}/{}\nRestart Count:  {}\nCPU:            {}m\nMemory:         {}Mi\nOwner:          {}\nContainers:\n{}\nAge:            {}\n\nEvents:\n{}",
                        p.name,
                        p.namespace,
                        p.node,
                        p.ip,
                        p.phase.label(),
                        p.ready.0,
                        p.ready.1,
                        p.restarts,
                        p.cpu_millicores,
                        p.mem_mib,
                        p.owner,
                        p.containers
                            .iter()
                            .map(|c| format!("  - {}", c))
                            .collect::<Vec<_>>()
                            .join("\n"),
                        format_age(p.age_secs),
                        if events.is_empty() { "  <none>\n".to_string() } else { events },
                    )
                })
                .unwrap_or_else(|| "not found".to_string()),
            ResourceKind::Deployments => d
                .deployments
                .iter()
                .find(|x| Some(x.namespace.as_str()) == namespace && x.name == name)
                .map(|x| {
                    format!(
                        "Name:        {}\nNamespace:   {}\nReplicas:    {}/{} ready\nUp-to-date:  {}\nAvailable:   {}\nAge:         {}",
                        x.name, x.namespace, x.ready.0, x.ready.1, x.up_to_date, x.available, format_age(x.age_secs)
                    )
                })
                .unwrap_or_else(|| "not found".to_string()),
            ResourceKind::ReplicaSets => d
                .replicasets
                .iter()
                .find(|x| Some(x.namespace.as_str()) == namespace && x.name == name)
                .map(|x| {
                    format!(
                        "Name:        {}\nNamespace:   {}\nOwner:       Deployment/{}\nDesired:     {}\nCurrent:     {}\nReady:       {}\nAge:         {}",
                        x.name, x.namespace, x.owner_deployment, x.desired, x.current, x.ready, format_age(x.age_secs)
                    )
                })
                .unwrap_or_else(|| "not found".to_string()),
            ResourceKind::StatefulSets => d
                .statefulsets
                .iter()
                .find(|x| Some(x.namespace.as_str()) == namespace && x.name == name)
                .map(|x| {
                    format!(
                        "Name:        {}\nNamespace:   {}\nReplicas:    {}/{} ready\nAge:         {}",
                        x.name, x.namespace, x.ready.0, x.ready.1, format_age(x.age_secs)
                    )
                })
                .unwrap_or_else(|| "not found".to_string()),
            ResourceKind::Services => d
                .services
                .iter()
                .find(|x| Some(x.namespace.as_str()) == namespace && x.name == name)
                .map(|x| {
                    format!(
                        "Name:         {}\nNamespace:    {}\nType:         {}\nCluster-IP:   {}\nExternal-IP:  {}\nPorts:        {}\nSelector:     {}\nAge:          {}",
                        x.name, x.namespace, x.svc_type.label(), x.cluster_ip, x.external_ip, x.ports, x.selector, format_age(x.age_secs)
                    )
                })
                .unwrap_or_else(|| "not found".to_string()),
            ResourceKind::Ingresses => d
                .ingresses
                .iter()
                .find(|x| Some(x.namespace.as_str()) == namespace && x.name == name)
                .map(|x| {
                    format!(
                        "Name:       {}\nNamespace:  {}\nClass:      {}\nHosts:      {}\nAddress:    {}\nPorts:      {}\nAge:        {}",
                        x.name, x.namespace, x.class, x.hosts, x.address, x.ports, format_age(x.age_secs)
                    )
                })
                .unwrap_or_else(|| "not found".to_string()),
            ResourceKind::Nodes => d
                .nodes
                .iter()
                .find(|x| x.name == name)
                .map(|x| {
                    format!(
                        "Name:       {}\nStatus:     {}\nRoles:      {}\nVersion:    {}\nCPU Usage:  {}%\nMem Usage:  {}%\nAge:        {}",
                        x.name, x.status.label(), x.roles, x.version, x.cpu_pct, x.mem_pct, format_age(x.age_secs)
                    )
                })
                .unwrap_or_else(|| "not found".to_string()),
            ResourceKind::Namespaces => d
                .namespaces
                .iter()
                .find(|x| x.name == name)
                .map(|x| format!("Name:    {}\nStatus:  {}\nAge:     {}", x.name, x.status, format_age(x.age_secs)))
                .unwrap_or_else(|| "not found".to_string()),
            ResourceKind::ConfigMaps => d
                .configmaps
                .iter()
                .find(|x| Some(x.namespace.as_str()) == namespace && x.name == name)
                .map(|x| {
                    format!(
                        "Name:       {}\nNamespace:  {}\nData Keys:  {}\nAge:        {}",
                        x.name, x.namespace, x.data_count, format_age(x.age_secs)
                    )
                })
                .unwrap_or_else(|| "not found".to_string()),
            ResourceKind::Secrets => d
                .secrets
                .iter()
                .find(|x| Some(x.namespace.as_str()) == namespace && x.name == name)
                .map(|x| {
                    format!(
                        "Name:       {}\nNamespace:  {}\nType:       {}\nData Keys:  {}\nAge:        {}",
                        x.name, x.namespace, x.secret_type, x.data_count, format_age(x.age_secs)
                    )
                })
                .unwrap_or_else(|| "not found".to_string()),
            ResourceKind::Events => d
                .events
                .iter()
                .find(|x| x.object == name)
                .map(|x| {
                    format!(
                        "Object:    {}\nNamespace: {}\nType:      {}\nReason:    {}\nMessage:   {}\nCount:     {}\nAge:       {}",
                        x.object, x.namespace, x.event_type.label(), x.reason, x.message, x.count, format_age(x.age_secs)
                    )
                })
                .unwrap_or_else(|| "not found".to_string()),
            ResourceKind::Pvcs => d
                .pvcs
                .iter()
                .find(|x| Some(x.namespace.as_str()) == namespace && x.name == name)
                .map(|x| {
                    format!(
                        "Name:           {}\nNamespace:      {}\nStatus:         {}\nVolume:         {}\nCapacity:       {}\nStorageClass:   {}\nAge:            {}",
                        x.name, x.namespace, x.status, x.volume, x.capacity, x.storage_class, format_age(x.age_secs)
                    )
                })
                .unwrap_or_else(|| "not found".to_string()),
        }
    }

    /// Produces the next fake log line for a pod's log stream.
    pub fn next_log_line(&self, container: &str, seq: u64) -> String {
        let mut rng = rand::thread_rng();
        let ts_secs = (self.clock * 2 + seq) % 86_400;
        let ts = format!(
            "{:02}:{:02}:{:02}",
            ts_secs / 3600,
            (ts_secs % 3600) / 60,
            ts_secs % 60
        );
        let template = LOG_TEMPLATES[rng.gen_range(0..LOG_TEMPLATES.len())];
        let filled = template
            .replace("{rand}", &hash(&mut rng, 6))
            .replace("{ms}", &rng.gen_range(1..500).to_string());
        format!("{} {} {}", ts, container, filled)
    }
}

const LOG_TEMPLATES: &[&str] = &[
    "INFO  Handling request GET /api/v1/health status=200 duration={ms}ms",
    "INFO  Connected to upstream in {ms}ms",
    "DEBUG cache hit for key=session:{rand}",
    "DEBUG cache miss for key=user:{rand}, fetching from db",
    "INFO  request completed status=200 duration={ms}ms",
    "WARN  slow query detected ({ms}ms) query=\"SELECT * FROM orders\"",
    "INFO  processed message id={rand} in {ms}ms",
    "ERROR failed to reach downstream service: connection reset",
    "INFO  health check ok",
    "DEBUG gc pause {ms}ms",
];

fn sev(good: bool) -> Severity {
    if good {
        Severity::Good
    } else {
        Severity::Warn
    }
}

fn jitter_pct(rng: &mut impl Rng, v: &mut u8) {
    let delta: i16 = rng.gen_range(-4..=4);
    let nv = (*v as i16 + delta).clamp(3, 97);
    *v = nv as u8;
}

fn jitter_u32(rng: &mut impl Rng, v: &mut u32, step: u32, min: u32, max: u32) {
    if rng.gen_bool(0.5) {
        *v = v.saturating_add(rng.gen_range(0..=step)).min(max);
    } else {
        *v = v.saturating_sub(rng.gen_range(0..=step)).max(min);
    }
}

fn random_event_template(rng: &mut impl Rng, pod_name: &str) -> (&'static str, String, EventType) {
    let templates: &[(&str, &str, EventType)] = &[
        ("Pulled", "Container image already present on machine", EventType::Normal),
        ("Created", "Created container", EventType::Normal),
        ("Started", "Started container", EventType::Normal),
        ("Scheduled", "Successfully assigned pod to node", EventType::Normal),
        ("BackOff", "Back-off restarting failed container", EventType::Warning),
        ("Unhealthy", "Readiness probe failed: HTTP probe failed with statuscode: 503", EventType::Warning),
    ];
    let (reason, msg, ty) = templates[rng.gen_range(0..templates.len())];
    (reason, format!("{} ({})", msg, pod_name), ty)
}

fn build_cluster_data(rng: &mut impl Rng, provider: Provider, node_count: usize) -> ClusterData {
    let mut d = ClusterData::default();

    for ns in NAMESPACES {
        d.namespaces.push(NamespaceInfo {
            name: ns.to_string(),
            status: "Active",
            age_secs: rng.gen_range(3600..30 * 86_400),
        });
    }

    let control_plane_name = match provider {
        Provider::Minikube => "minikube".to_string(),
        Provider::Kind => format!("{}-control-plane", cluster_slug(provider)),
    };
    d.nodes.push(NodeInfo {
        name: control_plane_name.clone(),
        status: NodeStatus::Ready,
        roles: "control-plane".to_string(),
        version: "v1.30.2".to_string(),
        cpu_pct: rng.gen_range(15..45),
        mem_pct: rng.gen_range(20..55),
        age_secs: rng.gen_range(5 * 86_400..40 * 86_400),
    });
    for i in 1..node_count {
        let not_ready = provider == Provider::Kind && node_count > 2 && i == node_count - 1;
        d.nodes.push(NodeInfo {
            name: format!("{}-worker{}", cluster_slug(provider), i),
            status: if not_ready { NodeStatus::NotReady } else { NodeStatus::Ready },
            roles: "<none>".to_string(),
            version: "v1.30.2".to_string(),
            cpu_pct: if not_ready { 0 } else { rng.gen_range(10..70) },
            mem_pct: if not_ready { 0 } else { rng.gen_range(15..75) },
            age_secs: rng.gen_range(5 * 86_400..40 * 86_400),
        });
    }
    let node_names: Vec<String> = d.nodes.iter().map(|n| n.name.clone()).collect();
    let ready_nodes: Vec<String> = d
        .nodes
        .iter()
        .filter(|n| n.status == NodeStatus::Ready)
        .map(|n| n.name.clone())
        .collect();

    // kube-root-ca.crt configmap in every namespace, like real clusters.
    for ns in NAMESPACES {
        d.configmaps.push(ConfigMapInfo {
            name: "kube-root-ca.crt".to_string(),
            namespace: ns.to_string(),
            data_count: 1,
            age_secs: rng.gen_range(5 * 86_400..40 * 86_400),
        });
    }

    // kube-system control plane / system pods.
    let ready_node = ready_nodes.first().cloned().unwrap_or(control_plane_name.clone());
    let system_pods: &[(&str, u32)] = &[
        ("etcd", 1),
        ("kube-apiserver", 1),
        ("kube-scheduler", 1),
        ("kube-controller-manager", 1),
    ];
    for (name, _) in system_pods {
        spawn_static_pod(&mut d, rng, name, &control_plane_name);
    }
    for nn in &node_names {
        let pod_name = format!("kube-proxy-{}", hash(rng, 5));
        d.pods.push(PodInfo {
            name: pod_name.clone(),
            namespace: "kube-system".to_string(),
            ready: (1, 1),
            phase: PodPhase::Running,
            restarts: 0,
            cpu_millicores: rng.gen_range(5..20),
            mem_mib: rng.gen_range(16..40),
            node: nn.clone(),
            ip: fake_ip(rng),
            containers: vec!["kube-proxy".to_string()],
            owner: "DaemonSet/kube-proxy".to_string(),
            age_secs: rng.gen_range(5 * 86_400..40 * 86_400),
        });
    }
    spawn_static_pod(&mut d, rng, "storage-provisioner", &ready_node);
    let coredns_rs = format!("coredns-{}", hash(rng, 10));
    d.deployments.push(DeploymentInfo {
        name: "coredns".to_string(),
        namespace: "kube-system".to_string(),
        ready: (2, 2),
        up_to_date: 2,
        available: 2,
        age_secs: rng.gen_range(5 * 86_400..40 * 86_400),
    });
    d.replicasets.push(ReplicaSetInfo {
        name: coredns_rs.clone(),
        namespace: "kube-system".to_string(),
        owner_deployment: "coredns".to_string(),
        desired: 2,
        current: 2,
        ready: 2,
        age_secs: rng.gen_range(5 * 86_400..40 * 86_400),
    });
    for _ in 0..2 {
        d.pods.push(PodInfo {
            name: format!("{}-{}", coredns_rs, hash(rng, 5)),
            namespace: "kube-system".to_string(),
            ready: (1, 1),
            phase: PodPhase::Running,
            restarts: 0,
            cpu_millicores: rng.gen_range(5..30),
            mem_mib: rng.gen_range(20..60),
            node: ready_nodes.choose(rng).cloned().unwrap_or(ready_node.clone()),
            ip: fake_ip(rng),
            containers: vec!["coredns".to_string()],
            owner: "Deployment/coredns".to_string(),
            age_secs: rng.gen_range(5 * 86_400..40 * 86_400),
        });
    }
    d.services.push(ServiceInfo {
        name: "kube-dns".to_string(),
        namespace: "kube-system".to_string(),
        svc_type: ServiceType::ClusterIP,
        cluster_ip: fake_svc_ip(rng),
        external_ip: "<none>".to_string(),
        ports: "53/UDP,53/TCP".to_string(),
        selector: "k8s-app=kube-dns".to_string(),
        age_secs: rng.gen_range(5 * 86_400..40 * 86_400),
    });

    // Deliberately "interesting" pods for demo flavor.
    let crash_target = "payment-service";
    let pending_target = "notification-worker";

    for spec in APPS {
        let deploy_age = rng.gen_range(2 * 3600..25 * 86_400);
        match spec.kind {
            WorkloadKind::Deployment => {
                let rs_hash = hash(rng, 10);
                let rs_name = format!("{}-{}", spec.name, rs_hash);
                let mut ready_count = spec.replicas;

                d.replicasets.push(ReplicaSetInfo {
                    name: rs_name.clone(),
                    namespace: spec.namespace.to_string(),
                    owner_deployment: spec.name.to_string(),
                    desired: spec.replicas,
                    current: spec.replicas,
                    ready: spec.replicas,
                    age_secs: deploy_age,
                });

                for i in 0..spec.replicas {
                    let is_crash = spec.name == crash_target && i == 0;
                    let is_pending = spec.name == pending_target && i == 0;
                    let phase = if is_crash {
                        ready_count -= 1;
                        PodPhase::CrashLoopBackOff
                    } else if is_pending {
                        ready_count -= 1;
                        PodPhase::Pending
                    } else if rng.gen_bool(0.04) {
                        PodPhase::ContainerCreating
                    } else {
                        PodPhase::Running
                    };
                    let ready = if phase == PodPhase::Running { (1, 1) } else { (0, 1) };
                    let restarts = if is_crash { rng.gen_range(15..60) } else if rng.gen_bool(0.1) { rng.gen_range(1..4) } else { 0 };

                    d.pods.push(PodInfo {
                        name: format!("{}-{}", rs_name, hash(rng, 5)),
                        namespace: spec.namespace.to_string(),
                        ready,
                        phase,
                        restarts,
                        cpu_millicores: rng.gen_range(5..300),
                        mem_mib: rng.gen_range(32..600),
                        node: ready_nodes.choose(rng).cloned().unwrap_or(ready_node.clone()),
                        ip: fake_ip(rng),
                        containers: vec![spec.name.to_string()],
                        owner: format!("ReplicaSet/{}", rs_name),
                        age_secs: rng.gen_range(0..deploy_age),
                    });
                }

                d.deployments.push(DeploymentInfo {
                    name: spec.name.to_string(),
                    namespace: spec.namespace.to_string(),
                    ready: (ready_count, spec.replicas),
                    up_to_date: spec.replicas,
                    available: ready_count,
                    age_secs: deploy_age,
                });
            }
            WorkloadKind::StatefulSet => {
                d.statefulsets.push(StatefulSetInfo {
                    name: spec.name.to_string(),
                    namespace: spec.namespace.to_string(),
                    ready: (spec.replicas, spec.replicas),
                    age_secs: deploy_age,
                });
                for i in 0..spec.replicas {
                    d.pods.push(PodInfo {
                        name: format!("{}-{}", spec.name, i),
                        namespace: spec.namespace.to_string(),
                        ready: (1, 1),
                        phase: PodPhase::Running,
                        restarts: if rng.gen_bool(0.1) { rng.gen_range(1..3) } else { 0 },
                        cpu_millicores: rng.gen_range(20..250),
                        mem_mib: rng.gen_range(64..700),
                        node: ready_nodes.choose(rng).cloned().unwrap_or(ready_node.clone()),
                        ip: fake_ip(rng),
                        containers: vec![spec.name.to_string()],
                        owner: format!("StatefulSet/{}", spec.name),
                        age_secs: rng.gen_range(0..deploy_age),
                    });
                }
                d.pvcs.push(PvcInfo {
                    name: format!("{}-data-0", spec.name),
                    namespace: spec.namespace.to_string(),
                    status: "Bound",
                    volume: format!("pvc-{}", hash(rng, 12)),
                    capacity: if spec.name == "postgres" { "10Gi".to_string() } else { "5Gi".to_string() },
                    storage_class: "standard".to_string(),
                    age_secs: deploy_age,
                });
            }
        }

        if let Some(port) = spec.port {
            let (svc_type, external_ip) = if spec.name == "ingress-nginx-controller" {
                match provider {
                    Provider::Minikube => (ServiceType::LoadBalancer, fake_ip(rng)),
                    Provider::Kind => (ServiceType::LoadBalancer, "<pending>".to_string()),
                }
            } else if spec.name == "frontend" {
                (ServiceType::NodePort, "<none>".to_string())
            } else {
                (ServiceType::ClusterIP, "<none>".to_string())
            };
            let ports = match svc_type {
                ServiceType::NodePort => format!("{}:{}/TCP", port, rng.gen_range(30000..32767)),
                _ => format!("{}/TCP", port),
            };
            d.services.push(ServiceInfo {
                name: spec.name.to_string(),
                namespace: spec.namespace.to_string(),
                svc_type,
                cluster_ip: fake_svc_ip(rng),
                external_ip,
                ports,
                selector: format!("app={}", spec.name),
                age_secs: deploy_age,
            });
        }

        d.configmaps.push(ConfigMapInfo {
            name: format!("{}-config", spec.name),
            namespace: spec.namespace.to_string(),
            data_count: rng.gen_range(2..9),
            age_secs: deploy_age,
        });

        if matches!(spec.name, "auth-service" | "payment-service" | "postgres") {
            d.secrets.push(SecretInfo {
                name: format!("{}-secret", spec.name),
                namespace: spec.namespace.to_string(),
                secret_type: "Opaque".to_string(),
                data_count: rng.gen_range(1..5),
                age_secs: deploy_age,
            });
        }

        let _ = spec.image; // used only for generation flavor, not surfaced in tables yet
    }

    d.secrets.push(SecretInfo {
        name: format!("default-token-{}", hash(rng, 5)),
        namespace: "kube-system".to_string(),
        secret_type: "kubernetes.io/service-account-token".to_string(),
        data_count: 3,
        age_secs: rng.gen_range(5 * 86_400..40 * 86_400),
    });
    d.secrets.push(SecretInfo {
        name: "app-tls".to_string(),
        namespace: "default".to_string(),
        secret_type: "kubernetes.io/tls".to_string(),
        data_count: 2,
        age_secs: rng.gen_range(3600..20 * 86_400),
    });

    // One-off Job pods, for status variety in the Pods table.
    d.pods.push(PodInfo {
        name: format!("db-migration-{}", hash(rng, 5)),
        namespace: "default".to_string(),
        ready: (0, 1),
        phase: PodPhase::Succeeded,
        restarts: 0,
        cpu_millicores: 0,
        mem_mib: 0,
        node: ready_node.clone(),
        ip: fake_ip(rng),
        containers: vec!["migrate".to_string()],
        owner: "Job/db-migration".to_string(),
        age_secs: rng.gen_range(600..7200),
    });
    d.pods.push(PodInfo {
        name: format!("cache-warmup-{}", hash(rng, 5)),
        namespace: "default".to_string(),
        ready: (0, 1),
        phase: PodPhase::Failed,
        restarts: rng.gen_range(1..3),
        cpu_millicores: 0,
        mem_mib: 0,
        node: ready_node.clone(),
        ip: fake_ip(rng),
        containers: vec!["warmup".to_string()],
        owner: "Job/cache-warmup".to_string(),
        age_secs: rng.gen_range(1200..9000),
    });

    d.ingresses.push(IngressInfo {
        name: "app-ingress".to_string(),
        namespace: "default".to_string(),
        class: "nginx".to_string(),
        hosts: "app.local".to_string(),
        address: d
            .services
            .iter()
            .find(|s| s.name == "ingress-nginx-controller")
            .map(|s| s.external_ip.clone())
            .unwrap_or_else(|| "<pending>".to_string()),
        ports: "80,443".to_string(),
        age_secs: rng.gen_range(3600..20 * 86_400),
    });

    // Seed a handful of events referencing real pods.
    let sample_pods: Vec<PodInfo> = d.pods.iter().take(8).cloned().collect();
    for p in &sample_pods {
        let (reason, msg, ty) = random_event_template(rng, &p.name);
        d.events.push(EventInfo {
            namespace: p.namespace.clone(),
            event_type: ty,
            reason: reason.to_string(),
            object: format!("Pod/{}", p.name),
            message: msg,
            count: rng.gen_range(1..5),
            age_secs: rng.gen_range(0..3600),
        });
    }
    if let Some(crash_pod) = d.pods.iter().find(|p| p.phase == PodPhase::CrashLoopBackOff) {
        d.events.push(EventInfo {
            namespace: crash_pod.namespace.clone(),
            event_type: EventType::Warning,
            reason: "BackOff".to_string(),
            object: format!("Pod/{}", crash_pod.name),
            message: format!("Back-off restarting failed container ({})", crash_pod.name),
            count: crash_pod.restarts,
            age_secs: rng.gen_range(0..600),
        });
    }
    d.events.sort_by(|a, b| a.age_secs.cmp(&b.age_secs));

    d
}

fn spawn_static_pod(d: &mut ClusterData, rng: &mut impl Rng, name: &str, node: &str) {
    d.pods.push(PodInfo {
        name: format!("{}-{}", name, node),
        namespace: "kube-system".to_string(),
        ready: (1, 1),
        phase: PodPhase::Running,
        restarts: 0,
        cpu_millicores: rng.gen_range(10..80),
        mem_mib: rng.gen_range(40..150),
        node: node.to_string(),
        ip: fake_ip(rng),
        containers: vec![name.to_string()],
        owner: "<none>".to_string(),
        age_secs: rng.gen_range(5 * 86_400..40 * 86_400),
    });
}

fn cluster_slug(provider: Provider) -> &'static str {
    match provider {
        Provider::Minikube => "minikube",
        Provider::Kind => "kind",
    }
}

fn fake_ip(rng: &mut impl Rng) -> String {
    format!("10.244.{}.{}", rng.gen_range(0..255), rng.gen_range(2..255))
}

fn fake_svc_ip(rng: &mut impl Rng) -> String {
    format!("10.96.{}.{}", rng.gen_range(0..255), rng.gen_range(2..255))
}
