use std::fmt::Debug;
use std::collections::HashMap;

use futures::{StreamExt, TryFutureExt, FutureExt};
use futures::task::{Spawn, SpawnExt};
use futures::future::RemoteHandle;
use futures::channel::mpsc;

use derive_more::From;

use serde::{Serialize, de::DeserializeOwned};

use common::conn::BoxFuture;
use common::mutable_state::MutableState;

use async_std::io::prelude::WriteExt;
use async_std::fs;
use async_std::path::{PathBuf, Path};

use lockfile::{try_lock_file, LockFileHandle};

use app::file::{NodeAddressFile, IdentityFile};
use app::common::{NetAddress, PublicKey, PrivateKey, derive_public_key};

use node::NodeState;

use database::file_db::FileDb;
use database::{DatabaseClient, AtomicDb, database_loop};

use crypto::identity::SoftwareEd25519Identity;
use identity::{create_identity, IdentityClient};

use crate::messages::{NodeName, NodeInfo, NodeInfoLocal, NodeInfoRemote, NodesInfo};

use crate::store::store::{LoadedNode, Store, LoadedNodeLocal, LoadedNodeRemote};
use crate::store::consts::{LOCKFILE, LOCAL, REMOTE, NODE_DB, NODE_IDENT, NODE_INFO, APP_IDENT, COMPACT_DB};
use crate::compact_node::CompactState;

#[derive(Debug)]
struct LiveNodeLocal {
    node_identity_handle: RemoteHandle<()>,
    compact_db_handle: RemoteHandle<()>,
    node_db_handle: RemoteHandle<()>,
}

#[derive(Debug)]
struct LiveNodeRemote {
    app_identity_handle: RemoteHandle<()>,
    compact_db_handle: RemoteHandle<()>,
}

#[derive(Debug)]
enum LiveNode {
    Local(LiveNodeLocal),
    Remote(LiveNodeRemote),
}

pub struct FileStore<S,FS> {
    spawner: S,
    file_spawner: FS,
    store_path_buf: PathBuf,
    /// If dropped, the advisory lock file protecting the file store will be freed.
    lock_file_handle: LockFileHandle,
    live_nodes: HashMap<NodeName, LiveNode>,
}


#[derive(Debug, From)]
pub enum FileStoreError {
    DuplicateNodeName(NodeName),
    SpawnError,
    LockError,
    SerdeError(serde_json::Error),
    RemoveNodeError,
    DerivePublicKeyError,
    FileDbError,
    AsyncStdIoError(async_std::io::Error),
    NodeIsLoaded,
    NodeNotLoaded,
    NodeDoesNotExist,
    LoadIdentityError,
    LoadDbError,
}


#[derive(Debug, Clone)]
struct FileStoreNodeLocal {
    node_private_key: PrivateKey,
    node_db: PathBuf,
    compact_db: PathBuf,
}

#[derive(Debug, Clone)]
struct FileStoreNodeRemote {
    app_private_key: PrivateKey,
    node_public_key: PublicKey,
    node_address: NetAddress,
    compact_db: PathBuf,
}

#[derive(Debug, Clone)]
enum FileStoreNode {
    Local(FileStoreNodeLocal),
    Remote(FileStoreNodeRemote),
}

type FileStoreNodes = HashMap<NodeName, FileStoreNode>;

/*
 * File store structure:
 *
 * - lockfile
 * - local [dir]
 *      - node_name1
 *          - node.ident
 *          - node.db
 *          - compact.db
 * - remote [dir]
 *      - node_name2
 *          - app.ident
 *          - node.info (public_key + address)
 *          - compact.db
*/


