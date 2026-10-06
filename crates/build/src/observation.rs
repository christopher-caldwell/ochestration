//! Read-only, in-memory observations of confirmed Build transitions.
//!
//! Observers receive facts after persistence, never authority to route a gate.
//! They must handle output errors locally; there is deliberately no error return
//! that could turn a presentation failure into a controller stop. Observations
//! are synchronous, so an action-start notification finishes before invocation.

use crate::state::{BuildPlan, BuildState, Gate, Scope};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionOutcome {
    Complete,
    Pass,
    ChangesRequired,
    Blocked,
    Retry,
}

/// The executed action, distinct from the gate/scope in the resulting state.
/// Audit outcomes reflect the controller's validated verdict, not the agent's
/// `complete` envelope (which can also contain failed or unknown coverage).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FinishedAction {
    pub action_id: String,
    pub gate: Gate,
    pub scope: Scope,
    pub outcome: ActionOutcome,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuildEvent {
    /// The persisted state on entry to an existing Build, including Running.
    /// Running alone does not establish that a provider process is alive.
    Snapshot,
    /// The first persisted state of a fresh Build; also its initial snapshot.
    Initialized,
    /// A blocked Build has persisted its existing continuation preparation.
    Continued,
    ActionStarted,
    ActionRouted(FinishedAction),
    /// The invocation is returning a recorded Complete or Stopped state.
    Terminal,
}

#[derive(Clone, Debug)]
pub struct BuildObservation<'a> {
    pub event: BuildEvent,
    /// The resolved effort ID used for this invocation, even when selected implicitly.
    pub effort_id: &'a str,
    pub state: &'a BuildState,
    /// Valid ordered plan facts matching this state's authority and digest.
    /// Unavailable facts must not be replaced with invented progress. Loading
    /// them is best-effort for snapshots, never a new execution prerequisite.
    pub plan: Option<&'a BuildPlan>,
}

pub trait BuildObserver {
    fn observe(&mut self, observation: BuildObservation<'_>);
}

impl<F> BuildObserver for F
where
    F: FnMut(BuildObservation<'_>),
{
    fn observe(&mut self, observation: BuildObservation<'_>) {
        self(observation);
    }
}
