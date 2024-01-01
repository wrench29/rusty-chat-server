use server::ChatServer;

mod server;

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<(), ()> {
    let (host, port) = ("127.0.0.1", 6969);

    let chat_server = ChatServer::create_async(host, port).await?;

    chat_server.run().await;

    Ok(())
}
