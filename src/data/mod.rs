mod model;

pub use model::*;

use k8s_openapi::api::core::v1::{Pod, Service, Node, Namespace, ConfigMap, Secret, Event, PersistentVolumeClaim};
use k8s_openapi::api::apps::v1::{Deployment, ReplicaSet, StatefulSet};
use k8s_openapi::api::networking::v1::Ingress;

fn get_age_secs(time: &Option<k8s_openapi::apimachinery::pkg::apis::meta::v1::Time>) -> u64 {
    time.as_ref()
        .map(|ts| {
            let now = k8s_openapi::chrono::Utc::now();
            (now - ts.0).num_seconds().max(0) as u64
        })
        .unwrap_or(0)
}

pub fn pod_row(p: &PodInfo) -> ResourceRow {
    ResourceRow {
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
    }
}

pub fn deployment_row(d: &Deployment) -> ResourceRow {
    let namespace = d.metadata.namespace.clone();
    let name = d.metadata.name.clone().unwrap_or_default();
    
    let spec_replicas = d.spec.as_ref().and_then(|s| s.replicas).unwrap_or(0) as u32;
    let status = d.status.as_ref();
    let ready_replicas = status.and_then(|s| s.ready_replicas).unwrap_or(0) as u32;
    let updated_replicas = status.and_then(|s| s.updated_replicas).unwrap_or(0) as u32;
    let available_replicas = status.and_then(|s| s.available_replicas).unwrap_or(0) as u32;
    
    let age_secs = get_age_secs(&d.metadata.creation_timestamp);
    
    let severity = if ready_replicas == spec_replicas {
        Severity::Good
    } else if ready_replicas == 0 {
        Severity::Bad
    } else {
        Severity::Warn
    };
    
    ResourceRow {
        namespace: namespace.clone(),
        name: name.clone(),
        cells: vec![
            namespace.unwrap_or_default(),
            name,
            format!("{}/{}", ready_replicas, spec_replicas),
            updated_replicas.to_string(),
            available_replicas.to_string(),
            format_age(age_secs),
        ],
        status_col: Some(2),
        severity,
    }
}

pub fn replicaset_row(r: &ReplicaSet) -> ResourceRow {
    let namespace = r.metadata.namespace.clone();
    let name = r.metadata.name.clone().unwrap_or_default();
    
    let desired = r.spec.as_ref().and_then(|s| s.replicas).unwrap_or(0) as u32;
    let status = r.status.as_ref();
    let current = status.map(|s| s.replicas).unwrap_or(0) as u32;
    let ready = status.and_then(|s| s.ready_replicas).unwrap_or(0) as u32;
    
    let age_secs = get_age_secs(&r.metadata.creation_timestamp);
    
    let severity = if ready == desired {
        Severity::Good
    } else if ready == 0 && desired > 0 {
        Severity::Bad
    } else {
        Severity::Warn
    };
    
    ResourceRow {
        namespace: namespace.clone(),
        name: name.clone(),
        cells: vec![
            namespace.unwrap_or_default(),
            name,
            desired.to_string(),
            current.to_string(),
            ready.to_string(),
            format_age(age_secs),
        ],
        status_col: Some(4),
        severity,
    }
}

pub fn statefulset_row(s: &StatefulSet) -> ResourceRow {
    let namespace = s.metadata.namespace.clone();
    let name = s.metadata.name.clone().unwrap_or_default();
    
    let spec_replicas = s.spec.as_ref().and_then(|spec| spec.replicas).unwrap_or(0) as u32;
    let status = s.status.as_ref();
    let ready_replicas = status.and_then(|s| s.ready_replicas).unwrap_or(0) as u32;
    
    let age_secs = get_age_secs(&s.metadata.creation_timestamp);
    
    let severity = if ready_replicas == spec_replicas {
        Severity::Good
    } else if ready_replicas == 0 {
        Severity::Bad
    } else {
        Severity::Warn
    };
    
    ResourceRow {
        namespace: namespace.clone(),
        name: name.clone(),
        cells: vec![
            namespace.unwrap_or_default(),
            name,
            format!("{}/{}", ready_replicas, spec_replicas),
            format_age(age_secs),
        ],
        status_col: Some(2),
        severity,
    }
}

