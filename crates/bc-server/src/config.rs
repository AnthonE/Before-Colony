use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

/// What the server runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Mode {
    /// The game: one sector simulation plus drone NPCs.
    Game,
    /// Transport smoke test: datagrams and stream bytes are echoed back.
    Echo,
}

/// How pilots get their suits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Ruleset {
    /// The game: pilots start in their hangar bay, fly the suit they built (a worn Argo, to begin
    /// with), dock to bring home what they found, and lose it for good when it's destroyed. Their
    /// credits, stores and suit are kept (signed in); the Colony Exchange is shared.
    Survival,
    /// Any frame from the title screen, free respawns, and the dock buys the hold: for trying the
    /// Lodestars out, the load tests and the combat end-to-end tests.
    Arcade,
}

/// How pilots fly (`bc_sim::tuning::FlightRules`; drones fly by the real rules either way).
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Flight {
    /// For fun over realism: the tank is a boost gauge (only boost burns it, and it fills back up
    /// once boost is let go), and pilots bear twice the G.
    Anime,
    /// The simulator: every newton burns propellant, and a pilot bears 6 g for good.
    Real,
}

impl Flight {
    pub fn rules(self) -> bc_sim::tuning::FlightRules {
        match self {
            Flight::Anime => bc_sim::tuning::FlightRules::Anime,
            Flight::Real => bc_sim::tuning::FlightRules::Real,
        }
    }
}

/// Which tactical oracle feeds the Orrery. The in-sim local oracle always runs; `jev` adds
/// TypeSafe Jev as a blended prior when `TYPESAFE_API_KEY` is set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum OracleKind {
    Local,
    Jev,
}

/// The Elo Pros store: it lists the game, serves its page and polls its `/status.json`.
pub const STORE_ORIGIN: &str = "https://elopros.com";

/// A command line's list: repeated flags and comma-separated values alike, trimmed, empties
/// dropped (`--siwe-domain game.elopros.com,elopros.com`).
pub fn split_list(values: &[String]) -> Vec<String> {
    values
        .iter()
        .flat_map(|v| v.split(','))
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .collect()
}

/// The host (and port, unless it's the scheme's default) of an `Origin`, as SIWE names a domain:
/// `https://elopros.com` is `elopros.com`, `http://127.0.0.1:8080` stays `127.0.0.1:8080`.
pub fn origin_host(origin: &str) -> &str {
    let rest = origin.split_once("://").map_or(origin, |(_, r)| r);
    let rest = rest.split('/').next().unwrap_or(rest);
    rest.strip_suffix(":443").filter(|_| origin.starts_with("https://")).unwrap_or(rest)
}

/// Which of `domains` a session's challenge names: the one its page came from (`origin`), or the
/// first (agents, and pages from anywhere else, whose wallets will warn).
pub fn pick_domain<'a>(domains: &'a [String], origin: Option<&str>) -> &'a str {
    let host = origin.map(origin_host);
    host.and_then(|h| domains.iter().find(|d| d.eq_ignore_ascii_case(h)))
        .or_else(|| domains.first())
        .map_or("", String::as_str)
}

