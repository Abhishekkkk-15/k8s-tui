use k8s_openapi::api::core::v1::Pod;
use kube::{Api, Client, api::ListParams};

pub struct Daemon {
    pub client: Client,
}

impl Daemon {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let client = Client::try_default().await?;
        let deamon = Self { client };
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
}
