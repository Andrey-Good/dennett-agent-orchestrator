#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dennett_observability::init("dennett-node");
    let config = dennett_node::NodeConfig::from_environment()?;
    dennett_node::run(config, async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "failed to wait for the explicit Node shutdown signal");
        }
    })
    .await?;
    Ok(())
}
