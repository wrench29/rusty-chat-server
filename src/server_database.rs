use std::fs;

use serde::{Deserialize, Serialize};
use sqlite::{Connection, State};

pub struct UserCredentials {
    pub name: String,
    pub password_hash: String,
}

#[derive(Serialize, Deserialize)]
pub struct UserCredentialsRaw {
    pub name: String,
    pub password: String,
}

pub trait ServerDatabase {
    fn get_user_by_name(&self, name: &str) -> Option<UserCredentials>;
    fn add_new_user(&self, user_credentials: &UserCredentials);
}

pub struct ServerSQLiteDatabase {
    db: Connection,
}

impl Default for ServerSQLiteDatabase {
    fn default() -> Self {
        fs::create_dir_all("data").expect("should have rights to access the working directory");
        let connection = sqlite::open("data/database.sqlite").unwrap();

        let create_tables_query = "
            CREATE TABLE IF NOT EXISTS user_credentials (
                id INTEGER PRIMARY KEY AUTOINCREMENT, 
                name TEXT UNIQUE NOT NULL, 
                password_hash TEXT NOT NULL
            );
        ";

        connection.execute(create_tables_query).unwrap();

        Self { db: connection }
    }
}

impl ServerDatabase for ServerSQLiteDatabase {
    fn get_user_by_name(&self, name: &str) -> Option<UserCredentials> {
        let query = "SELECT * FROM user_credentials WHERE name = ?;";

        let mut statement = self.db.prepare(query).unwrap();
        statement.bind((1, name)).unwrap();
        if let Ok(State::Row) = statement.next() {
            let user_credentials = UserCredentials {
                name: statement.read::<String, _>("name").unwrap(),
                password_hash: statement.read::<String, _>("password_hash").unwrap(),
            };
            Some(user_credentials)
        } else {
            None
        }
    }

    fn add_new_user(&self, user_credentials: &UserCredentials) {
        let query = "INSERT INTO user_credentials (name, password_hash) VALUES (?, ?);";

        let mut statement = self.db.prepare(query).unwrap();
        statement.bind((1, user_credentials.name.as_str())).unwrap();
        statement
            .bind((2, user_credentials.password_hash.as_str()))
            .unwrap();
        statement.next().unwrap();
    }
}
