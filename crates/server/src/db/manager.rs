use crate::{Error, Result};
use dashmap::DashMap;
use rusqlite::{Connection, OpenFlags};
use shared_proto::ServerEvent;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tokio::sync::{Semaphore, broadcast, mpsc, oneshot};
type Job = Box<dyn FnOnce(&mut Connection) + Send>;
#[derive(Clone)]
pub struct Database {
    tx: mpsc::Sender<Job>,
    readers: Arc<Mutex<Vec<Connection>>>,
    permits: Arc<Semaphore>,
    pub events: broadcast::Sender<ServerEvent>,
}
pub fn configure(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000; PRAGMA cache_size=-64000;")
}
impl Database {
    pub fn open(path: &Path, schema: &str) -> anyhow::Result<Self> {
        let mut c = Connection::open(path)?;
        configure(&c)?;
        c.execute_batch(schema)?;
        let (tx, mut rx) = mpsc::channel::<Job>(256);
        std::thread::Builder::new()
            .name("sqlite-writer".into())
            .spawn(move || {
                while let Some(job) = rx.blocking_recv() {
                    job(&mut c)
                }
            })?;
        let mut readers = Vec::new();
        for _ in 0..4 {
            let c = Connection::open_with_flags(
                path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )?;
            c.execute_batch(
                "PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000; PRAGMA cache_size=-64000;",
            )?;
            readers.push(c);
        }
        let (events, _) = broadcast::channel(1024);
        Ok(Self {
            tx,
            readers: Arc::new(Mutex::new(readers)),
            permits: Arc::new(Semaphore::new(4)),
            events,
        })
    }
    pub async fn write<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Box::new(move |c| {
                let _ = tx.send(f(c));
            }))
            .await
            .map_err(|_| Error::Internal(anyhow::anyhow!("database writer stopped")))?;
        rx.await.map_err(|e| Error::Internal(e.into()))?
    }
    pub async fn read<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let permit = self
            .permits
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| Error::Internal(e.into()))?;
        let readers = self.readers.clone();
        tokio::task::spawn_blocking(move || {
            let c = readers
                .lock()
                .map_err(|_| Error::Internal(anyhow::anyhow!("read pool poisoned")))?
                .pop()
                .ok_or_else(|| Error::Internal(anyhow::anyhow!("read pool empty")))?;
            let result = f(&c);
            readers
                .lock()
                .map_err(|_| Error::Internal(anyhow::anyhow!("read pool poisoned")))?
                .push(c);
            drop(permit);
            result
        })
        .await
        .map_err(|e| Error::Internal(e.into()))?
    }
}
pub struct Manager {
    pub global: Database,
    root: PathBuf,
    workspaces: DashMap<String, Database>,
    opening: tokio::sync::Mutex<()>,
}
impl Manager {
    pub fn open(root: &Path) -> anyhow::Result<Self> {
        std::fs::create_dir_all(root.join("workspaces"))?;
        Ok(Self {
            global: Database::open(
                &root.join("global.db"),
                include_str!("../../migrations/global.sql"),
            )?,
            root: root.to_owned(),
            workspaces: DashMap::new(),
            opening: tokio::sync::Mutex::new(()),
        })
    }
    pub async fn workspace(&self, id: &str) -> Result<Database> {
        uuid::Uuid::parse_str(id).map_err(|_| Error::NotFound)?;
        if let Some(db) = self.workspaces.get(id) {
            return Ok(db.clone());
        }
        let _guard = self.opening.lock().await;
        if let Some(db) = self.workspaces.get(id) {
            return Ok(db.clone());
        }
        let path = self.root.join("workspaces").join(format!("{id}.db"));
        let key = id.to_owned();
        let db = tokio::task::spawn_blocking(move || {
            Database::open(&path, include_str!("../../migrations/workspace.sql"))
        })
        .await
        .map_err(|e| Error::Internal(e.into()))??;
        Ok(self.workspaces.entry(key).or_insert(db).clone())
    }
    pub async fn authorize(&self, w: String, u: String) -> Result<String> {
        self.global
            .read(move |c| {
                c.query_row(
                    "SELECT role FROM workspace_members WHERE workspace_id=? AND user_id=?",
                    [w, u],
                    |r| r.get(0),
                )
                .map_err(|e| {
                    if matches!(e, rusqlite::Error::QueryReturnedNoRows) {
                        Error::Forbidden
                    } else {
                        e.into()
                    }
                })
            })
            .await
    }
}
