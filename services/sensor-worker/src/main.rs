#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dennett_observability::init("dennett-sensor-worker");
    tokio::signal::ctrl_c().await?;
    Ok(())
}
