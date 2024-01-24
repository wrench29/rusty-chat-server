use std::fmt;

use pwhash::bcrypt;
use serde::{Deserialize, Serialize};

use crate::server_database::{ServerDatabase, UserCredentials, UserCredentialsRaw};

#[derive(Debug, Serialize, Deserialize)]
pub enum AuthenticationError {
    WrongNameOrPassword,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum RegistrationError {
    IncorrectName(UserNameError),
    IncorrectPassword(PasswordError),
    NameAlreadyInUse,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum UserNameError {
    IncorrectLength(u32, u32),
    MultipleDots,
    MultipleUnderscores,
    UnallowedCharacter,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum PasswordError {
    IncorrectLength(u32, u32),
    UnallowedCharacter,
}

impl fmt::Display for AuthenticationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthenticationError::WrongNameOrPassword => write!(f, "wrong user name or password"),
        }
    }
}

impl fmt::Display for RegistrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegistrationError::IncorrectName(user_name_error) => {
                write!(f, "user name error: {user_name_error}")
            }
            RegistrationError::IncorrectPassword(password_error) => {
                write!(f, "password error: {password_error}")
            }
            RegistrationError::NameAlreadyInUse => write!(f, "name is already taken"),
        }
    }
}

impl fmt::Display for UserNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UserNameError::IncorrectLength(min, max) => {
                write!(f, "incorrect length, should be between {min} and {max}")
            }
            UserNameError::MultipleDots => write!(f, "cannot use multiple dots in succession"),
            UserNameError::MultipleUnderscores => {
                write!(f, "cannot use multiple underscores in succession")
            }
            UserNameError::UnallowedCharacter => {
                write!(
                    f,
                    "unallowed character, allowed only alphanumeric ASCII symbols"
                )
            }
        }
    }
}

impl fmt::Display for PasswordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PasswordError::IncorrectLength(min, max) => {
                write!(f, "incorrect length, should be between {min} and {max}")
            }
            PasswordError::UnallowedCharacter => {
                write!(f, "unallowed character, allowed only graphic ASCII symbols")
            }
        }
    }
}

impl From<UserNameError> for RegistrationError {
    fn from(value: UserNameError) -> Self {
        Self::IncorrectName(value)
    }
}

impl From<PasswordError> for RegistrationError {
    fn from(value: PasswordError) -> Self {
        Self::IncorrectPassword(value)
    }
}

pub struct UserService<T: ServerDatabase> {
    db: T,
}

impl<T: ServerDatabase> UserService<T> {
    pub fn new(database: T) -> Self {
        Self { db: database }
    }

    pub fn check_user(&self, name: &str) {
        if let Some(user_credentials) = self.db.get_user_by_name(name) {
            println!(
                "User found: {}:{}",
                user_credentials.name, user_credentials.password_hash
            );
        } else {
            println!("User not found");
        }
    }

    pub fn authenticate_user(
        &self,
        user_credentials_raw: &UserCredentialsRaw,
    ) -> Result<(), AuthenticationError> {
        let user_credentials = self.db.get_user_by_name(&user_credentials_raw.name);
        match user_credentials {
            Some(user_credentials) => {
                if bcrypt::verify(
                    user_credentials_raw.password.clone(),
                    &user_credentials.password_hash,
                ) {
                    Ok(())
                } else {
                    Err(AuthenticationError::WrongNameOrPassword)
                }
            }
            None => Err(AuthenticationError::WrongNameOrPassword),
        }
    }

    pub fn add_user(
        &self,
        user_credentials_raw: &UserCredentialsRaw,
    ) -> Result<(), RegistrationError> {
        Self::verify_name(&user_credentials_raw.name)?;
        if self
            .db
            .get_user_by_name(&user_credentials_raw.name)
            .is_some()
        {
            return Err(RegistrationError::NameAlreadyInUse);
        }
        Self::verify_password(&user_credentials_raw.password)?;

        let password_hash = bcrypt::hash(user_credentials_raw.password.clone())
            .expect("system rng should be available");

        let user_credentials = UserCredentials {
            name: user_credentials_raw.name.clone(),
            password_hash,
        };

        self.db.add_new_user(&user_credentials);

        Ok(())
    }

    fn verify_name(name: &str) -> Result<(), UserNameError> {
        // Q: UUUHH WHY NOT USE REGULAR EXPRESSION HUH???!?!?!
        // A: iДi нахуй

        if name.len() < 7 || name.len() > 32 {
            return Err(UserNameError::IncorrectLength(7, 32));
        }

        let mut was_dot = false;
        let mut was_underscore = false;
        for ch in name.chars() {
            if ch.is_ascii_alphanumeric() {
                was_dot = false;
                was_underscore = false;
                continue;
            }

            if ch == '.' {
                if was_dot {
                    return Err(UserNameError::MultipleDots);
                } else {
                    was_dot = true;
                    continue;
                }
            }
            if ch == '_' {
                if was_underscore {
                    return Err(UserNameError::MultipleUnderscores);
                } else {
                    was_underscore = true;
                    continue;
                }
            }

            return Err(UserNameError::UnallowedCharacter);
        }

        Ok(())
    }

    fn verify_password(password: &str) -> Result<(), PasswordError> {
        if password.len() < 8 || password.len() > 32 {
            return Err(PasswordError::IncorrectLength(8, 32));
        }

        for ch in password.chars() {
            if ch.is_ascii_graphic() {
                continue;
            }

            return Err(PasswordError::UnallowedCharacter);
        }

        Ok(())
    }
}
