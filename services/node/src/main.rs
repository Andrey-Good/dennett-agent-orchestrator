#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dennett_observability::init("dennett-node");
    tokio::signal::ctrl_c().await?;
    Ok(())
}
