#[macro_use]
extern crate lazy_static;

use std::{sync::Arc, fs::File};

use clap::Command;
use log::{warn, info};
use mp5_usenet::{Listener, GroupClient, StateClient, FileClient, FileResponse};
use anyhow::{Result, anyhow};
use regex::Regex;
use tokio::{net::TcpStream, io::{AsyncWriteExt, BufReader, AsyncBufReadExt}};

lazy_static! {
    static ref LIST: Regex = Regex::new(r"(?i)list").unwrap();
    static ref GROUP: Regex = Regex::new(r"(?i)group (?<name>[a-z.]+)").unwrap();
    static ref ARTICLE: Regex = Regex::new(r"(?i)article (?<number>\d+)").unwrap();
}

struct Clients {
    group: GroupClient,
    state: StateClient,
    file: FileClient,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let mut clap = Command::new("gateway");
    clap = Listener::add_args(clap);
    clap = GroupClient::add_args(clap);
    clap = StateClient::add_args(clap);
    clap = FileClient::add_args(clap);
    let args = clap.get_matches();

    let listener = Listener::from_clap(&args).await?;

    let clients = Arc::new(Clients{
        group: GroupClient::from_clap(&args).await?,
        state: StateClient::from_clap(&args).await?,
        file: FileClient::from_clap(&args).await?,
    });
    let mut counter = 0;

    while let Ok(conn) = listener.conn.accept().await {
        let (conn, _) = conn;
        tokio::spawn(handle_connection(conn, clients.clone(), counter));
        counter += 1;
    }

    Ok(())
}

async fn handle_connection(conn: TcpStream, clients: Arc<Clients>, session: u64) {
    if let Err(e) = handle_connection_inner(conn, clients, session).await {
        warn!("Error handling client: {}", e);
    }
}

async fn handle_connection_inner(conn: TcpStream, clients: Arc<Clients>, session: u64) -> Result<()> {
    let (reader, mut writer) = conn.into_split();
    writer.write_all(b"200 amazing news server is ready\r\n").await?;

    // Read a line and dispatch to the correct service
    let mut line_reader = BufReader::new(reader);
    let mut line = String::with_capacity(1024);
    while 0 != line_reader.read_line(&mut line).await? {
        if LIST.is_match(&line) {
            let groups = clients.group.list(session).await?;
            writer.write_all(b"215 list of newsgroups follows\r\n").await?;
            for group in groups {
                writer.write_all(format!("{} {} {} {}\r\n", group.name, group.last, group.first, if group.posting_allowed {'y'} else {'n'}).as_bytes()).await?;
                writer.write_all(b".\r\n").await?;
            }
        } else if let Some(m) = GROUP.captures(&line) {
            let group = m.name("name").ok_or(anyhow!("No group specified"))?;
            clients.state.group(session, group.as_str().to_string()).await?;
            writer.write_all(format!("211 10 10 {} selected\r\n", group.as_str()).as_bytes()).await?;
        } else if let Some(m) = ARTICLE.captures(&line) {
            let context = clients.state.context(session).await?;
            let article = m.name("number").unwrap().as_str().parse()?;
            let article = clients.file.article(session, context.group, article).await?;
            info!("Got article response {:?}", article);
            if let FileResponse::Article(article) = article {
                writer.write_all(format!("220 {} <4105@ucbvax.ARPA> Article retrieved, text follows\r\n", article).as_bytes()).await?;
                writer.write_all(article.as_bytes()).await?;
                writer.write_all(b"\r\n.\r\n").await?;
            }
        }
        line.clear();
    }
    Ok(())
}