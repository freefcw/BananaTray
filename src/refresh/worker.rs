use std::sync::Mutex;

use smol::channel::TrySendError;

use super::{RefreshCoordinator, RefreshRequest};
use crate::utils::BoundedThreadOwner;

/// RefreshCoordinator 的唯一生产 owner：发送请求，并在退出时有界回收线程。
pub(crate) struct RefreshWorker {
    tx: smol::channel::Sender<RefreshRequest>,
    owner: Mutex<Option<BoundedThreadOwner>>,
}

impl RefreshWorker {
    pub(crate) fn spawn(coordinator: RefreshCoordinator) -> std::io::Result<Self> {
        let tx = coordinator.sender();
        let owner = BoundedThreadOwner::spawn("refresh-coordinator", move || {
            smol::block_on(coordinator.run());
        })?;
        Ok(Self {
            tx,
            owner: Mutex::new(Some(owner)),
        })
    }

    #[cfg(test)]
    pub(crate) fn detached(tx: smol::channel::Sender<RefreshRequest>) -> Self {
        Self {
            tx,
            owner: Mutex::new(None),
        }
    }

    pub(crate) fn try_send(
        &self,
        request: RefreshRequest,
    ) -> Result<(), TrySendError<RefreshRequest>> {
        self.tx.try_send(request)
    }

    pub(crate) fn request_shutdown(&self) {
        let _ = self.tx.try_send(RefreshRequest::Shutdown);
    }

    pub(crate) fn join_before(&self, deadline: std::time::Instant) -> bool {
        let Ok(mut owner) = self.owner.lock() else {
            return false;
        };
        owner
            .as_mut()
            .map(|owner| owner.shutdown_before(deadline))
            .unwrap_or(true)
    }

    #[cfg(test)]
    pub(crate) fn shutdown_before(&self, deadline: std::time::Instant) -> bool {
        self.request_shutdown();
        self.join_before(deadline)
    }
}
