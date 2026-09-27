//! Before Colony Bot SDK.
//!
//! Agents play through the same WebTransport protocol and client state machine (`bc-client-core`)
//! as the browser, so they get the same rules, input rate and sensor-limited view as humans. They
//! identify as [`PilotKind::Agent`](bc_proto::PilotKind::Agent) in the handshake and appear in-game
//! as Mobile Doll (MD) pilots.
//!
//! ```no_run
//! # async fn demo() -> anyhow::Result<()> {
//! use bc_bot::{BotClient, BotConfig, DollBrain};
//! use bc_proto::{Faction, FrameId};
//!
//! let mut bot = BotClient::connect(&BotConfig {
//!     server: "http://127.0.0.1:8080".into(),
//!     name: "Agent-01".into(),
//!     frame: FrameId::Leo,
//!     faction: Faction::Colonies,
//! })
//! .await?;
//! let mut brain = DollBrain::new(7);
//! bot.run_for(std::time::Duration::from_secs(60), &mut |ctx| brain.decide(ctx)).await?;
//! # Ok(()) }
//! ```

pub mod client;
pub mod transport;

/// The ready-made brains: the Mobile Doll AI, and a miner. (They live in `bc-client-core`, so the
/// browser autopilot can use them too.)
pub use bc_client_core::brains::{self, DollBrain, MinerBrain};
pub use client::{BotClient, BotConfig};
pub use transport::{EndpointInfo, connect, discover, install_crypto_provider};

/// Parses a frame an agent may fly, by its slug (`leo`, `wingzero`, `heavyarms`…; see
/// [`FrameId::slug`](bc_proto::FrameId::slug)). Mobile Dolls' frames are the server's alone.
pub fn parse_frame(s: &str) -> Option<bc_proto::FrameId> {
    bc_proto::FrameId::from_slug(s).filter(|f| bc_sim::content::playable(*f))
}

/// Parses `colonies | oz | alliance`.
pub fn parse_faction(s: &str) -> Option<bc_proto::Faction> {
    use bc_proto::Faction;
    match s.to_ascii_lowercase().as_str() {
        "colonies" | "colony" => Some(Faction::Colonies),
        "oz" => Some(Faction::Oz),
        "alliance" => Some(Faction::Alliance),
        _ => None,
    }
}
