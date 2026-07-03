//! Real Kubernetes integration lives here.
//!
//! The TUI currently runs entirely on mock data (see `src/data`). This module
//! is the starting point for wiring up a real `kube` client behind the same
//! shapes the UI already renders — nothing in `main.rs` calls this yet.

#![allow(dead_code)]

use k8s_openapi::api::core::v1::Pod;
use kube::{Api, Client};

pub async fn list_default_namespace_pods() -> Result<(), Box<dyn std::error::Error>> {
    // Infers configuration from ~/.kube/config or in-cluster environment
    let client = Client::try_default().await?;
    let pods: Api<Pod> = Api::default_namespaced(client);

    for p in pods.list(&Default::default()).await? {
        println!("Found Pod: {}", p.metadata.name.unwrap());
    }
    Ok(())
}
