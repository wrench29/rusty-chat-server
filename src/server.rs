use std::collections::HashMap;

use log::info;
use serde::{Deserialize, Serialize};
use serde_json::from_str;

pub enum ChatServerResponseCommand {
    SendToAll(Vec<u8>),
    SendToAllExcept(String, Vec<u8>),
    SendToSome(Vec<String>, Vec<u8>),
    DisconnectUser(String),
}

#[derive(Serialize, Deserialize)]
enum ChatRequest {
    Authentication { name: String },
    Message { message: String },
}

#[derive(Serialize, Deserialize)]
enum ChatResponse {
    AuthenticationResult {
        result: bool,
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
    state: ChatState,
}

impl ChatServer {
    pub fn new() -> Self {
        Self {
            state: ChatState {
                users: HashMap::new(),
            },
        }
    }
    pub fn on_user_connect(&mut self, user_id: String) {
        info!("User {user_id} has connected.");
        self.state.users.insert(
            user_id,
            UserData {
                authenticated: false,
                credentials: None,
            },
        );
    }
    pub fn on_user_disconnect(&mut self, user_id: String) -> Option<ChatServerResponseCommand> {
        let user = self.state.users.get_mut(&user_id)?;

        if user.authenticated {
            let user_name = user.credentials.as_ref().unwrap().name.to_string();

            info!("User {user_id} with name {user_name} has disconnected.");

            Some(Self::make_response_to_all(&ChatResponse::Connection {
                user_name: user_name.clone(),
                is_connected: false,
            }))
        } else {
            info!("User {user_id} has disconnected.");
            self.state.users.remove(&user_id);
            None
        }
    }
    pub fn on_user_message(
        &mut self,
        user_id: String,
        message: &[u8],
    ) -> Option<Vec<ChatServerResponseCommand>> {
        let request = Self::message_to_request(message)?;
        let user = self.state.users.get_mut(&user_id)?;

        if user.authenticated {
            if let ChatRequest::Message { message } = request {
                let user_name = &user.credentials.as_ref().unwrap().name;

                info!("User {user_id} with name {user_name} has sent message '{message}'.",);

                let response = ChatResponse::Message {
                    user_name: user_name.to_string(),
                    message,
                };

                return Some(vec![Self::make_response_to_all(&response)]);
            }
        } else if let ChatRequest::Authentication { name } = request {
            if !Self::verify_name(&name) {
                info!("User {user_id} could not authenticate with name '{name}', disconnecting.");

                return Some(vec![ChatServerResponseCommand::DisconnectUser(user_id)]);
            }

            user.authenticated = true;
            user.credentials = Some(UserCredentials { name: name.clone() });

            info!("User {user_id} has authenticated with name '{name}'.");

            return Some(vec![
                Self::make_response_to_user(
                    &user_id,
                    &ChatResponse::AuthenticationResult { result: true },
                ),
                Self::make_response_to_all(&ChatResponse::Connection {
                    user_name: name.clone(),
                    is_connected: true,
                }),
            ]);
        }

        None
    }

    fn verify_name(name: &str) -> bool {
        // let regex_str = r"^(?=.{8,20}$)(?![_.])(?!.*[_.]{2})[a-zA-Z0-9._]+(?<![_.])$";
        // let regex = Regex::new(regex_str).unwrap();

        // regex.is_match(name.as_bytes())

        name.len() > 8 && name.len() < 20
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

    fn make_response_to_user(user_id: &str, response: &ChatResponse) -> ChatServerResponseCommand {
        let message = serde_json::to_string(response).unwrap();
        ChatServerResponseCommand::SendToSome(vec![user_id.to_string()], message.into_bytes())
    }

    fn make_response_to_all(response: &ChatResponse) -> ChatServerResponseCommand {
        let message = serde_json::to_string(response).unwrap();
        ChatServerResponseCommand::SendToAll(message.into_bytes())
    }
}