pub async fn open_file_store<FS,S>(store_path_buf: PathBuf, spawner: S, file_spawner: FS) -> Result<FileStore<S,FS>, FileStoreError> 
where   
    FS: Spawn,
    S: Spawn + Sync,
{
    // Create directory if missing:
    if !store_path_buf.is_dir().await {
        fs::create_dir_all(&store_path_buf).await?;
    }

    let store_path_buf_std: std::path::PathBuf = store_path_buf.clone().into();
    let lockfile_path_buf = store_path_buf_std.join(LOCKFILE);
    let lock_file_handle = file_spawner.spawn_with_handle(async move {
        try_lock_file(&lockfile_path_buf)
    }).map_err(|_| FileStoreError::SpawnError)?
    .await
    .map_err(|_| FileStoreError::LockError)?;

    // Verify file store's integrity:
    verify_store(&store_path_buf).await?;

    Ok(FileStore {
        spawner,
        file_spawner,
        store_path_buf,
        lock_file_handle,
        live_nodes: HashMap::new(),
    })
}


async fn read_local_node(node_path: &Path) -> Result<FileStoreNodeLocal, FileStoreError> {
    let node_ident_path = node_path.join(NODE_IDENT);
    let ident_data = fs::read_to_string(&node_ident_path).await?;
    let identity_file: IdentityFile = serde_json::from_str(&ident_data)?;

    Ok(FileStoreNodeLocal {
        node_private_key: identity_file.private_key,
        node_db: node_path.join(NODE_DB),
        compact_db: node_path.join(COMPACT_DB),
    })
}

async fn read_remote_node(node_path: &Path) -> Result<FileStoreNodeRemote, FileStoreError> {
    let app_ident_path = node_path.join(APP_IDENT);
    let ident_data = fs::read_to_string(&app_ident_path).await?;
    let identity_file: IdentityFile = serde_json::from_str(&ident_data)?;

    let node_info_path = node_path.join(NODE_INFO);
    let node_info_data = fs::read_to_string(&node_info_path).await?;
    let node_address_file: NodeAddressFile = serde_json::from_str(&node_info_data)?;

    Ok(FileStoreNodeRemote {
        app_private_key: identity_file.private_key,
        node_public_key: node_address_file.public_key,
        node_address: node_address_file.address,
        compact_db: node_path.join(COMPACT_DB),
    })
}

async fn read_all_nodes(store_path: &Path) -> Result<FileStoreNodes, FileStoreError> {
    let mut file_store_nodes = HashMap::new();

    // Read local nodes:
    let local_dir = store_path.join(LOCAL);
    if let Ok(mut dir) = fs::read_dir(local_dir).await {
        while let Some(res) = dir.next().await {
            let local_node_entry = res?;
            let node_name = NodeName::new(local_node_entry.file_name().to_string_lossy().to_string());
            read_local_node(&local_node_entry.path()).await?;
            let local_node = FileStoreNode::Local(read_local_node(&local_node_entry.path()).await?);
            if file_store_nodes.insert(node_name.clone(), local_node).is_some() {
                return Err(FileStoreError::DuplicateNodeName(node_name));
            }
        }
    }

    // Read remote nodes:
    let remote_dir = store_path.join(REMOTE);
    if let Ok(mut dir) = fs::read_dir(remote_dir).await {
        while let Some(res) = dir.next().await {
            let remote_node_entry = res?;
            let node_name = NodeName::new(remote_node_entry.file_name().to_string_lossy().to_string());
            let remote_node = FileStoreNode::Remote(read_remote_node(&remote_node_entry.path()).await?);
            if file_store_nodes.insert(node_name.clone(), remote_node).is_some() {
                return Err(FileStoreError::DuplicateNodeName(node_name));
            }
        }
    }

    Ok(file_store_nodes)
}

async fn remove_node(store_path: &Path, node_name: &NodeName) -> Result<(), FileStoreError> {
    // Attempt to remove from local dir:
    let node_path = store_path.join(LOCAL).join(node_name.as_str());
    let is_local_removed = fs::remove_file(&node_path).await.is_ok();

    // Attempt to remove from remote dir:
    let remote_dir = store_path.join(REMOTE).join(node_name.as_str());
    let is_remote_removed = fs::remove_file(&node_path).await.is_ok();

    if !(is_local_removed || is_remote_removed) {
        // No removal worked:
        return Err(FileStoreError::RemoveNodeError);
    }

    return Ok(())
}


