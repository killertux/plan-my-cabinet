//! Explicit, headless optimization: workers only own snapshots and send proposals.
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, TryRecvError},
};
use std::thread;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::candidate_generation::{
    GenerationError, SearchBudget, generate_cancellable, validate_complete,
};
use crate::candidate_ranking::{Objective, RankedCandidate, Ranking, RankingError, rank};
use crate::commands::{EditError, ProjectEditor};
use crate::cut_tree::WitnessError;
use crate::domain::{Allocation, Project};

/// An optimization input token excludes scene poses, grid, display settings and hardware.
/// Names and prices are included because they affect comparative previews and costs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceToken {
    pub project_id: Uuid,
    pub revision: u64,
    pub manufacturing: [u8; 32],
}

fn source(project: &Project) -> SourceToken {
    let mut hash = Sha256::new();
    hash.update(b"optimization inputs v1");
    hash.update(
        serde_json::to_vec(&(
            &project.materials,
            &project
                .boards
                .iter()
                .map(|b| {
                    (
                        &b.id,
                        &b.name,
                        &b.material_id,
                        &b.length,
                        &b.width,
                        &b.thickness,
                        &b.grain_override,
                    )
                })
                .collect::<Vec<_>>(),
            &project.stock,
            &project.allocations,
            project.cutting_kerf,
            project.cut_fee,
            project.currency,
        ))
        .expect("validated project fields serialize"),
    );
    SourceToken {
        project_id: project.id,
        revision: project.revision,
        manufacturing: hash.finalize().into(),
    }
}

#[derive(Debug)]
pub enum WorkerMessage {
    Progress { placements: usize },
    Completed(Result<Box<CompletedSearch>, WorkerError>),
}

#[derive(Debug)]
pub struct CompletedSearch {
    pub source: SourceToken,
    pub ranking: Ranking,
    pub exhausted: bool,
    /// The editor's allocations at search start, for comparative preview.
    pub original_allocations: Vec<Allocation>,
    /// Verified incumbent, if the starting layout was complete and feasible.
    pub original_plan: Option<RankedCandidate>,
    cancelled: Arc<AtomicBool>,
}

impl CompletedSearch {
    pub fn is_current(&self, project: &Project) -> bool {
        project.id == self.source.project_id
            && source(project).manufacturing == self.source.manufacturing
    }

