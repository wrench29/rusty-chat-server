use std::{io::Write, time::SystemTime};

use env_logger::fmt::Color;
use log::{error, warn, LevelFilter};

use server::ChatServer;
use time::{format_description::parse, OffsetDateTime};

mod config;
mod server;

fn get_ip_port_from_config() -> (String, u16) {
    let config_obj = config::read_config();

    const DEFAULT_HOST: &str = "127.0.0.1";
    const DEFAULT_PORT: u16 = 6969;

    if config_obj.is_err() {
        error!("{e}.", e = config_obj.err().unwrap());
        warn!("Using default values for ip and port.");
        return (DEFAULT_HOST.to_string(), DEFAULT_PORT);
    }
    let config_obj = config_obj.unwrap();

    let host = config_obj.network.ip.unwrap_or(DEFAULT_HOST.to_string());
    let port = config_obj.network.port.unwrap_or(DEFAULT_PORT);

    (host, port)
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<(), ()> {
    let mut logger_builder = env_logger::builder();
    logger_builder
        .filter_level(LevelFilter::max())
        .format_timestamp_secs()
        .format(|buf, record| {
            let mut style = buf.style();

            let offset_time: OffsetDateTime = SystemTime::now().into();
            let format_description =
                parse("[day].[month].[year] | [hour]:[minute]:[second]").unwrap();
            let formatted_time = offset_time.format(&format_description).unwrap();

            let style_bg_color = match record.level() {
                log::Level::Error => Color::Red,
                log::Level::Warn => Color::Yellow,
                log::Level::Info => Color::White,
                log::Level::Debug => Color::Cyan,
                log::Level::Trace => Color::White,
            };

            if style_bg_color == Color::White {
                style.set_color(Color::Black);
            }
            style.set_bg(style_bg_color);

            writeln!(
                buf,
                "[{}] {} {}",
                formatted_time,
                style.value(record.level()),
                record.args()
            )
        })
        .init();

    let (host, port) = get_ip_port_from_config();

    let chat_server = ChatServer::create_async(&host, port).await?;

    chat_server.run().await;

    Ok(())
}
