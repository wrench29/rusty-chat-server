use std::{collections::HashMap, io, sync::Arc};

use log::{error, info, warn};
use tokio::{
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpListener, TcpStream,
    },
    signal, spawn,
    sync::Mutex,
    task::yield_now,
};
use uuid::Uuid;

use crate::{
    server::{ChatServer, ChatServerResponseCommand},
    server_database::ServerDatabase,
};

pub struct ChatTcpServer<T: ServerDatabase> {
    address: String,
    listener: Arc<TcpListener>,
    connections: Arc<Mutex<HashMap<String, Arc<OwnedWriteHalf>>>>,
    chat_server: Arc<Mutex<ChatServer<T>>>,
}

impl<T: ServerDatabase + Send + 'static> ChatTcpServer<T> {
    pub async fn create_async(
        host: &str,
        port: u16,
        chat_server: ChatServer<T>,
    ) -> Result<Self, ()> {
        let address = format!("{host}:{port}");

        let address_ref = &address;
        let listener = TcpListener::bind(address_ref).await.map_err(|err| {
            error!("Could not bind {address_ref} to the server ({err}).");
        })?;

        Ok(Self {
            address,
            listener: Arc::new(listener),
            connections: Arc::new(Mutex::new(HashMap::new())),
            chat_server: Arc::new(Mutex::new(chat_server)),
        })
    }

    pub async fn run(self) {
        info!(
            "** Started accepting connections at {address}. **",
            address = self.address
        );

        let listener_handle = tokio::spawn(tcp_listener_loop(
            Arc::clone(&self.listener),
            self.connections.clone(),
            self.chat_server.clone(),
        ));

        signal::ctrl_c().await.unwrap();

        warn!("** Detected CTRL^C, stopping the server... **");

        yield_now().await;

        listener_handle.abort();

        info!("** Server has stopped successfully **");
    }
}

async fn tcp_listener_loop<T: ServerDatabase + Send + 'static>(
    listener: Arc<TcpListener>,
    connections: Arc<Mutex<HashMap<String, Arc<OwnedWriteHalf>>>>,
    chat_server: Arc<Mutex<ChatServer<T>>>,
) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(handle_incoming_tcp_stream(
                    stream,
                    connections.clone(),
                    chat_server.clone(),
                ));
            }
            Err(err) => {
                error!("Could not accept an incoming connection ({err}).");
            }
        }
    }
}

async fn process_command(
    connections: Arc<Mutex<HashMap<String, Arc<OwnedWriteHalf>>>>,
    command: ChatServerResponseCommand,
) {
    let message_to_send: Option<Vec<u8>>;
    let mut users_list: Option<Vec<String>> = None;

    match command {
        ChatServerResponseCommand::SendToAll(message) => message_to_send = Some(message),
        ChatServerResponseCommand::SendToAllExcept(connection_id_exception, message) => {
            message_to_send = Some(message);

            info!("Requested message distribution to all except self.");

            let connections = connections.lock().await;

            if connections.len() == 1 {
                info!("There is no users to send the message.");
                return;
            }

            let mut users_list_new = Vec::<String>::new();
            connections.keys().for_each(|k| {
                if k != &connection_id_exception {
                    users_list_new.push(k.to_string());
                }
            });
            users_list = Some(users_list_new);
        }
        ChatServerResponseCommand::SendToSome(connection_id_exceptions, message) => {
            message_to_send = Some(message);
            users_list = Some(connection_id_exceptions);
        }
        ChatServerResponseCommand::DisconnectUser(connection_id) => {
            let mut connections = connections.lock().await;
            connections.remove(&connection_id);
            return;
        }
    }

    let final_users_list = match users_list {
        Some(list) => list,
        None => {
            let connections = connections.lock().await;
            connections.keys().map(|k| k.to_string()).collect()
        }
    };

    let message_bytes = message_to_send.unwrap();

    let mut join_handles = Vec::new();

    for connection_id in &final_users_list {
        let connections = connections.lock().await;
        let connection = if let Some(connection) = connections.get(connection_id) {
            connection.clone()
        } else {
            continue;
        };

        info!("Sending to {connection_id}...");
        join_handles.push(spawn(write_message(connection, message_bytes.clone())));
    }

    let mut i = 0;
    for handle in join_handles {
        let write_result = if let Ok(result) = handle.await {
            result
        } else {
            i += 1;
            continue;
        };
        let connection_id = &final_users_list[i];
        if let Err(e) = write_result {
            error!("Could not send message to connection {connection_id} ({e}).");
        } else {
            info!("Sent successfully to {connection_id}.");
        }

        i += 1;
    }
}

