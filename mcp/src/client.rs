// SameBoy subprocess manager — spawns sameboy-json and communicates via JSON-RPC

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, Mutex};

use crate::protocol::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};

type ResponseSender = mpsc::UnboundedSender<Result<JsonRpcResponse>>;
type PendingMap = HashMap<u64, ResponseSender>;

pub struct SameBoyClient {
    request_id: Arc<AtomicU64>,
    cmd_tx: mpsc::UnboundedSender<JsonRpcRequest>,
    notif_rx: mpsc::UnboundedReceiver<JsonRpcNotification>,
    pending: Arc<Mutex<PendingMap>>,
    _child: Arc<Mutex<Option<Child>>>,
}

impl SameBoyClient {
    pub async fn spawn(bin_path: &str, rom_path: Option<&str>) -> Result<Self> {
        let mut cmd = Command::new(bin_path);
        cmd.kill_on_drop(true)
            .stdout(std::process::Stdio::piped())
            .stdin(std::process::Stdio::piped());
        
        if let Some(rom) = rom_path {
            cmd.arg(rom);
        }
        
        let mut child = cmd.spawn()
            .context("Failed to spawn sameboy-json")?;
        
        let stdin = child.stdin.take().expect("stdin should be available");
        let stdout = child.stdout.take().expect("stdout should be available");
        
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<JsonRpcRequest>();
        let (notif_tx, notif_rx) = mpsc::unbounded_channel::<JsonRpcNotification>();
        
        let request_id = Arc::new(AtomicU64::new(0));
        let pending: Arc<Mutex<PendingMap>> = Arc::new(Mutex::new(HashMap::new()));
        
        // Writer task
        tokio::spawn({
            async move {
                let mut stdin = BufWriter::new(stdin);
                let mut rx = cmd_rx;
                while let Some(request) = rx.recv().await {
                    let line = serde_json::to_string(&request).unwrap();
                    if stdin.write_all(format!("{}\n", line).as_bytes()).await.is_ok() {
                        stdin.flush().await.ok();
                    }
                }
            }
        });
        
        // Reader task
        let reader_pending = Arc::clone(&pending);
        tokio::spawn({
            let notif_tx = notif_tx;
            async move {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let line = line.trim().to_string();
                    if line.is_empty() {
                        continue;
                    }
                    
                    if let Ok(response) = serde_json::from_str::<JsonRpcResponse>(&line) {
                        let mut pending = reader_pending.lock().await;
                        if let Some(tx) = pending.remove(&response.id) {
                            let result = if let Some(err) = &response.error {
                                Err(anyhow::anyhow!("{}", err))
                            } else {
                                Ok(response)
                            };
                            let _ = tx.send(result);
                        }
                        continue;
                    }
                    
                    if let Ok(notification) = serde_json::from_str::<JsonRpcNotification>(&line) {
                        let _ = notif_tx.send(notification);
                        continue;
                    }
                }
            }
        });
        
        Ok(Self {
            request_id,
            cmd_tx,
            notif_rx,
            pending,
            _child: Arc::new(Mutex::new(Some(child))),
        })
    }
    
    /// Send a command and wait for the response
    pub async fn call(&self, method: &str, params: Option<serde_json::Value>) -> Result<JsonRpcResponse> {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst) + 1;
        let (tx, mut rx) = mpsc::unbounded_channel();
        
        {
            let mut pending = self.pending.lock().await;
            pending.insert(id, tx);
        }
        
        let request = JsonRpcRequest { id, method: method.into(), params };
        self.cmd_tx.send(request)
            .context("Failed to send command (process may have exited)")?;
        
        rx.recv().await
            .context("No response received")?
    }
    
    /// Receive next notification (blocks until available)
    pub async fn recv_notification(&mut self) -> Option<JsonRpcNotification> {
        self.notif_rx.recv().await
    }
}

impl Drop for SameBoyClient {
    fn drop(&mut self) {
        if let Ok(mut guard) = self._child.try_lock() {
            if let Some(child) = &mut *guard {
                let _ = child.kill();
            }
        }
    }
}
