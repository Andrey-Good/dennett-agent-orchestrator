#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    denet_observability::init("denet-node");
    tokio::signal::ctrl_c().await?;
    Ok(())
}