/// Verify store's integrity
pub async fn verify_store(store_path: &Path) -> Result<(), FileStoreError> {
    // We read all nodes, and make sure it works correctly. We discard the result.
    // We might have a different implementation for this function in the future.
    let _ = read_all_nodes(store_path).await?;
    Ok(())
}


async fn create_local_node(
    node_name: NodeName,
    node_private_key: PrivateKey,
    store_path: &Path) -> Result<(), FileStoreError> {

    let node_path = store_path.join(LOCAL).join(&node_name.as_str());

    // Create node's dir. Should fail if the directory already exists: 
    fs::create_dir_all(&node_path).await?;

    // Create node database file:
    let node_db_path = node_path.join(NODE_DB);
    let node_public_key = derive_public_key(&node_private_key).map_err(|_| FileStoreError::DerivePublicKeyError)?;
    let initial_state = NodeState::<NetAddress>::new(node_public_key);
    let _ = FileDb::create(node_db_path.into(), initial_state).map_err(|_| FileStoreError::FileDbError)?;

    // Create compact database file:
    let compact_db_path = node_path.join(COMPACT_DB);
    let initial_state = CompactState::new();
    let _ = FileDb::create(compact_db_path.into(), initial_state).map_err(|_| FileStoreError::FileDbError)?;

    // Create node.ident file:
    let identity_file = IdentityFile {
        private_key: node_private_key,
    };
    let identity_file_string = serde_json::to_string(&identity_file)?;

    let node_ident_path = node_path.join(NODE_IDENT);
    let mut file = fs::File::create(node_ident_path).await?;
    file.write_all(identity_file_string.as_bytes()).await?;

    Ok(())
}

async fn create_remote_node(
    node_name: NodeName, 
    app_private_key: PrivateKey, 
    node_public_key: PublicKey, 
    node_address: NetAddress, 
    store_path: &Path) -> Result<(), FileStoreError> {
    
    let node_path = store_path.join(REMOTE).join(&node_name.as_str());

    // Create node's dir. Should fail if the directory already exists: 
    fs::create_dir_all(&node_path).await?;

    // Create app.ident file:
    let identity_file = IdentityFile {
        private_key: app_private_key,
    };
    let identity_file_string = serde_json::to_string(&identity_file)?;

    let node_ident_path = node_path.join(APP_IDENT);
    let mut file = fs::File::create(node_ident_path).await?;
    file.write_all(identity_file_string.as_bytes()).await?;

    // Create compact database file:
    let compact_db_path = node_path.join(COMPACT_DB);
    let initial_state = CompactState::new();
    let _ = FileDb::create(compact_db_path.into(), initial_state).map_err(|_| FileStoreError::FileDbError)?;

    // Create node.info:
    let node_address_file = NodeAddressFile {
        public_key: node_public_key,
        address: node_address,
    };
    let node_address_string = serde_json::to_string(&node_address_file)?;

    let node_info_path = node_path.join(NODE_INFO);
    let mut file = fs::File::create(node_info_path).await?;
    file.write_all(node_address_string.as_bytes()).await?;

    Ok(())
}

fn file_store_node_to_node_info(file_store_node: FileStoreNode) -> Result<NodeInfo, FileStoreError> {
    Ok(match file_store_node {
        FileStoreNode::Local(local) => {
            NodeInfo::Local(NodeInfoLocal {
                node_public_key: derive_public_key(&local.node_private_key)
                    .map_err(|_| FileStoreError::DerivePublicKeyError)?,
            })
        },
        FileStoreNode::Remote(remote) => {
            NodeInfo::Remote(NodeInfoRemote {
                app_public_key: derive_public_key(&remote.app_private_key)
                    .map_err(|_| FileStoreError::DerivePublicKeyError)?,
                node_public_key: remote.node_public_key,
                node_address: remote.node_address,
            })
        },
    })
}

fn create_identity_server<S>(private_key: &PrivateKey, spawner: &S) 
    -> Result<(RemoteHandle<()>, IdentityClient), FileStoreError> 
