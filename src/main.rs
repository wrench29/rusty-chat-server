use std::io::Write;
use std::net::TcpListener;

use anyhow::Result;

fn main() -> Result<()> {
    let address = "127.0.0.1:6969";

    let listener = TcpListener::bind(address).map_err(|err| {
        eprintln!("Error: Could not bind {address} to the server.");
        err
    })?;

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let _ = writeln!(stream, "Hello World").map_err(|err| {
                    eprintln!("Error: Could not write a message to the stream ({err}).");
                });
            }
            Err(err) => {
                eprintln!("Error: Could not accept an incoming connection ({err}).");
            }
        }
    }

    println!("Hello, world!");

    Ok(())
}
