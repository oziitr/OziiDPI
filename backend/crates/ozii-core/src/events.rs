use crate::models::{BackendStatus, RoutingMetrics};
use serde::{Deserialize, Serialize};
use std::sync::mpsc::{Receiver, Sender, channel};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BackendEvent {
    StateChanged {
        old_state: BackendStatus,
        new_state: BackendStatus,
    },
    EngineStarted {
        pid: u32,
    },
    EngineFailed {
        reason: String,
    },
    RoutingMetricsUpdated {
        metrics: RoutingMetrics,
    },
    DiagnosticWarning {
        message: String,
    },
    RecoveryStarted,
    RecoveryFinished {
        success: bool,
    },
}

use std::sync::Mutex;

pub struct EventBroadcaster {
    sender: Sender<BackendEvent>,
    receiver: Mutex<Receiver<BackendEvent>>,
}

impl EventBroadcaster {
    pub fn new() -> Self {
        let (sender, receiver) = channel();
        Self {
            sender,
            receiver: Mutex::new(receiver),
        }
    }

    pub fn get_sender(&self) -> Sender<BackendEvent> {
        self.sender.clone()
    }

    /// In a real Tauri app, the frontend would pull from the receiver or
    /// the sender would directly emit to `AppHandle::emit_all`.
    pub fn try_recv(&self) -> Result<BackendEvent, std::sync::mpsc::TryRecvError> {
        self.receiver.lock().unwrap().try_recv()
    }
}

impl Default for EventBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}
