//! Application-wide lifecycle cancellation for the Community build.
//!
//! This token owns normal application shutdown only; it is unrelated to
//! feature authorization.

use tokio_util::sync::CancellationToken;

#[derive(Clone, Default)]
pub(crate) struct ApplicationLifecycle {
    shutdown: CancellationToken,
}

impl ApplicationLifecycle {
    pub(crate) fn shutdown(&self) {
        self.shutdown.cancel();
    }

    pub(crate) fn is_shutting_down(&self) -> bool {
        self.shutdown.is_cancelled()
    }

    pub(crate) fn token(&self) -> CancellationToken {
        self.shutdown.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::ApplicationLifecycle;

    #[tokio::test]
    async fn shutdown_cancellation_is_shared_by_clones() {
        let lifecycle = ApplicationLifecycle::default();
        let observer = lifecycle.clone();
        assert!(!observer.is_shutting_down());

        lifecycle.shutdown();

        assert!(observer.is_shutting_down());
        assert!(observer.token().is_cancelled());
    }
}
