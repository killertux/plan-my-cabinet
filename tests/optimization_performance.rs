#[path = "support/performance_fixture.rs"]
mod performance_fixture;

use plan_my_cabinet::candidate_generation::SearchBudget;
use plan_my_cabinet::candidate_ranking::Objective;
use plan_my_cabinet::commands::ProjectEditor;
use plan_my_cabinet::optimization_worker::{OptimizationWorker, WorkerError, WorkerMessage};
use std::time::{Duration, Instant};

const BUDGET: SearchBudget = SearchBudget {
    placements: 10_000,
    witness_states: 20_000,
    beam_width: 8,
};

fn wait(worker: &mut OptimizationWorker) -> (Duration, Result<(), WorkerError>) {
    let start = Instant::now();
    loop {
        match worker.try_receive().unwrap() {
            Some(WorkerMessage::Completed(result)) => return (start.elapsed(), result.map(|_| ())),
            _ => std::thread::sleep(Duration::from_millis(1)),
        }
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "worker did not terminate"
        );
    }
}

#[test]
fn full_fixture_budget_and_cancellation_observations() {
    let editor = ProjectEditor::new(performance_fixture::fixture()).unwrap();
    assert_eq!(
        (editor.project().boards.len(), editor.project().stock.len()),
        (100, 10)
    );
    let mut worker = OptimizationWorker::start(
        &editor,
        Objective::FewestCuts,
        BUDGET,
        Duration::from_secs(5),
    );
    let (search, result) = wait(&mut worker);
    eprintln!(
        "100-board/10-stock worker completion: {search:?} (5s search deadline); result: {result:?}"
    );

    let mut worker = OptimizationWorker::start(
        &editor,
        Objective::FewestCuts,
        BUDGET,
        Duration::from_secs(5),
    );
    // Cancel while the full search is active; never reduce the fixture or the budget.
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(10) {
        std::thread::yield_now();
    }
    let cancelled = Instant::now();
    worker.cancel();
    let (latency, result) = wait(&mut worker);
    eprintln!(
        "100-board/10-stock cancellation: {latency:?} (250ms target; target met: {})",
        latency < Duration::from_millis(250)
    );
    assert!(
        latency < Duration::from_secs(2),
        "cancellation unresponsive"
    );
    assert!(matches!(result, Err(WorkerError::Cancelled)));
    assert!(cancelled.elapsed() >= latency);
    assert!(editor.project().allocations.is_empty());
}
