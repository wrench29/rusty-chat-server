use tokio::sync::mpsc::Sender;

// Latter argument is the message
pub enum ChatServerMessage {
    SendToAll(Vec<u8>),
    SendToAllExcept(String, Vec<u8>),
    SendToSome(Vec<String>, Vec<u8>),
    DisconnectUser(String),
    StopThread,
}

pub struct ChatServer {
    sender: Option<Sender<ChatServerMessage>>,
}

impl ChatServer {
    pub fn new() -> Self {
        Self { sender: None }
    }
    pub fn set_message_sender(&mut self, sender: Sender<ChatServerMessage>) {
        self.sender = Some(sender);
    }
    pub async fn on_user_connect(&mut self, user_id: String) {
        self.send_message(ChatServerMessage::SendToAll(
            format!("[Server] User {user_id} has connected.").into_bytes(),
        ))
        .await;
    }
    pub async fn on_user_disconnect(&mut self, user_id: String) {
        self.send_message(ChatServerMessage::SendToAll(
            format!("[Server] User {user_id} has disconnected.").into_bytes(),
        ))
        .await;
    }
    pub async fn on_user_message(&mut self, user_id: String, message: &[u8]) {
        let message = String::from_utf8(message.to_vec()).unwrap();

        self.send_message(ChatServerMessage::SendToAllExcept(
            user_id.clone(),
            format!("[{user_id}] {message}").into_bytes(),
        ))
        .await;
    }

    async fn send_message(&mut self, message: ChatServerMessage) {
        self.sender.as_mut().unwrap().send(message).await.unwrap();
    }
}
