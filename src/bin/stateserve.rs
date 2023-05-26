use std::collections::HashMap;

use clap::Command;
use log::{warn, info};
use mp5_usenet::{Listener, Context, StateRequest, StateResponse};
use anyhow::Result;
use tokio::{net::TcpStream, io::{AsyncWriteExt, AsyncReadExt}};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let mut clap = Command::new("stateserve");
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

    let default = Context::default();
    let mut sessions: HashMap<u64, Context> = HashMap::default();

    let mut buff = vec![0; 8192];
    loop {
        let session = reader.read_u64().await?;
        let index = reader.read_u64().await?;
        let msg = reader.read_u64().await? as usize;
        if 0 == reader.read_exact(&mut buff[0..msg]).await? {
            return Ok(());
        }
        let cmd: StateRequest = serde_json::from_slice(&buff[0..msg])?;
        match cmd {
            StateRequest::Group(name) => {
                info!("Setting group for {} to {}", session, name);
                let session_state = sessions.entry(session).or_insert(Context::default());
                session_state.group = name;
                let resp = serde_json::to_vec(&StateResponse::Nil)?;
                writer.write_u64(session).await?;
                writer.write_u64(index).await?;
                writer.write_u64(resp.len().try_into()?).await?;
                writer.write_all(&resp).await?;
            },
            StateRequest::Context => {
                let resp = sessions.get(&session).or_else(|| Some(&default)).unwrap();
                let resp = serde_json::to_vec(resp)?;
                writer.write_u64(session).await?;
                writer.write_u64(index).await?;
                writer.write_u64(resp.len().try_into()?).await?;
                writer.write_all(&resp).await?;
                info!("Sent message {}", String::from_utf8_lossy(&resp));
            }
            _ => unimplemented!(),
        }
    }
}