where
    S: Spawn,
{
    let identity = SoftwareEd25519Identity::from_private_key(&private_key)
        .map_err(|_| FileStoreError::LoadIdentityError)?;

    // Spawn identity service:
    let (sender, identity_loop) = create_identity(identity);
    let server_handle = spawner
        .spawn_with_handle(identity_loop)
        .map_err(|_| FileStoreError::SpawnError)?;
    let identity_client = IdentityClient::new(sender);

    Ok((server_handle, identity_client))
}

async fn spawn_db<S,FS,MS>(db_path_buf: PathBuf, spawner: &S, file_spawner: FS) 
    -> Result<(MS, RemoteHandle<()>, DatabaseClient<MS::Mutation>), FileStoreError>
where
    S: Spawn,
    FS: Spawn + Send + Clone + 'static,
    MS: Clone + Serialize + DeserializeOwned + MutableState + Send + Debug + 'static,
    MS::Mutation: Clone + Serialize + DeserializeOwned + Send + Debug,
    MS::MutateError: Debug + Send,
{
    // This operation blocks, so we are running it using the file_spawner:
    let atomic_db = file_spawner.spawn_with_handle(async move {
        FileDb::<MS>::load(db_path_buf.into()).map_err(|_| FileStoreError::LoadDbError)
    }).map_err(|_| FileStoreError::SpawnError)?.await?;

    // Get initial state:
    let state = atomic_db.get_state().clone();

    // Spawn database service:
    let (db_request_sender, incoming_db_requests) = mpsc::channel(0);
    let loop_fut = database_loop(
        atomic_db,
        incoming_db_requests,
        file_spawner.clone(),
    )
    .map_err(|e| error!("spawn_db(): database_loop() error: {:?}", e))
    .map(|_| ());

    let remote_handle = spawner
        .spawn_with_handle(loop_fut)
        .map_err(|_| FileStoreError::SpawnError)?;

    Ok((state, remote_handle, DatabaseClient::new(db_request_sender)))
}


async fn load_local_node<S,FS>(local: &FileStoreNodeLocal, spawner: &S, file_spawner: FS) 
    -> Result<(LiveNodeLocal, LoadedNodeLocal), FileStoreError> 
where
    S: Spawn,
    FS: Spawn + Send + Clone + 'static,
{
    // Create identity server:
    let (node_identity_handle, node_identity_client) = create_identity_server(&local.node_private_key, spawner)?;

    // Spawn compact database:
    let (compact_state, compact_db_handle, compact_db_client) = spawn_db(local.compact_db.clone(), spawner, file_spawner.clone()).await?;

    // Spawn node database:
    let (node_state, node_db_handle, node_db_client) = spawn_db(local.compact_db.clone(), spawner, file_spawner.clone()).await?;

    // When we drop those handles, all servers will be closed:
    let live_node_local = LiveNodeLocal {
        node_identity_handle,
        compact_db_handle,
        node_db_handle,
    };

    let loaded_node_local = LoadedNodeLocal {
        node_identity_client,
        compact_state,
        compact_db_client,
        node_state,
        node_db_client,
    };

    Ok((live_node_local, loaded_node_local))
}

async fn load_remote_node<S,FS>(remote: &FileStoreNodeRemote, spawner: &S, file_spawner: FS) 
    -> Result<(LiveNodeRemote, LoadedNodeRemote), FileStoreError> 
where
    S: Spawn,
    FS: Spawn + Send + Clone + 'static,
{

    // Create identity server:
    let (app_identity_handle, app_identity_client) = create_identity_server(&remote.app_private_key, spawner)?;
    
    // Spawn compact database:
    let (compact_state, compact_db_handle, compact_db_client) = spawn_db(remote.compact_db.clone(), spawner, file_spawner.clone()).await?;

    // When we drop those handles, all servers will be closed:
    let live_node_remote = LiveNodeRemote {
        app_identity_handle,
        compact_db_handle,
    };

    let loaded_node_remote = LoadedNodeRemote {
        app_identity_client,
        node_public_key: remote.node_public_key.clone(),
        node_address: remote.node_address.clone(),
        compact_state,
        compact_db_client,
    };

    Ok((live_node_remote, loaded_node_remote))
}

