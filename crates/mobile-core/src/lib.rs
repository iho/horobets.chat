//! Mobile bindings retain the same cache, outbox and networking worker as GPUI.
use client_core::{ClientEngine, Command, Snapshot};
use std::sync::{Arc, Mutex};

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum MobileError {
    #[error("{detail}")]
    Client { detail: String },
}
fn error(e: impl std::fmt::Display) -> MobileError {
    MobileError::Client {
        detail: e.to_string(),
    }
}

#[uniffi::export(callback_interface)]
pub trait SnapshotListener: Send + Sync {
    /// Called on a Rust worker thread. UI adapters must dispatch to the main actor.
    fn on_snapshot(&self, snapshot_json: String);
}

#[derive(uniffi::Object)]
pub struct MobileClient {
    engine: Mutex<Option<ClientEngine>>,
    updates: async_channel::Receiver<Snapshot>,
    subscribed: Mutex<bool>,
}

#[uniffi::export]
impl MobileClient {
    #[uniffi::constructor]
    pub fn new(data_dir: String) -> Result<Arc<Self>, MobileError> {
        let engine = ClientEngine::start(data_dir.into()).map_err(error)?;
        let updates = engine.updates.clone();
        Ok(Arc::new(Self {
            engine: Mutex::new(Some(engine)),
            updates,
            subscribed: Mutex::new(false),
        }))
    }

    /// A tagged JSON command matching client_core::Command; no independent mobile sync stack.
    pub fn send_command(&self, command_json: String) -> Result<(), MobileError> {
        let command: Command = serde_json::from_str(&command_json).map_err(error)?;
        let guard = self.engine.lock().map_err(error)?;
        guard
            .as_ref()
            .ok_or_else(|| error("Client is closed"))?
            .send(command)
            .map_err(error)
    }

    /// Only one consumer may receive the coalesced snapshot stream.
    pub fn subscribe(&self, listener: Box<dyn SnapshotListener>) -> Result<(), MobileError> {
        let mut subscribed = self.subscribed.lock().map_err(error)?;
        if *subscribed {
            return Err(error("A listener is already attached"));
        }
        if self.updates.is_closed() {
            return Err(error("Client is closed"));
        }
        let updates = self.updates.clone();
        std::thread::Builder::new()
            .name("messenger-mobile-events".into())
            .spawn(move || {
                while let Ok(snapshot) = updates.recv_blocking() {
                    // All snapshot fields are JSON-compatible, including UTF-8 file paths.
                    if let Ok(json) = serde_json::to_string(&snapshot) {
                        listener.on_snapshot(json);
                    }
                }
            })
            .map_err(error)?;
        *subscribed = true;
        Ok(())
    }

    /// Closes the callback stream and drops the command sender so both workers exit.
    pub fn shutdown(&self) {
        self.updates.close();
        if let Ok(mut engine) = self.engine.lock() {
            engine.take();
        }
    }
}
impl Drop for MobileClient {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Listener(std::sync::mpsc::Sender<String>);
    impl SnapshotListener for Listener {
        fn on_snapshot(&self, value: String) {
            let _ = self.0.send(value);
        }
    }
    #[test]
    fn callbacks_commands_and_shutdown_cross_the_mobile_boundary() {
        let dir = tempfile::tempdir().unwrap();
        let client = MobileClient::new(dir.path().to_str().unwrap().into()).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        client.subscribe(Box::new(Listener(tx.clone()))).unwrap();
        let initial: serde_json::Value =
            serde_json::from_str(&rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap())
                .unwrap();
        assert!(initial["user_id"].is_null());
        assert!(client.subscribe(Box::new(Listener(tx))).is_err());
        assert!(client.send_command("invalid JSON".into()).is_err());
        client
            .send_command(r#"{"type":"clear_error"}"#.into())
            .unwrap();
        assert!(rx.recv_timeout(std::time::Duration::from_secs(5)).is_ok());
        client.shutdown();
        assert!(client.send_command(r#"{"type":"refresh"}"#.into()).is_err());
    }
}
