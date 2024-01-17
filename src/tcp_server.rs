use std::collections::HashMap;
use std::sync::Arc;

use log::{error, info, warn};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpListener;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::Mutex;
use tokio::task::yield_now;
use tokio::{io, signal};
use uuid::Uuid;

use crate::server::{ChatServer, ChatServerMessage};

pub struct ChatTcpServer {
    listener: Arc<TcpListener>,
    address: String,
}

struct ChatServerStreams {
    write_stream: Arc<Mutex<OwnedWriteHalf>>,
    read_stream: Arc<Mutex<OwnedReadHalf>>,
}

struct ChatServerState {
    connections: HashMap<String, Arc<ChatServerStreams>>,
    chat_server: ChatServer,
}

enum ChatServerEvent {
    UserConnected(String, Arc<ChatServerStreams>),
    UserDisconnected(String),
    UserSentMessage(String, String),
    CtrlCPressed,
}

impl ChatTcpServer {
    pub async fn create_async(host: &str, port: u16) -> Result<Self, ()> {
        let address = format!("{host}:{port}");

        let address_ref = &address;
        let listener = TcpListener::bind(address_ref).await.map_err(|err| {
            error!("Could not bind {address_ref} to the server ({err}).");
        })?;

        Ok(Self {
            listener: Arc::new(listener),
            address,
        })
    }

    pub async fn run(self) {
        info!(
            "** Started accepting connections at {address}. **",
            address = self.address
        );

        let state = Arc::new(Mutex::new(ChatServerState {
            connections: HashMap::new(),
            chat_server: ChatServer::new(),
        }));

        let (event_sender, event_receiver) = channel::<ChatServerEvent>(32);
        let (message_sender, message_receiver) = channel::<ChatServerMessage>(32);

        state
            .lock()
            .await
            .chat_server
            .set_message_sender(message_sender.clone());

        let event_handler_handle = tokio::spawn(event_handler(state.clone(), event_receiver));
        let sender_handle = tokio::spawn(message_send_handler(state.clone(), message_receiver));
        let listener_handle = tokio::spawn(tcp_listener_loop(
            Arc::clone(&self.listener),
            event_sender.clone(),
        ));

        signal::ctrl_c().await.unwrap();

        warn!("** Detected CTRL^C, stopping the server... **");

        event_sender
            .send(ChatServerEvent::CtrlCPressed)
            .await
            .unwrap();

        message_sender
            .send(ChatServerMessage::StopThread)
            .await
            .unwrap();

        yield_now().await;

        let _ = event_handler_handle.await;

        listener_handle.abort();
        sender_handle.abort();

        info!("** Server has stopped successfully **");
    }
}

async fn tcp_listener_loop(listener: Arc<TcpListener>, event_sender: Sender<ChatServerEvent>) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let (stream_read, stream_write) = stream.into_split();
                tokio::spawn(handle_incoming_tcp_stream(
                    stream_write,
                    stream_read,
                    event_sender.clone(),
                ));
            }
            Err(err) => {
                error!("Could not accept an incoming connection ({err}).");
            }
        }
    }
}

async fn event_handler(
    state: Arc<Mutex<ChatServerState>>,
    mut receiver: Receiver<ChatServerEvent>,
) {
    loop {
        let received_state = receiver.recv().await;
        if received_state.is_none() {
            error!("Could not receive state from channel because it has been closed.");
            return;
        }
        let received_state = received_state.unwrap();

        match received_state {
            ChatServerEvent::UserConnected(connection_id, streams) => {
                {
                    let client_address = streams
                        .read_stream
                        .lock()
                        .await
                        .peer_addr()
                        .map(|r| r.to_string())
                        .unwrap_or("UNKNOWN_ADDRESS".to_string());
                    info!("New connection ({client_address}) with ID {connection_id}.");
                }
                let mut state = state.lock().await;

                state.connections.insert(connection_id.clone(), streams);
                state.chat_server.on_user_connect(connection_id).await;
            }
            ChatServerEvent::UserDisconnected(connection_id) => {
                let mut state = state.lock().await;
                state.connections.remove(&connection_id);
                state
                    .chat_server
                    .on_user_disconnect(connection_id.clone())
                    .await;

                info!("Connection {connection_id} has been closed.");
            }
            ChatServerEvent::UserSentMessage(connection_id, message_str) => {
                state
                    .lock()
                    .await
                    .chat_server
                    .on_user_message(connection_id, message_str.as_bytes())
                    .await;
                yield_now().await;
            }
            ChatServerEvent::CtrlCPressed => {
                let mut state = state.lock().await;
                state.connections.clear();
                break;
            }
        }
    }
}