impl<S,FS> Store for FileStore<S,FS>
where
    S: Spawn + Send + Sync,
    FS: Spawn + Clone + Send + Sync + 'static,
{
    type Error = FileStoreError;

    fn create_local_node<'a>(
        &'a mut self,
        node_name: NodeName,
        node_private_key: PrivateKey,
    ) -> BoxFuture<'a, Result<(), Self::Error>> {
        Box::pin(create_local_node(node_name, node_private_key, &self.store_path_buf))
    }

    fn create_remote_node<'a>(
        &'a mut self,
        node_name: NodeName,
        app_private_key: PrivateKey,
        node_public_key: PublicKey,
        node_address: NetAddress,
    ) -> BoxFuture<'a, Result<(), Self::Error>> {
        Box::pin(async move {
            if self.live_nodes.contains_key(&node_name) {
                return Err(FileStoreError::NodeIsLoaded);
            }
            create_remote_node(node_name, app_private_key, node_public_key, node_address, &self.store_path_buf).await
        })
    }

    fn list_nodes<'a>(&'a self) -> BoxFuture<'a, Result<NodesInfo, Self::Error>> {
        Box::pin(async move {
            let mut inner_nodes_info = HashMap::new();
            for (node_name, file_store_node) in read_all_nodes(&self.store_path_buf).await? {
                let node_info = file_store_node_to_node_info(file_store_node)?;
                let _ = inner_nodes_info.insert(node_name, node_info);
            }
            Ok(NodesInfo(inner_nodes_info))
        })
    }

    fn load_node<'a>(
        &'a mut self,
        node_name: NodeName,
    ) -> BoxFuture<'a, Result<LoadedNode, Self::Error>> {
        Box::pin(async move {
            // Make sure the node we want to load is not already loaded:
            if self.live_nodes.contains_key(&node_name) {
                return Err(FileStoreError::NodeIsLoaded);
            }

            let file_store_nodes = read_all_nodes(&self.store_path_buf).await?;
            let file_store_node = if let Some(file_store_node) = file_store_nodes.get(&node_name) {
                file_store_node
            } else {
                return Err(FileStoreError::NodeDoesNotExist);
            };
            
            let (live_node, loaded_node) = match file_store_node {
                FileStoreNode::Local(local) => {
                    let (live_node_local, loaded_node_local) = load_local_node(local, &self.spawner, self.file_spawner.clone()).await?;
                    (LiveNode::Local(live_node_local), LoadedNode::Local(loaded_node_local))
                },
                FileStoreNode::Remote(remote) => {
                    let (live_node_remote, loaded_node_remote) = load_remote_node(remote, &self.spawner, self.file_spawner.clone()).await?;
                    (LiveNode::Remote(live_node_remote), LoadedNode::Remote(loaded_node_remote))
                },
            };

            // Keep the loaded node:
            self.live_nodes.insert(node_name, live_node);

            Ok(loaded_node)
        })
    }

    /// Unload a node
    fn unload_node<'a>(&'a mut self, node_name: &'a NodeName) -> BoxFuture<'a, Result<(), Self::Error>> {
        Box::pin(async move {
            if self.live_nodes.remove(node_name).is_none() {
                return Err(FileStoreError::NodeNotLoaded);
            }
            Ok(())
        })
    }

    /// Remove a node from the store
    /// A node must be in unloaded state to be removed.
    fn remove_node<'a>(&'a mut self, node_name: NodeName) -> BoxFuture<'a, Result<(), Self::Error>> {
        Box::pin(async move {
            // Do not remove node if it is currently loaded:
            if self.live_nodes.contains_key(&node_name) {
                return Err(FileStoreError::NodeIsLoaded);
            }
            remove_node(&self.store_path_buf, &node_name).await
        })
    }
}