    pub fn placement_changes(&self, index: usize) -> Option<Vec<Uuid>> {
        let candidate = &self.ranking.candidates.get(index)?.candidate;
        Some(
            candidate
                .allocations
                .iter()
                .filter(|allocation| {
                    self.original_allocations
                        .iter()
                        .find(|a| a.board_id == allocation.board_id)
                        .is_none_or(|old| {
                            old.stock_id != allocation.stock_id
                                || old.origin != allocation.origin
                                || old.quarter_turn != allocation.quarter_turn
                        })
                })
                .map(|a| a.board_id)
                .collect(),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkerError {
    Cancelled,
    Generation(Box<GenerationError>),
    Ranking(Box<RankingError>),
    Disconnected,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ApplyError {
    Cancelled,
    DifferentProject,
    Stale { started: u64, current: u64 },
    UnknownCandidate,
    InvalidCandidate(WitnessError),
    Edit(EditError<()>),
}

pub struct OptimizationWorker {
    cancelled: Arc<AtomicBool>,
    receiver: Receiver<WorkerMessage>,
    finished: bool,
}

impl OptimizationWorker {
    /// Spawns a search over a private document copy; the editor remains available for edits.
    pub fn start(
        editor: &ProjectEditor,
        objective: Objective,
        budget: SearchBudget,
        duration: Duration,
    ) -> Self {
        let snapshot = editor.project().clone();
        let token = source(&snapshot);
        let originals = snapshot.allocations.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let deadline = Instant::now() + duration;
            let _ = sender.send(WorkerMessage::Progress { placements: 0 });
            let expired = || Instant::now() >= deadline;
            let stop = || worker_cancelled.load(Ordering::Relaxed) || expired();
            let generation = generate_cancellable(&snapshot, budget, &stop, &|placements| {
                // Keep completion from sitting behind thousands of progress
                // notifications in the UI's bounded per-frame receive loop.
                if placements % 256 == 0 {
                    let _ = sender.send(WorkerMessage::Progress { placements });
                }
            });
            let result = if worker_cancelled.load(Ordering::Relaxed) {
                Err(WorkerError::Cancelled)
            } else {
                match generation {
                    Ok(generation) => rank(&snapshot, &generation.complete, objective)
                        .map(|ranking| {
                            let original_plan = ranking
                                .candidates
                                .iter()
                                .find(|c| c.candidate.allocations == originals)
                                .cloned();
                            CompletedSearch {
                                source: token,
                                ranking,
                                exhausted: generation.exhausted,
                                original_allocations: originals,
                                original_plan,
                                cancelled: worker_cancelled.clone(),
                            }
                        })
                        .map_err(|error| WorkerError::Ranking(Box::new(error))),
                    Err(GenerationError::Cancelled) if expired() => {
                        // The bounded search has no certified result to return at the deadline.
                        rank(&snapshot, &[], objective)
                            .map(|ranking| CompletedSearch {
                                source: token,
                                ranking,
                                exhausted: true,
                                original_allocations: originals,
                                original_plan: None,
                                cancelled: worker_cancelled.clone(),
                            })
                            .map_err(|error| WorkerError::Ranking(Box::new(error)))
                    }
                    Err(error) => Err(WorkerError::Generation(Box::new(error))),
                }
            };
            let _ = sender.send(WorkerMessage::Completed(result.map(Box::new)));
        });
        Self {
            cancelled,
            receiver,
            finished: false,
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    /// Nonblocking; only one completion is emitted. A cancelled result cannot be accepted.
    pub fn try_receive(&mut self) -> Result<Option<WorkerMessage>, WorkerError> {
        if self.finished {
            return Ok(None);
        }
        match self.receiver.try_recv() {
            Ok(message) => {
                if matches!(message, WorkerMessage::Completed(_)) {
                    self.finished = true;
                }
                Ok(Some(message))
            }
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(WorkerError::Disconnected),
        }
    }

    /// Explicit acceptance; re-check source inputs and every witness against current locks.
    /// A no-op preserves the undo/redo branches.
    pub fn apply(
        editor: &mut ProjectEditor,
        result: &CompletedSearch,
        index: usize,
    ) -> Result<bool, ApplyError> {
        if result.cancelled.load(Ordering::Relaxed) {
            return Err(ApplyError::Cancelled);
        }
        let current = editor.project();
        if current.id != result.source.project_id {
            return Err(ApplyError::DifferentProject);
        }
        if source(current).manufacturing != result.source.manufacturing {
            return Err(ApplyError::Stale {
                started: result.source.revision,
                current: current.revision,
            });
        }
        let candidate = &result
            .ranking
            .candidates
            .get(index)
            .ok_or(ApplyError::UnknownCandidate)?
            .candidate;
        validate_complete(current, candidate).map_err(ApplyError::InvalidCandidate)?;
        let allocations = candidate.allocations.clone();
        editor
            .transact(|project| {
                project.allocations = allocations;
                Ok(())
            })
            .map_err(ApplyError::Edit)
    }
}

impl Drop for OptimizationWorker {
    fn drop(&mut self) {
        if !self.finished {
            self.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Board, BoardGrain, Material, Stock, StockGrain, StockSource};
    use crate::money::Currency;
    use crate::units::{Length, Pose, Quaternion};

    fn mm(n: i64) -> Length {
        Length::from_micrometres(n * 1000)
    }
    fn fixture() -> ProjectEditor {
        let mut p = Project::new("worker", Currency::Brl);
        let material = Uuid::new_v4();
        let board = Uuid::new_v4();
        p.materials.push(Material {
            id: material,
            name: "ply".into(),
            default_thickness: mm(18),
            default_grain: BoardGrain::Unrestricted,
        });
        p.boards.push(Board {
            id: board,
            name: "part".into(),
            material_id: material,
            length: mm(100),
            width: mm(50),
            thickness: mm(18),
            grain_override: None,
            parent_id: None,
            pose: Pose::new([0.; 3], Quaternion::IDENTITY).unwrap(),
        });
        p.stock.push(Stock {
            id: Uuid::new_v4(),
            name: "sheet".into(),
            material_id: material,
            length: mm(100),
            width: mm(50),
            thickness: mm(18),
            grain: StockGrain::Nondirectional,
            source: StockSource::Owned,
            price: None,
            priority: 0,
            trim: [Length::ZERO; 4],
        });
        ProjectEditor::new(p).unwrap()
    }
    fn budget() -> SearchBudget {
        SearchBudget {
            placements: 100,
            witness_states: 100,
            beam_width: 4,
        }
    }
    fn complete(worker: &mut OptimizationWorker) -> Result<CompletedSearch, WorkerError> {
        for _ in 0..5000 {
            if let Some(WorkerMessage::Completed(result)) = worker.try_receive().unwrap() {
                return result.map(|result| *result);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("worker timed out");
    }

    #[test]
    fn cancel_cannot_commit_even_after_completion() {
        let mut editor = fixture();
        let before = editor.project().clone();
        let mut worker = OptimizationWorker::start(
            &editor,
            Objective::FewestCuts,
            budget(),
            Duration::from_secs(5),
        );
        worker.cancel();
        if let Ok(result) = complete(&mut worker) {
            assert_eq!(
                OptimizationWorker::apply(&mut editor, &result, 0),
                Err(ApplyError::Cancelled)
            );
        }
        assert_eq!(editor.project(), &before);
        assert!(!editor.can_undo());
    }

    #[test]
    fn stale_edit_noop_and_atomic_acceptance() {
        let mut editor = fixture();
        let mut worker = OptimizationWorker::start(
            &editor,
            Objective::FewestCuts,
            budget(),
            Duration::from_secs(5),
        );
        let result = complete(&mut worker).unwrap();
        assert!(!result.ranking.candidates.is_empty());
        assert_eq!(
            result.placement_changes(0),
            Some(vec![editor.project().boards[0].id])
        );
        editor
            .transact(|p| -> Result<(), ()> {
                p.boards[0].length = mm(99);
                Ok(())
            })
            .unwrap();
        let before = editor.project().clone();
        assert!(matches!(
            OptimizationWorker::apply(&mut editor, &result, 0),
            Err(ApplyError::Stale { .. })
        ));
        assert_eq!(editor.project(), &before);
        editor.undo().unwrap();
        // Manufacturing content reverted; revision advanced, but the witness is fresh.
        assert_eq!(OptimizationWorker::apply(&mut editor, &result, 0), Ok(true));
        assert_eq!(
            editor.project().allocations,
            result.ranking.candidates[0].candidate.allocations
        );
        let mut incumbent_worker = OptimizationWorker::start(
            &editor,
            Objective::FewestCuts,
            budget(),
            Duration::from_secs(5),
        );
        let incumbent = complete(&mut incumbent_worker).unwrap();
        let index = incumbent
            .ranking
            .candidates
            .iter()
            .position(|c| c.candidate.allocations == editor.project().allocations)
            .unwrap();
        assert_eq!(
            OptimizationWorker::apply(&mut editor, &incumbent, index),
            Ok(false)
        );
        editor.undo().unwrap();
        assert!(editor.project().allocations.is_empty());
        assert!(editor.can_redo());
    }

    #[test]
    fn invalid_candidate_and_unrelated_edit() {
        let mut editor = fixture();
        let mut worker = OptimizationWorker::start(
            &editor,
            Objective::FewestCuts,
            budget(),
            Duration::from_secs(5),
        );
        let mut result = complete(&mut worker).unwrap();
        editor.set_grid_spacing(mm(20)).unwrap();
        assert_eq!(OptimizationWorker::apply(&mut editor, &result, 0), Ok(true));
        editor.undo().unwrap();
        result.ranking.candidates[0].candidate.allocations[0].origin[0] = mm(1);
        let before = editor.project().clone();
        assert!(matches!(
            OptimizationWorker::apply(&mut editor, &result, 0),
            Err(ApplyError::InvalidCandidate(_))
        ));
        assert_eq!(editor.project(), &before);
        assert!(editor.can_redo());
    }
}
