#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    denet_observability::init("denet-sensor-worker");
    tokio::signal::ctrl_c().await?;
    Ok(())
}
