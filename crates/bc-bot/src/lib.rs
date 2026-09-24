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

pub mod brains;
pub mod client;
pub mod transport;

pub use brains::DollBrain;
pub use client::{BotClient, BotConfig};
pub use transport::{EndpointInfo, connect, discover, install_crypto_provider};

/// Parses `leo | wingzero | taurus | virgo`.
pub fn parse_frame(s: &str) -> Option<bc_proto::FrameId> {
    use bc_proto::FrameId;
    match s.to_ascii_lowercase().replace(['-', '_', ' '], "").as_str() {
        "leo" => Some(FrameId::Leo),
        "wingzero" | "wing" | "zero" => Some(FrameId::WingZero),
        "taurus" => Some(FrameId::Taurus),
        "virgo" => Some(FrameId::Virgo),
        _ => None,
    }
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
