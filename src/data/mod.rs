mod mock;
mod model;

pub use mock::{MockBackend, ResourceRow};
pub use model::*;


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

