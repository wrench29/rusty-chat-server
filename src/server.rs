use std::collections::HashMap;

use log::info;
use serde::{Deserialize, Serialize};
use serde_json::from_str;

use crate::{
    server_database::{ServerDatabase, UserCredentialsRaw},
    user_service::{AuthenticationError, RegistrationError, UserService},
};

pub enum ChatServerResponseCommand {
    SendToAll(Vec<u8>),
    SendToAllExcept(String, Vec<u8>),
    SendToSome(Vec<String>, Vec<u8>),
    DisconnectUser(String),
}

#[derive(Serialize, Deserialize)]
enum ChatRequest {
    Authentication {
        user_credentials_raw: UserCredentialsRaw,
    },
    Registration {
        user_credentials_raw: UserCredentialsRaw,
    },
    Message {
        message: String,
    },
}

#[derive(Serialize, Deserialize)]
enum ChatResponse {
    AuthenticationResult {
        result: bool,
        error: Option<AuthenticationError>,
    },
    RegistrationResult {
        result: bool,
        error: Option<RegistrationError>,
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

struct UserData {
    authenticated: bool,
    name: Option<String>,
}

struct ChatState {
    users: HashMap<String, UserData>,
}

pub struct ChatServer<T: ServerDatabase> {
    state: ChatState,
    user_service: UserService<T>,
}

impl<T: ServerDatabase> ChatServer<T> {
    pub fn new(user_service: UserService<T>) -> Self {
        Self {
            state: ChatState {
                users: HashMap::new(),
            },
            user_service,
        }
    }
    pub fn on_user_connect(&mut self, user_id: String) {
        info!("User {user_id} has connected.");
        self.state.users.insert(
            user_id,
            UserData {
                authenticated: false,
                name: None,
            },
        );
    }
    pub fn on_user_disconnect(&mut self, user_id: String) -> Option<ChatServerResponseCommand> {
        let user = self.state.users.get_mut(&user_id)?;

        if user.authenticated {
            let user_name = user.name.as_ref().unwrap();

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
        let is_authenticated = self.state.users.get(&user_id)?.authenticated;

        if is_authenticated {
            self.process_request_authenticated(&user_id, request)
        } else {
            self.process_request_unauthenticated(&user_id, request)
        }
    }

    fn process_request_authenticated(
        &mut self,
        user_id: &str,
        request: ChatRequest,
    ) -> Option<Vec<ChatServerResponseCommand>> {
        if let ChatRequest::Message { message } = request {
            let user_name = self.state.users.get(user_id)?.name.as_ref()?;

            info!("User {user_id} with name {user_name} has sent message '{message}'.",);

            let response = ChatResponse::Message {
                user_name: user_name.to_string(),
                message,
            };

            return Some(vec![
                self.make_response_to_all_authenticated(user_id, &response)
            ]);
        }
        None
    }
    fn process_request_unauthenticated(
        &mut self,
        user_id: &str,
        request: ChatRequest,
    ) -> Option<Vec<ChatServerResponseCommand>> {
        match request {
            ChatRequest::Authentication {
                user_credentials_raw,
            } => self.authenticate(user_id, &user_credentials_raw),
            ChatRequest::Registration {
                user_credentials_raw,
            } => self.register(user_id, &user_credentials_raw),
            ChatRequest::Message { message: _ } => None,
        }
    }

    fn register(
        &mut self,
        user_id: &str,
        user_credentials_raw: &UserCredentialsRaw,
    ) -> Option<Vec<ChatServerResponseCommand>> {
        match self.user_service.add_user(user_credentials_raw) {
            Ok(_) => {
                info!(
                    "User {user_id} has registered with name '{}'.",
                    user_credentials_raw.name
                );

                Some(vec![Self::make_response_to_user(
                    user_id,
                    &ChatResponse::RegistrationResult {
                        result: true,
                        error: None,
                    },
                )])
            }
            Err(e) => {
                info!(
                    "User {user_id} could not register with name '{}', disconnecting.",
                    user_credentials_raw.name
                );

                Some(vec![Self::make_response_to_user(
                    user_id,
                    &ChatResponse::RegistrationResult {
                        result: false,
                        error: Some(e),
                    },
                )])
            }
        }
    }

    fn authenticate(
        &mut self,
        user_id: &str,
        user_credentials_raw: &UserCredentialsRaw,
    ) -> Option<Vec<ChatServerResponseCommand>> {
        match self.user_service.authenticate_user(user_credentials_raw) {
            Ok(_) => {
                let user_data = self.state.users.get_mut(user_id)?;
                user_data.authenticated = true;
                user_data.name = Some(user_credentials_raw.name.clone());

                info!(
                    "User {user_id} has authenticated with name '{}'.",
                    user_credentials_raw.name
                );

                Some(vec![
                    Self::make_response_to_user(
                        user_id,
                        &ChatResponse::AuthenticationResult {
                            result: true,
                            error: None,
                        },
                    ),
                    self.make_response_to_all_authenticated(
                        user_id,
                        &ChatResponse::Connection {
                            user_name: user_credentials_raw.name.clone(),
                            is_connected: true,
                        },
                    ),
                ])
            }
            Err(e) => {
                info!(
                    "User {user_id} could not authenticate with name '{}'.",
                    user_credentials_raw.name
                );

                Some(vec![Self::make_response_to_user(
                    user_id,
                    &ChatResponse::AuthenticationResult {
                        result: false,
                        error: Some(e),
                    },
                )])
            }
        }
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

    fn make_response_to_all_authenticated(
        &self,
        sender_user_id: &str,
        response: &ChatResponse,
    ) -> ChatServerResponseCommand {
        let users = {
            let mut authenticated_users = Vec::<String>::new();
            for (user_id, user_data) in &self.state.users {
                if user_id == sender_user_id {
                    continue;
                }
                if user_data.authenticated {
                    authenticated_users.push(user_id.to_string());
                }
            }
            authenticated_users
        };
        let message = serde_json::to_string(response).unwrap();
        ChatServerResponseCommand::SendToSome(users, message.into_bytes())
    }
}
