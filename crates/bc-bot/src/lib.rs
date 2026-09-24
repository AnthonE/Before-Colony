//! Before Colony Bot SDK.
//!
//! Agents play through the same WebTransport protocol and client state machine (`bc-client-core`)
//! as the browser, so they obey the same rules, input rate and sensor-limited view as humans. They
//! identify as agents in the handshake and are shown in-game as Mobile Doll (MD) pilots.

pub mod transport;

pub use transport::{EndpointInfo, connect, discover, install_crypto_provider};
