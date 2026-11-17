use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use tokio::sync::{mpsc, watch};

/// A progress event emitted by the pipeline.
#[derive(Debug, Clone)]
pub enum Progress {
    /// Formulation stage started.
    FormulationStarted,
    /// Formulation stage finished, produced `count` formulations.
    /// `ideas_per_formulation` is the expected number of ideas per formulation.
    FormulationComplete {
        count: usize,
        ideas_per_formulation: usize,
    },
    /// Starting to process formulation `index` (0-based) of `total`.
    FormulationProcessingStarted { index: usize, total: usize },
    /// Element selection stage finished for a formulation.
    ElementSelectionComplete { index: usize, count: usize },
    /// Ideation stage finished for a formulation, produced `count` ideas.
    IdeationComplete { index: usize, count: usize },
    /// A single idea finished refinement within formulation `index`.
    IdeaRefined { index: usize },
    /// All refinements finished for formulation `index`.
    RefinementComplete {
        index: usize,
        accepted: usize,
        total: usize,
    },
    /// All formulations processed.
    PipelineComplete,
}

/// Pipeline-side progress sender.
///
/// Wraps an [`mpsc::Sender<Progress>`] for detailed events and a [`watch::Sender<f32>`] for the
/// latest completion fraction (`0.0..=1.0`). State is tracked internally via atomics — no mutex
/// needed.
///
/// ## Weighting model
///
/// Total is set once at [`Progress::FormulationComplete`]:
///
/// ```text
/// total = 1 + N * (2 + ideas_per_formulation)
/// ```
///
/// - Formulation = 1 unit
/// - Per formulation: element selection = 1, ideation = 1, refinement per idea =
///   `ideas_per_formulation`
///
/// Only `completed` ever increases after that, guaranteeing monotonic progress.
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
    pub fn new() -> (Self, mpsc::Receiver<Progress>, watch::Receiver<f32>) {
        let (events_tx, events_rx) = mpsc::channel(64);
        let (fraction_tx, fraction_rx) = watch::channel(0.0_f32);
        let handle = Self {
            events_tx,
            fraction_tx,
            completed: AtomicU32::new(0),
            total: AtomicU32::new(0),
        };
        (handle, events_rx, fraction_rx)
    }

    /// Send a progress event. Updates both the event channel and the fraction.
    pub fn send(&self, event: Progress) {
        let fraction = self.advance(&event);
        let _ = self.events_tx.try_send(event);
        let _ = self.fraction_tx.send(fraction);
    }

    fn advance(&self, event: &Progress) -> f32 {
        match event {
            Progress::FormulationComplete {
                count,
                ideas_per_formulation,
            } => {
                let total = 1 + (*count as u32) * (2 + *ideas_per_formulation as u32);
                self.total.store(total, Ordering::Relaxed);
                self.completed.store(1, Ordering::Relaxed);
            }
            Progress::ElementSelectionComplete { .. }
            | Progress::IdeationComplete { .. }
            | Progress::IdeaRefined { .. } => {
                self.completed.fetch_add(1, Ordering::Relaxed);
            }
            Progress::PipelineComplete => return 1.0,
            _ => {}
        }
        let total = self.total.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            (self.completed.load(Ordering::Relaxed) as f32 / total as f32).min(1.0)
        }
    }
}

pub(crate) type SharedProgress = Arc<ProgressHandle>;

pub(crate) fn send(progress: &Option<SharedProgress>, event: Progress) {
    if let Some(p) = progress {
        p.send(event);
    }
}
