//! The ZERO System's tactical oracle layer, off the hot path.
//!
//! The simulation always runs its own deterministic local oracle inside the tick
//! (`bc_sim::zero::local_oracle`). This crate adds external oracles that answer typed questions
//! with calibrated probabilities; today that is TypeSafe's **Jev**. Their answers are blended into
//! the local ones (`p ∝ p_local^½ · p_jev^½`) while fresh (15 ticks).
//!
//! Flow: the sector pushes a [`TacticalPicture`] per ZERO pilot (≈4 Hz) into a lock-free ring. The
//! [`spawn_worker`] task keeps only the latest picture per pilot, runs ≤ 8 oracle calls at once, and
//! pushes [`TacticalAdvice`] back through another ring. The tick never waits: a failing or slow
//! oracle simply means no advice.

pub mod jev;
pub mod local;
pub mod worker;

pub use bc_sim::zero::{TacticalAdvice, TacticalPicture};
pub use jev::JevOracle;
pub use local::LocalOracle;
pub use worker::{WorkerHandle, spawn_worker};

/// Why an oracle call produced no advice.
#[derive(Debug, thiserror::Error)]
pub enum OracleError {
    #[error("oracle timed out")]
    Timeout,
    #[error("oracle rejected the API key (401)")]
    Unauthorized,
    #[error("oracle rejected the request (422): {0}")]
    BadRequest(String),
    #[error("oracle is rate limiting (429)")]
    RateLimited,
    #[error("oracle is overloaded ({0})")]
    Overloaded(u16),
    #[error("oracle HTTP {0}")]
    Http(u16),
    #[error("transport: {0}")]
    Transport(String),
    #[error("malformed response: {0}")]
    Malformed(String),
    #[error("circuit breaker open")]
    CircuitOpen,
}

/// Something that turns a tactical picture into typed advice.
pub trait TacticalOracle: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn assess(
        &self,
        picture: &TacticalPicture,
    ) -> impl std::future::Future<Output = Result<TacticalAdvice, OracleError>> + Send;
}
