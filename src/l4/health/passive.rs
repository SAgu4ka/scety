use crate::l4::backend::BackendNode;
use std::sync::Arc;

#[derive(Clone)]
pub struct PassiveHealthTracker {
    max_failures: usize,
}

impl PassiveHealthTracker {
    pub fn new(max_failures: usize) -> Self {
        Self { max_failures }
    }

    #[inline]
    pub fn record_success(&self, node: &Arc<BackendNode>) {
        node.mark_success(1);
    }

    #[inline]
    pub fn record_failure(&self, node: &Arc<BackendNode>) {
        node.mark_failure(self.max_failures);
    }
}
