use std::collections::HashMap;

use clap::Command;
use log::{warn, info};
use mp5_usenet::{Listener, FileRequest, FileResponse};
use anyhow::Result;
use tokio::{net::TcpStream, io::{AsyncWriteExt, AsyncReadExt}};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let mut clap = Command::new("fileserve");
    clap = Listener::add_args(clap);
    let args = clap.get_matches();

    let listener = Listener::from_clap(&args).await?;

    while let Ok(conn) = listener.conn.accept().await {
        let (conn, _) = conn;
        tokio::spawn(handle_connection(conn));
    }

    Ok(())
}

async fn handle_connection(conn: TcpStream) {
    if let Err(e) = handle_connection_inner(conn).await {
        warn!("Error handling client: {}", e);
    }
}

async fn handle_connection_inner(conn: TcpStream) -> Result<()> {
    let (mut reader, mut writer) = conn.into_split();

    let articles = HashMap::from([
        (String::from("alt.tacos"), HashMap::from([
            (10u64, "From: cbosgd!mhuxj!mhuxt!eagle!jerry (Jerry Schwarz)
Newsgroups: net.general
Title: Usenet Etiquette -- Please Read
Article-I.D.: eagle.642
Posted: Fri Nov 19 16:14:55 1982
Received: Fri Nov 19 16:59:30 1982
Expires: Mon Jan  1 00:00:00 1990

The body of the article comes here, after a blank line."),
        ]))
    ]);

    let mut buff = vec![0; 8192];
    loop {
        let session = reader.read_u64().await?;
        let index = reader.read_u64().await?;
        let msg = reader.read_u64().await? as usize;
        if 0 == reader.read_exact(&mut buff[0..msg]).await? {
            return Ok(());
        }
        let cmd: FileRequest = serde_json::from_slice(&buff[0..msg])?;
        match cmd {
            FileRequest::Article(group, id) => {
                let article = articles.get(&group).unwrap().get(&id).unwrap();
                let resp = serde_json::to_vec(&FileResponse::Article(article.to_string()))?;
                info!("Sending response {}", String::from_utf8_lossy(&resp));
                writer.write_u64(session).await?;
                writer.write_u64(index).await?;
                writer.write_u64(resp.len().try_into()?).await?;
                writer.write_all(&resp).await?;
            }
            _ => unimplemented!(),
        }
    }
}