use std::{io::Write, time::SystemTime};

use env_logger::fmt::Color;
use log::LevelFilter;

use server::ChatServer;
use time::{format_description::parse, OffsetDateTime};

mod server;

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<(), ()> {
    let (host, port) = ("127.0.0.1", 6969);

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

    let chat_server = ChatServer::create_async(host, port).await?;

    chat_server.run().await;

    Ok(())
}
