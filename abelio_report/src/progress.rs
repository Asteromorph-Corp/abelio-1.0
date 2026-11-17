use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use tokio::sync::{mpsc, watch};

/// A progress event emitted by the pipeline.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone)]
pub enum Progress {
    /// Draft report generated.
    DraftComplete,
    /// Research plan generated.
    PlanComplete,
    /// Summary generated.
    SummaryComplete,
}

/// Pipeline-side progress sender.
///
/// Wraps an [`mpsc::Sender<Progress>`] for detailed events and a [`watch::Sender<f32>`] for the
/// latest completion fraction (`0.0..=1.0`). State is tracked internally via atomics — no mutex
/// needed.
///
/// ## Weighting model
///
/// ```text
/// total = 3
/// ```
///
/// draft = 1, plan = 1, summarize = 1.
///
/// Only `completed` ever increases, guaranteeing monotonic progress.
pub struct ProgressHandle {
    events_tx: mpsc::Sender<Progress>,
    fraction_tx: watch::Sender<f32>,
    completed: AtomicU32,
    total: AtomicU32,
}

impl ProgressHandle {
    /// Create a new handle and its two receivers.
    ///
    /// - [`mpsc::Receiver<Progress>`] — consume detailed events.
    /// - [`watch::Receiver<f32>`] — poll `.borrow()` for latest fraction.
    pub fn new(total: u32) -> (Self, mpsc::Receiver<Progress>, watch::Receiver<f32>) {
        let (events_tx, events_rx) = mpsc::channel(64);
        let (fraction_tx, fraction_rx) = watch::channel(0.0_f32);
        let handle = Self {
            events_tx,
            fraction_tx,
            completed: AtomicU32::new(0),
            total: AtomicU32::new(total),
        };
        (handle, events_rx, fraction_rx)
    }

    /// Send a progress event. Updates both the event channel and the fraction.
    pub fn send(&self, event: Progress) {
        let fraction = self.advance();
        let _ = self.events_tx.try_send(event);
        let _ = self.fraction_tx.send(fraction);
    }

    fn advance(&self) -> f32 {
        let completed = self.completed.fetch_add(1, Ordering::Relaxed) + 1;
        let total = self.total.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            (completed as f32 / total as f32).min(1.0)
        }
    }
}

pub(crate) type SharedProgress = Arc<ProgressHandle>;

pub(crate) fn send(progress: &Option<SharedProgress>, event: Progress) {
    if let Some(p) = progress {
        p.send(event);
    }
}
