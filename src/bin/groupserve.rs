use clap::Command;
use log::warn;
use mp5_usenet::{Listener, GroupRequest, GroupResponse, Group};
use anyhow::Result;
use tokio::{net::TcpStream, io::{AsyncWriteExt, AsyncReadExt}};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let mut clap = Command::new("groupserve");
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

    let mut buff = vec![0; 8192];
    loop {
        let session = reader.read_u64().await?;
        let index = reader.read_u64().await?;
        let msg = reader.read_u64().await? as usize;
        if 0 == reader.read_exact(&mut buff[0..msg]).await? {
            return Ok(());
        }
        let cmd: GroupRequest = serde_json::from_slice(&buff[0..msg])?;
        match cmd {
            GroupRequest::List => {
                let resp = GroupResponse::List(vec![
                    Group::new("alt.tacos", 100, 10, false),
                ]);
                let resp = serde_json::to_vec(&resp)?;
                writer.write_u64(session).await?;
                writer.write_u64(index).await?;
                writer.write_u64(resp.len().try_into()?).await?;
                writer.write_all(&resp).await?;
            }
            _ => unimplemented!(),
        }
    }
}