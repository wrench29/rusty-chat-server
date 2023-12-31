use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use tokio::io;
use tokio::net::{TcpListener, TcpStream};

use uuid::Uuid;

struct ChatServer {
    listener: Rc<TcpListener>,
    connections: HashMap<String, Arc<Mutex<TcpStream>>>,
    address: String,
}

impl ChatServer {
    pub async fn create_async(host: &str, port: u16) -> Result<Self, ()> {
        let address = format!("{host}:{port}");

        let address_ref = &address;
        let listener = TcpListener::bind(address_ref).await.map_err(|err| {
            eprintln!("Error: Could not bind {address_ref} to the server ({err}).");
        })?;

        Ok(Self {
            listener: Rc::new(listener),
            connections: HashMap::new(),
            address,
        })
    }

    pub async fn run(mut self) {
        println!(
            "Log: Started accepting connections at {address}.",
            address = self.address
        );

        let listener = Rc::clone(&self.listener);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => self.handle_incoming_tcp_stream(stream).await,
                Err(err) => {
                    eprintln!("Error: Could not accept an incoming connection ({err}).");
                }
            }
        }
    }

    async fn handle_incoming_tcp_stream(&mut self, stream: TcpStream) {
        let new_uuid = Uuid::new_v4().to_string();

        self.connections
            .insert(new_uuid.clone(), Arc::new(Mutex::new(stream)));

        let stream_mutex = self.connections.get(&new_uuid).unwrap().clone();
        
        {
            let stream = stream_mutex.lock().unwrap();

            let client_address = stream
                .peer_addr()
                .map(|r| r.to_string())
                .unwrap_or("UNKNOWN_ADDRESS".to_string());
    
            println!("Log: New connection ({client_address}) with UUID {new_uuid}.");
    
            let write_result = Self::write_to_stream(&stream, b"Hello World").await;
            if write_result.is_err() {
                let e = write_result.err().unwrap();
                eprintln!("Error: Could not write data into stream of {new_uuid} ({e}).");
                self.connections.remove(&new_uuid);
                return;
            }
        }

        Self::connection_listener(new_uuid.clone(), Arc::clone(&stream_mutex)).await;

        self.connections.remove(&new_uuid);

        println!("Log: Connection {new_uuid} has been closed.");
    }

    async fn connection_listener(connection_id: String, stream: Arc<Mutex<TcpStream>>) {
        let stream = stream.lock().unwrap();

        let mut header_buffer: [u8; 4] = [0; 4];
        let header_result = Self::read_from_stream(&stream, &mut header_buffer).await;
        if header_result.is_err() {
            let e = header_result.err().unwrap();
            eprintln!("Error: Could not read header of the message from {connection_id} ({e}).");
            // Send message to close connection
            return;
        }

        // Header is 4 bytes long integer, representing message length
        let header = u32::from_le_bytes(header_buffer);
        println!("Debug: Message length is {header}.");
    }

    async fn read_from_stream(stream: &TcpStream, buf: &mut [u8]) -> io::Result<usize> {
        let mut cursor: usize = 0;
        loop {
            if cursor >= buf.len() {
                return Ok(buf.len());
            }

            stream.readable().await?;

            let mut current_slice = &mut buf[cursor..];

            match stream.try_read(&mut current_slice) {
                Ok(0) => break,
                Ok(n) => {
                    cursor += n;
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    continue;
                }
                Err(e) => {
                    return Err(e.into());
                }
            }
        }

        Ok(0)
    }

    async fn write_to_stream(stream: &TcpStream, buf: &[u8]) -> io::Result<()> {
        loop {
            stream.writable().await?;

            match stream.try_write(buf) {
                Ok(_) => {
                    break;
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    continue;
                }
                Err(e) => {
                    return Err(e.into());
                }
            }
        }
        Ok(())
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), ()> {
    let (host, port) = ("127.0.0.1", 6969);

    let chat_server = ChatServer::create_async(host, port).await?;

    chat_server.run().await;

    Ok(())
}
