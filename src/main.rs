use kube::{Client, Api};
use k8s_openapi::api::core::v1::Pod;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Infers configuration from ~/.kube/config or in-cluster environment
    let client = Client::try_default().await?;
    let pods: Api<Pod> = Api::default_namespaced(client);

    for p in pods.list(&Default::default()).await? {
        println!("Found Pod: {}", p.metadata.name.unwrap());
    }
    Ok(())
}
