#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    denet_observability::init("denet-memoryd");
    tokio::signal::ctrl_c().await?;
    Ok(())
}