async fn message_send_handler(
    state: Arc<Mutex<ChatServerState>>,
    mut recevier: Receiver<ChatServerMessage>,
) {
    loop {
        let message = recevier.recv().await;
        if message.is_none() {
            error!("Could not receive message from channel because it has been closed.");
            return;
        }
        let message = message.unwrap();

        let message_to_send: Option<Vec<u8>>;
        let mut users_list: Option<Vec<String>> = None;

        match message {
            ChatServerMessage::SendToAll(message) => message_to_send = Some(message),
            ChatServerMessage::SendToAllExcept(connection_id_exception, message) => {
                message_to_send = Some(message);

                info!("Requested message distribution to all except self.");

                let state = state.lock().await;

                if state.connections.len() == 1 {
                    info!("There is no users to send the message.");
                    continue;
                }

                let mut users_list_new = Vec::<String>::new();
                state.connections.keys().for_each(|k| {
                    if k != &connection_id_exception {
                        users_list_new.push(k.to_string());
                    }
                });
                users_list = Some(users_list_new);
            }
            ChatServerMessage::SendToSome(connection_id_exceptions, message) => {
                message_to_send = Some(message);
                users_list = Some(connection_id_exceptions);
            }
            ChatServerMessage::StopThread => {
                return;
            }
            ChatServerMessage::DisconnectUser(connection_id) => {
                state.lock().await.connections.remove(&connection_id);
                return;
            }
        }

        let final_users_list = match users_list {
            Some(list) => list,
            None => {
                let state = state.lock().await;
                state.connections.keys().map(|k| k.to_string()).collect()
            }
        };

        let message_bytes = message_to_send.unwrap();

        for connection_id in final_users_list {
            let state = state.lock().await;
            let connection = state.connections.get(&connection_id).unwrap();
            info!("Sending to {connection_id}...");
            {
                let stream = connection.write_stream.lock().await;
                let write_result = write_message(&stream, &message_bytes).await;
                if write_result.is_err() {
                    let e = write_result.err().unwrap();
                    error!("Could not send message to connection {connection_id} ({e}).");
                    break;
                }
            }
            info!("Sent successfully to {connection_id}.");
        }
    }
}

async fn handle_incoming_tcp_stream(
    write_stream: OwnedWriteHalf,
    read_stream: OwnedReadHalf,
    event_sender: Sender<ChatServerEvent>,
) {
    let connection_id = Uuid::new_v4().to_string();

    let streams = ChatServerStreams {
        read_stream: Arc::new(Mutex::new(read_stream)),
        write_stream: Arc::new(Mutex::new(write_stream)),
    };

    let streams = Arc::new(streams);

    event_sender
        .send(ChatServerEvent::UserConnected(
            connection_id.clone(),
            streams.clone(),
        ))
        .await
        .unwrap();

    yield_now().await;

    loop {
        // get message, process it, send event
        let message = read_message(connection_id.clone(), streams.clone()).await;
        if message.is_err() {
            break;
        }
        let message = message.unwrap();
        if message.is_empty() {
            break;
        }

        let message_str = String::from_utf8(message);
        if message_str.is_err() {
            continue;
        }
        let message_str = message_str.unwrap();

        info!("Message from {connection_id}: '{message_str}'.");

        event_sender
            .send(ChatServerEvent::UserSentMessage(
                connection_id.clone(),
                message_str,
            ))
            .await
            .unwrap();

        yield_now().await;
    }

    event_sender
        .send(ChatServerEvent::UserDisconnected(connection_id))
        .await
        .unwrap();
}

async fn read_message(
    connection_id: String,
    stream: Arc<ChatServerStreams>,
) -> io::Result<Vec<u8>> {
    let stream = stream.read_stream.lock().await;

    let mut header_buffer: [u8; 4] = [0; 4];
    let header_result = read_from_stream(&stream, &mut header_buffer).await;
    if header_result.is_err() {
        let e = header_result.err().unwrap();
        error!("Could not read header of the message from {connection_id} ({e}).");
        return Err(e);
    }

    // Header is 4 bytes long integer, representing message length
    let header = u32::from_le_bytes(header_buffer);

    let mut buffer: Vec<u8> = vec![0; header as usize];

    let body_result = read_from_stream(&stream, &mut buffer).await;
    if body_result.is_err() {
        let e = header_result.err().unwrap();
        error!("Could not read body of the message from {connection_id} ({e}).");
        return Err(e);
    }

    Ok(buffer)
}

async fn write_message(stream: &OwnedWriteHalf, buf: &[u8]) -> io::Result<()> {
    let header = (buf.len() as u32).to_le_bytes();

    let write_result = write_to_stream(stream, &header).await;
    if write_result.is_err() {
        let e = write_result.err().unwrap();
        return Err(e);
    }

    let write_result = write_to_stream(stream, buf).await;
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