#[derive(Clone, Debug)]
pub struct Config {
    pub mode: Mode,
    /// UDP port for WebTransport (0 = pick a free port).
    pub wt_port: u16,
    /// Dev HTTP address (port 0 = pick a free port).
    pub http_addr: SocketAddr,
    /// Built web client to serve at `/` (optional).
    pub web_dir: Option<PathBuf>,
    /// Drone NPCs to keep in the sector.
    pub drones: u32,
    /// One of the Almagest's aces is fielded among the drones at most this often, while none is out
    /// (`bc_sim::content::aces`); zero: none.
    pub ace_every: Duration,
    pub oracle: OracleKind,
    /// Player/agent slots in the sector.
    pub max_clients: usize,
    /// Seed for the deterministic simulation.
    pub seed: u64,
    /// TypeSafe API key (from `TYPESAFE_API_KEY`); never sent to clients.
    pub jev_key: Option<String>,
    /// Override for the Jev endpoint (`TYPESAFE_BASE_URL`), e.g. a proxy or a test mock.
    pub jev_url: Option<String>,
    /// The names wallets may sign in to: the hosts (and ports) pages are served from. A session's
    /// challenge names the one its page's origin matches (the WebTransport request's `Origin`), or
    /// the first. Empty: the HTTP address. A deployment behind a domain must set it, or wallets
    /// warn about a mismatch; one whose page is also served from another site (the store at
    /// elopros.com) lists both.
    pub siwe_domains: Vec<String>,
    /// A CA-issued certificate chain and its key (PEM files) for WebTransport. None: the server
    /// self-signs for 14 days and browsers pin the certificate's hash (`/cert-hash`).
    pub tls: Option<(PathBuf, PathBuf)>,
    /// Origins whose pages may read `/status`, `/status.json` and `/cert-hash` from another site
    /// (CORS): the store that lists the game, and serves its page.
    pub cors_origins: Vec<String>,
    /// Admit signed-in pilots only (agents excepted).
    pub require_auth: bool,
    /// How long a wallet may take to sign (people read the message first).
    pub sign_wait: Duration,
    /// How long a resume token lasts after its session ends.
    pub resume_ttl: Duration,
    /// Suits left asleep by signed-in pilots, at most; past it the longest asleep are cleared.
    pub max_sleepers: usize,
    /// A session with no input for this long is ended (its suit sleeps, or goes, as on leaving).
    pub idle_timeout: Duration,
    pub rules: Ruleset,
    pub flight: Flight,
    /// The fabricator and foundry work this many times faster than their recipes say.
    pub craft_speed: f64,
    /// Where pilot records and the exchange are kept (none: in memory, for this run only).
    pub data_dir: Option<PathBuf>,
    /// The colony is open (survival): pilots may ride the cap lifts down into its city. The
    /// server's command line opens it by default (`--no-colony` closes it); this default stays
    /// closed, so a test that doesn't ask for the colony gets no second sector.
    pub colony: bool,
    /// How many connections it takes, in all and from one address (`net::admit`).
    pub limits: crate::net::admit::Limits,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: Mode::Game,
            wt_port: 4433,
            http_addr: SocketAddr::from(([127, 0, 0, 1], 8080)),
            web_dir: None,
            drones: 24,
            ace_every: Duration::from_secs(300),
            oracle: OracleKind::Local,
            max_clients: 64,
            seed: 0xBC_0195,
            jev_key: None,
            jev_url: None,
            siwe_domains: Vec::new(),
            tls: None,
            cors_origins: vec![STORE_ORIGIN.to_string()],
            require_auth: false,
            sign_wait: Duration::from_secs(60),
            resume_ttl: Duration::from_secs(15 * 60),
            max_sleepers: 256,
            idle_timeout: Duration::from_secs(60),
            rules: Ruleset::Survival,
            flight: Flight::Anime,
            craft_speed: 1.0,
            data_dir: None,
            colony: false,
            limits: crate::net::admit::Limits::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn lists_take_commas_and_repeats() {
        assert_eq!(
            split_list(&list(&["game.elopros.com,elopros.com"])),
            list(&["game.elopros.com", "elopros.com"])
        );
        assert_eq!(split_list(&list(&["a", " b , ,c"])), list(&["a", "b", "c"]));
        assert!(split_list(&[]).is_empty());
    }

    #[test]
    fn an_origin_names_its_host() {
        assert_eq!(origin_host("https://elopros.com"), "elopros.com");
        assert_eq!(origin_host("https://elopros.com:443"), "elopros.com");
        assert_eq!(origin_host("https://game.elopros.com:8443"), "game.elopros.com:8443");
        assert_eq!(origin_host("http://127.0.0.1:8080"), "127.0.0.1:8080");
        assert_eq!(origin_host("http://localhost:443"), "localhost:443");
    }

    #[test]
    fn the_challenge_names_the_pages_own_domain() {
        let domains = list(&["game.elopros.com", "elopros.com"]);
        assert_eq!(pick_domain(&domains, Some("https://elopros.com")), "elopros.com");
        assert_eq!(pick_domain(&domains, Some("https://game.elopros.com")), "game.elopros.com");
        assert_eq!(pick_domain(&domains, Some("https://ELOPROS.com")), "elopros.com");
        // Agents send no Origin; a page from elsewhere gets the first (and its wallet warns).
        assert_eq!(pick_domain(&domains, None), "game.elopros.com");
        assert_eq!(pick_domain(&domains, Some("https://evil.example")), "game.elopros.com");
        assert_eq!(pick_domain(&[], Some("https://elopros.com")), "");
    }
}
