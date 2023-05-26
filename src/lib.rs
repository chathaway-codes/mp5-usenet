use std::{collections::HashMap, sync::Arc};

use clap::{Command, ArgMatches, arg};
use log::{warn, info};
use serde::{Serialize, Deserialize, de::DeserializeOwned};
use tokio::{net::{TcpListener, TcpStream, tcp::{OwnedWriteHalf, OwnedReadHalf}}, io::{AsyncWriteExt, AsyncReadExt}, sync::{Mutex, oneshot::{Sender, self}}};
use anyhow::{Result, anyhow};
use async_trait::async_trait;

pub struct GroupClient {
    client: Client,
}

impl GroupClient {
    pub fn add_args(clap: Command) -> Command {
        clap.arg(arg!(-g --groupserve <LISTEN> "Address of groupserve"))
    }

    pub async fn from_clap(clap: &ArgMatches ) -> Result<GroupClient> {
        let addr = clap.get_one::<String>("groupserve")
            .ok_or(anyhow!("--groupserve not set"))?;
        let client = Client::new(addr).await?;
        Ok(Self{
            client: client,
        })
    }

    // List calls the groupserve to get a list of groups.
    pub async fn list(&self, session: u64) -> Result<Vec<Group>> {
        let resp: GroupResponse = self.client.call(session, GroupRequest::List).await?;
        if let GroupResponse::List(items) = resp {
            return Ok(items);
        }
        Err(anyhow!("Wrong response time"))
    }
}
pub struct StateClient {
    client: Client,
}

impl StateClient {
    pub fn add_args(clap: Command) -> Command {
        clap.arg(arg!(-s --stateserve <LISTEN> "Address of stateserve"))
    }

    pub async fn from_clap(clap: &ArgMatches ) -> Result<StateClient> {
        let addr = clap.get_one::<String>("stateserve")
            .ok_or(anyhow!("--stateserve not set"))?;
        let client = Client::new(addr).await?;
        Ok(Self{
            client: client,
        })
    }

    pub async fn group(&self, session: u64, group: String) -> Result<()> {
        let _: StateResponse = self.client.call(session, StateRequest::Group(group)).await?;
        Ok(())
    }

    pub async fn context(&self, session: u64) -> Result<Context> {
        let resp = self.client.call(session, StateRequest::Context).await?;
        Ok(resp)
    }
}
pub struct FileClient {
    client: Client,
}

impl FileClient {
    pub fn add_args(clap: Command) -> Command {
        clap.arg(arg!(-f --fileserve <LISTEN> "Address of fileserve"))
    }

    pub async fn from_clap(clap: &ArgMatches ) -> Result<FileClient> {
        let addr = clap.get_one::<String>("fileserve")
            .ok_or(anyhow!("--fileserve not set"))?;
        let client = Client::new(addr).await?;
        Ok(Self{
            client: client,
        })
    }

    pub async fn article(&self, session: u64, group: String, article: u64) -> Result<FileResponse> {
        let resp = self.client.call(session, FileRequest::Article(group, article)).await?;
        Ok(resp)
    }
}

#[async_trait]
trait HandleCallback {
    async fn handle(&self, session: u64, index: usize, bytes: &[u8]) -> Result<()>;
}

pub struct ClientState {
    callbacks: HashMap<usize, Sender<Vec<u8>>>,
    counter: usize,
}

pub struct Client {
    conn: Mutex<OwnedWriteHalf>,
    state: Arc<Mutex<ClientState>>,
}

#[async_trait]
impl HandleCallback for Arc<Mutex<ClientState>> {
    async fn handle(&self, _session: u64, index: usize, bytes: &[u8]) -> Result<()> {
        let msg = Vec::from(bytes);
        let mut state = self.lock().await;
        if let Some(cb) = state.callbacks.remove(&index) {
            cb.send(msg).or_else(|_| Err(anyhow!("Failed to send through oneshot")))?;
        }
        Ok(())
    }
}

impl Client {
    pub async fn new(addr: &str) -> Result<Client> {
        let conn = TcpStream::connect(addr).await?;
        let (read, write) = conn.into_split();

        let state = Arc::new(Mutex::new(ClientState{
            callbacks: HashMap::default(),
            counter: 0,
        }));
        tokio::spawn(manage_connection(read, state.clone()));
        Ok(Self {
            conn: Mutex::new(write),
            state,
        })
    }

    pub async fn call<T: DeserializeOwned + Clone, R: Serialize>(&self, session: u64, request: R) -> Result<T> {
        let msg = serde_json::to_vec(&request)?;
        let (send, recv) = oneshot::channel();
        {
            let mut state = self.state.lock().await;
            // Register the callback
            let index = state.counter;
            state.callbacks.insert(index, send);
            state.counter += 1;
            
            // Send the request
            let mut conn = self.conn.lock().await;
            conn.write_u64(session).await?;
            conn.write_u64(index.try_into()?).await?;
            conn.write_u64(msg.len().try_into()?).await?;
            conn.write_all(&msg).await?;
            info!("Send message {}", String::from_utf8_lossy(&msg));
        }
        let bytes = recv.await?;
        let msg = serde_json::from_slice::<T>(&bytes)?;
        Ok(msg)
    }
}

async fn manage_connection<H: HandleCallback>(client: OwnedReadHalf, handle: H) {
    if let Err(e) = manage_connection_inner(client, handle).await {
        warn!("Error managing GroupClient: {}", e);
    }
}

async fn manage_connection_inner<H: HandleCallback>(mut client: OwnedReadHalf, handle: H) -> Result<()> {
    let mut buff = vec![0; 8192];
    loop {
        let session = client.read_u64().await?;
        let index = client.read_u64().await? as usize;
        let msg = client.read_u64().await? as usize;
        if 0 == client.read_exact(&mut buff[0..msg]).await? {
            return Ok(());
        }
        handle.handle(session, index, &buff[0..msg]).await?;
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Group {
    pub name: String,
    pub last: usize,
    pub first: usize,
    pub posting_allowed: bool,
}

impl Group {
    pub fn new(name: &str, last: usize, first: usize, posting_allowed: bool) -> Self {
        Self {
            name: name.to_string(),
            last,
            first,
            posting_allowed,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub enum GroupRequest {
    List,
    NewGroups,
    NewNews,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum GroupResponse {
    List(Vec<Group>),
    NewGroups,
    NewNews,
}

#[derive(Serialize, Deserialize)]
pub enum FileRequest {
    Article(String, u64),
    Body,
    Head,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum FileResponse {
    Article(String),
    Body,
    Head,
}

#[derive(Serialize, Deserialize)]
pub enum StateRequest {
    Group(String),
    Last,
    Next,
    Stat,

    // Gets the context for the provided session
    Context,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum StateResponse {
    Nil,
    // Gets the context for the provided session
    Context(Context),
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Context {
    pub group: String,
    pub article_pointer: usize,
}

impl Default for Context {
    fn default() -> Self {
        Self {
            group: String::new(),
            article_pointer: 0,
        }
    }
}

pub struct Listener {
    pub conn: TcpListener,
}

impl Listener {
    pub fn add_args(clap: Command) -> Command {
        clap.arg(arg!(-c --listen <LISTEN> "Address:port to listen on"))
    }

    pub async fn from_clap(clap: &ArgMatches ) -> Result<Self> {
        let addr = clap.get_one::<String>("listen")
            .ok_or(anyhow!("--listen not set"))?;
        let conn = TcpListener::bind(addr).await?;
        Ok(Self {
            conn,
        })
    }
}