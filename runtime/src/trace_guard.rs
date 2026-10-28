//! Finalize a pending trace if its request future is dropped or aborted.
use crate::store::Store;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::{Mutex, Notify};

static PENDING_WRITES: AtomicUsize = AtomicUsize::new(0);
static WRITES_FINISHED: Notify = Notify::const_new();

/// Wait for dropped request futures to finish persisting their cancellation traces.
/// Call only after dropping the request future, before shutting down its runtime.
pub async fn wait_for_cancelled_traces() {
    loop {
        let finished = WRITES_FINISHED.notified();
        tokio::pin!(finished);
        finished.as_mut().enable();
        if PENDING_WRITES.load(Ordering::Acquire) == 0 {
            return;
        }
        finished.await;
    }
}

pub(crate) struct TraceGuard {
    pending: Option<(Arc<Mutex<Store>>, String)>,
}
impl TraceGuard {
    pub fn new(store: Arc<Mutex<Store>>, id: &str) -> Self {
        Self {
            pending: Some((store, id.to_owned())),
        }
    }
    pub fn disarm(&mut self) {
        self.pending = None;
    }
}
impl Drop for TraceGuard {
    fn drop(&mut self) {
        let Some((store, id)) = self.pending.take() else {
            return;
        };
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            PENDING_WRITES.fetch_add(1, Ordering::AcqRel);
            runtime.spawn(async move {
                let diagnostic = serde_json::json!({"code":"request_cancelled","trace_id":id,
                    "message":"The request ended before a terminal provider result was recorded.",
                    "hint":"The client may have disconnected. Upstream usage may still have occurred; inspect recorded attempts before retrying.","retryable":false});
                if store.lock().await.fail_pending(&id, &diagnostic).is_err() {
                    eprintln!("warning: could not persist cancellation for trace {id}");
                }
                PENDING_WRITES.fetch_sub(1, Ordering::AcqRel);
                WRITES_FINISHED.notify_waiters();
            });
        }
    }
}