pub fn service_row(s: &Service) -> ResourceRow {
    let namespace = s.metadata.namespace.clone();
    let name = s.metadata.name.clone().unwrap_or_default();
    
    let spec = s.spec.as_ref();
    let svc_type = spec.and_then(|sp| sp.type_.as_deref()).unwrap_or("ClusterIP");
    let cluster_ip = spec.and_then(|sp| sp.cluster_ip.as_deref()).unwrap_or("<none>");
    
    let external_ip = s.status.as_ref()
        .and_then(|st| st.load_balancer.as_ref())
        .and_then(|lb| lb.ingress.as_ref())
        .map(|ingresses| {
            ingresses.iter()
                .map(|ing| ing.ip.as_deref().or(ing.hostname.as_deref()).unwrap_or(""))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(",")
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "<none>".to_string());
        
    let ports = spec.and_then(|sp| sp.ports.as_ref())
        .map(|ports| {
            ports.iter()
                .map(|p| {
                    let proto = p.protocol.as_deref().unwrap_or("TCP");
                    if let Some(node_port) = p.node_port {
                        format!("{}:{}/{}", p.port, node_port, proto)
                    } else {
                        format!("{}/{}", p.port, proto)
                    }
                })
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default();
        
    let age_secs = get_age_secs(&s.metadata.creation_timestamp);
    
    ResourceRow {
        namespace: namespace.clone(),
        name: name.clone(),
        cells: vec![
            namespace.unwrap_or_default(),
            name,
            svc_type.to_string(),
            cluster_ip.to_string(),
            external_ip,
            ports,
            format_age(age_secs),
        ],
        status_col: None,
        severity: Severity::Neutral,
    }
}

pub fn ingress_row(i: &Ingress) -> ResourceRow {
    let namespace = i.metadata.namespace.clone();
    let name = i.metadata.name.clone().unwrap_or_default();
    
    let spec = i.spec.as_ref();
    let class = spec.and_then(|s| s.ingress_class_name.clone())
        .or_else(|| {
            i.metadata.annotations.as_ref()
                .and_then(|a| a.get("kubernetes.io/ingress.class").cloned())
        })
        .unwrap_or_else(|| "<none>".to_string());
        
    let hosts = spec.and_then(|s| s.rules.as_ref())
        .map(|rules| {
            rules.iter()
                .filter_map(|r| r.host.clone())
                .collect::<Vec<_>>()
                .join(",")
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "*".to_string());
        
    let address = i.status.as_ref()
        .and_then(|st| st.load_balancer.as_ref())
        .and_then(|lb| lb.ingress.as_ref())
        .map(|ingresses| {
            ingresses.iter()
                .map(|ing| ing.ip.as_deref().or(ing.hostname.as_deref()).unwrap_or(""))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(",")
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_default();
        
    let ports = spec.and_then(|s| s.tls.as_ref())
        .map(|_| "80, 443".to_string())
        .unwrap_or_else(|| "80".to_string());
        
    let age_secs = get_age_secs(&i.metadata.creation_timestamp);
    
    ResourceRow {
        namespace: namespace.clone(),
        name: name.clone(),
        cells: vec![
            namespace.unwrap_or_default(),
            name,
            class,
            hosts,
            address,
            ports,
            format_age(age_secs),
        ],
        status_col: None,
        severity: Severity::Neutral,
    }
}

pub fn node_row(n: &Node) -> ResourceRow {
    let name = n.metadata.name.clone().unwrap_or_default();
    
    let status_str = n.status.as_ref()
        .and_then(|s| s.conditions.as_ref())
        .and_then(|conds| {
            conds.iter()
                .find(|c| c.type_ == "Ready")
                .map(|c| if c.status == "True" { "Ready" } else { "NotReady" })
        })
        .unwrap_or("Unknown");
        
    let roles = n.metadata.labels.as_ref()
        .map(|labels| {
            labels.keys()
                .filter_map(|k| {
                    if k.starts_with("node-role.kubernetes.io/") {
                        k.strip_prefix("node-role.kubernetes.io/")
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join(",")
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "<none>".to_string());
        
    let version = n.status.as_ref()
        .and_then(|s| s.node_info.as_ref())
        .map(|ni| ni.kubelet_version.clone())
        .unwrap_or_default();
        
    let age_secs = get_age_secs(&n.metadata.creation_timestamp);
    
    let severity = if status_str == "Ready" {
        Severity::Good
    } else {
        Severity::Bad
    };
    
    ResourceRow {
        namespace: None,
        name: name.clone(),
        cells: vec![
            name,
            status_str.to_string(),
            roles,
            version,
            "N/A".to_string(),
            "N/A".to_string(),
            format_age(age_secs),
        ],
        status_col: Some(1),
        severity,
    }
}

pub fn namespace_row(ns: &Namespace) -> ResourceRow {
    let name = ns.metadata.name.clone().unwrap_or_default();
    let status = ns.status.as_ref()
        .and_then(|s| s.phase.as_deref())
        .unwrap_or("Active");
    let age_secs = get_age_secs(&ns.metadata.creation_timestamp);
    
    let severity = if status == "Active" {
        Severity::Good
    } else {
        Severity::Warn
    };
    
    ResourceRow {
        namespace: None,
        name: name.clone(),
        cells: vec![
            name,
            status.to_string(),
            format_age(age_secs),
        ],
        status_col: Some(1),
        severity,
    }
}

pub fn configmap_row(cm: &ConfigMap) -> ResourceRow {
    let namespace = cm.metadata.namespace.clone();
    let name = cm.metadata.name.clone().unwrap_or_default();
    let data_count = cm.data.as_ref().map(|d| d.len()).unwrap_or(0)
        + cm.binary_data.as_ref().map(|d| d.len()).unwrap_or(0);
    let age_secs = get_age_secs(&cm.metadata.creation_timestamp);
    
    ResourceRow {
        namespace: namespace.clone(),
        name: name.clone(),
        cells: vec![
            namespace.unwrap_or_default(),
            name,
            data_count.to_string(),
            format_age(age_secs),
        ],
        status_col: None,
        severity: Severity::Neutral,
    }
}

pub fn secret_row(s: &Secret) -> ResourceRow {
    let namespace = s.metadata.namespace.clone();
    let name = s.metadata.name.clone().unwrap_or_default();
    let secret_type = s.type_.clone().unwrap_or_else(|| "Opaque".to_string());
    let data_count = s.data.as_ref().map(|d| d.len()).unwrap_or(0)
        + s.string_data.as_ref().map(|d| d.len()).unwrap_or(0);
    let age_secs = get_age_secs(&s.metadata.creation_timestamp);
    
    ResourceRow {
        namespace: namespace.clone(),
        name: name.clone(),
        cells: vec![
            namespace.unwrap_or_default(),
            name,
            secret_type,
            data_count.to_string(),
            format_age(age_secs),
        ],
        status_col: None,
        severity: Severity::Neutral,
    }
}

pub fn event_row(e: &Event) -> ResourceRow {
    let namespace = e.metadata.namespace.clone();
    let event_type_str = e.type_.as_deref().unwrap_or("Normal");
    let reason = e.reason.clone().unwrap_or_default();
    let object = format!("{}/{}", e.involved_object.kind.as_deref().unwrap_or(""), e.involved_object.name.as_deref().unwrap_or(""));
    let message = e.message.clone().unwrap_or_default();
    let count = e.count.unwrap_or(1);
    
    let age_secs = e.last_timestamp.as_ref()
        .map(|ts| {
            let now = k8s_openapi::chrono::Utc::now();
            (now - ts.0).num_seconds().max(0) as u64
        })
        .unwrap_or_else(|| get_age_secs(&e.metadata.creation_timestamp));
        
    let severity = if event_type_str == "Warning" {
        Severity::Warn
    } else {
        Severity::Neutral
    };
    
    ResourceRow {
        namespace: namespace.clone(),
        name: object.clone(),
        cells: vec![
            namespace.unwrap_or_default(),
            event_type_str.to_string(),
            reason,
            object,
            message,
            count.to_string(),
            format_age(age_secs),
        ],
        status_col: Some(1),
        severity,
    }
}

pub fn pvc_row(p: &PersistentVolumeClaim) -> ResourceRow {
    let namespace = p.metadata.namespace.clone();
    let name = p.metadata.name.clone().unwrap_or_default();
    
    let status_str = p.status.as_ref()
        .and_then(|s| s.phase.as_deref())
        .unwrap_or("Unknown");
        
    let volume = p.spec.as_ref()
        .and_then(|s| s.volume_name.clone())
        .unwrap_or_default();
        
    let capacity = p.status.as_ref()
        .and_then(|s| s.capacity.as_ref())
        .and_then(|c| c.get("storage"))
        .map(|qty| qty.0.clone())
        .unwrap_or_default();
        
    let storage_class = p.spec.as_ref()
        .and_then(|s| s.storage_class_name.clone())
        .unwrap_or_default();
        
    let age_secs = get_age_secs(&p.metadata.creation_timestamp);
    
    let severity = if status_str == "Bound" {
        Severity::Good
    } else if status_str == "Pending" {
        Severity::Warn
    } else {
        Severity::Bad
    };
    
    ResourceRow {
        namespace: namespace.clone(),
        name: name.clone(),
        cells: vec![
            namespace.unwrap_or_default(),
            name,
            status_str.to_string(),
            volume,
            capacity,
            storage_class,
            format_age(age_secs),
        ],
        status_col: Some(2),
        severity,
    }
}
