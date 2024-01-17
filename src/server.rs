use std::collections::HashMap;

use log::info;
use regex::bytes::Regex;
use serde::{Deserialize, Serialize};
use serde_json::from_str;
use tokio::sync::mpsc::Sender;

pub enum ChatServerMessage {
    SendToAll(Vec<u8>),
    SendToAllExcept(String, Vec<u8>),
    SendToSome(Vec<String>, Vec<u8>),
    DisconnectUser(String),
    StopThread,
}

#[derive(Serialize, Deserialize)]
enum ChatRequest {
    Authentication { name: String },
    Message { message: String },
}

#[derive(Serialize, Deserialize)]
enum ChatResponse {
    AuthenticationResult {
        result: String,
    },
    Message {
        user_name: String,
        message: String,
    },
    Connection {
        user_name: String,
        is_connected: bool,
    },
}

struct UserCredentials {
    name: String,
}

struct UserData {
    authenticated: bool,
    credentials: Option<UserCredentials>,
}

struct ChatState {
    users: HashMap<String, UserData>,
}

pub struct ChatServer {
    sender: Option<Sender<ChatServerMessage>>,
    state: ChatState,
}

impl ChatServer {
    pub fn new() -> Self {
        Self {
            sender: None,
            state: ChatState {
                users: HashMap::new(),
            },
        }
    }
    pub fn set_message_sender(&mut self, sender: Sender<ChatServerMessage>) {
        self.sender = Some(sender);
    }
    pub async fn on_user_connect(&mut self, user_id: String) {
        info!("User {user_id} has connected.");
        self.state.users.insert(
            user_id,
            UserData {
                authenticated: false,
                credentials: None,
            },
        );
    }
    pub async fn on_user_disconnect(&mut self, user_id: String) {
        let user = self.state.users.get_mut(&user_id);
        if user.is_none() {
            return;
        }
        let user = user.unwrap();

        if user.authenticated {
            let user_name = user.credentials.as_ref().unwrap().name.to_string();

            self.send_response(&ChatResponse::Connection {
                user_name: user_name.clone(),
                is_connected: false,
            })
            .await;

            info!("User {user_id} with name {user_name} has disconnected.");
        } else {
            info!("User {user_id} has disconnected.");
        }

        self.state.users.remove(&user_id);
    }
    pub async fn on_user_message(&mut self, user_id: String, message: &[u8]) {
        let request = Self::message_to_request(message);
        if request.is_none() {
            return;
        }
        let request = request.unwrap();

        let user = self.state.users.get_mut(&user_id);
        if user.is_none() {
            return;
        }
        let user = user.unwrap();

        if user.authenticated {
            if let ChatRequest::Message { message } = request {
                let user_name = &user.credentials.as_ref().unwrap().name;

                info!("User {user_id} with name {user_name} has sent message '{message}'.",);

                let response = ChatResponse::Message {
                    user_name: user_name.to_string(),
                    message,
                };

                self.send_response(&response).await;
            }
        } else if let ChatRequest::Authentication { name } = request {
            if !Self::verify_name(&name) {
                info!("User {user_id} could not authenticate with name '{name}', disconnecting.");

                self.send_message(ChatServerMessage::DisconnectUser(user_id))
                    .await;
                return;
            }

            user.authenticated = true;
            user.credentials = Some(UserCredentials { name: name.clone() });

            info!("User {user_id} has authenticated with name '{name}'.");

            self.send_response(&ChatResponse::Connection {
                user_name: name.clone(),
                is_connected: true,
            })
            .await;
        }
    }

    fn verify_name(name: &str) -> bool {
        let regex_str = r"^(?=.{8,20}$)(?![_.])(?!.*[_.]{2})[a-zA-Z0-9._]+(?<![_.])$";
        let regex = Regex::new(regex_str).unwrap();

        regex.is_match(name.as_bytes())
    }

    fn message_to_request(message: &[u8]) -> Option<ChatRequest> {
        if let Ok(message) = String::from_utf8(message.to_vec()) {
            if let Ok(message) = from_str::<ChatRequest>(&message) {
                Some(message)
            } else {
                None
            }
        } else {
            None
        }
    }

    async fn send_response(&mut self, response: &ChatResponse) {
        let message = serde_json::to_string(response).unwrap();
        self.send_message(ChatServerMessage::SendToAll(message.into_bytes()))
            .await;
    }

    async fn send_message(&mut self, message: ChatServerMessage) {
        self.sender.as_mut().unwrap().send(message).await.unwrap();
    }
}