async fn handle_incoming_tcp_stream<T: ServerDatabase>(
    stream: TcpStream,
    connections: Arc<Mutex<HashMap<String, Arc<OwnedWriteHalf>>>>,
    chat_server: Arc<Mutex<ChatServer<T>>>,
) {
    let connection_id = Uuid::new_v4().to_string();

    let (read_stream, write_stream) = stream.into_split();

    connections
        .lock()
        .await
        .insert(connection_id.clone(), Arc::new(write_stream));

    chat_server
        .lock()
        .await
        .on_user_connect(connection_id.clone());

    loop {
        let message = read_message(connection_id.clone(), &read_stream).await;
        if message.is_err() {
            break;
        }
        let message = message.unwrap();
        if message.is_empty() {
            break;
        }

        let response_commands = chat_server
            .lock()
            .await
            .on_user_message(connection_id.clone(), &message);
        if let Some(commands) = response_commands {
            for command in commands {
                process_command(connections.clone(), command).await;
            }
        }
    }

    connections.lock().await.remove(&connection_id);

    let response_command = chat_server
        .lock()
        .await
        .on_user_disconnect(connection_id.clone());

    if let Some(command) = response_command {
        process_command(connections.clone(), command).await;
    }
}

async fn read_message(connection_id: String, stream: &OwnedReadHalf) -> io::Result<Vec<u8>> {
    let mut header_buffer: [u8; 4] = [0; 4];
    let header_result = read_from_stream(stream, &mut header_buffer).await;
    if header_result.is_err() {
        let e = header_result.err().unwrap();
        error!("Could not read header of the message from {connection_id} ({e}).");
        return Err(e);
    }

    // Header is 4 bytes long integer, representing message length
    let header = u32::from_le_bytes(header_buffer);

    let mut buffer: Vec<u8> = vec![0; header as usize];

    let body_result = read_from_stream(stream, &mut buffer).await;
    if body_result.is_err() {
        let e = header_result.err().unwrap();
        error!("Could not read body of the message from {connection_id} ({e}).");
        return Err(e);
    }

    Ok(buffer)
}

async fn write_message(stream: Arc<OwnedWriteHalf>, buf: Vec<u8>) -> io::Result<()> {
    let header = (buf.len() as u32).to_le_bytes();

    let write_result = write_to_stream(&stream, &header).await;
    if write_result.is_err() {
        let e = write_result.err().unwrap();
        return Err(e);
    }

    let write_result = write_to_stream(&stream, &buf).await;
    if write_result.is_err() {
        let e = write_result.err().unwrap();
        return Err(e);
    }
    Ok(())
}

async fn read_from_stream(stream: &OwnedReadHalf, buf: &mut [u8]) -> io::Result<usize> {
    let mut cursor: usize = 0;
    loop {
        if cursor >= buf.len() {
            return Ok(buf.len());
        }

        stream.readable().await?;

        let current_slice = &mut buf[cursor..];

        match stream.try_read(current_slice) {
            Ok(0) => break,
            Ok(n) => {
                cursor += n;
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                continue;
            }
            Err(e) => {
                return Err(e);
            }
        }
    }

    Ok(0)
}

async fn write_to_stream(stream: &OwnedWriteHalf, buf: &[u8]) -> io::Result<()> {
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
                return Err(e);
            }
        }
    }
    Ok(())
}
