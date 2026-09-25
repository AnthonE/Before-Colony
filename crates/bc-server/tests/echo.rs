//! Transport smoke test: a real server in echo mode, a native WebTransport client pinned to the
//! self-signed certificate by hash (as a browser would), datagram and stream round trips.

use std::time::{Duration, Instant};

use bc_server::{Config, Mode};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn echo_round_trips() -> anyhow::Result<()> {
    let cfg = Config { mode: Mode::Echo, wt_port: 0, http_addr: "127.0.0.1:0".parse()?, ..Config::default() };
    let server = bc_server::start(cfg).await?;

    // Discover exactly like the web loader does.
    let info = bc_bot::discover(&format!("http://{}", server.http_addr)).await?;
    assert_eq!(info.cert_hash, Some(server.cert_hash));
    let conn = bc_bot::connect(&info.url, info.cert_hash).await?;

    // Datagrams: 20 pings, all under 50 ms on loopback.
    let mut worst = Duration::ZERO;
    for i in 0u32..20 {
        let payload = i.to_le_bytes();
        let sent = Instant::now();
        conn.send_datagram(payload)?;
        let echoed = tokio::time::timeout(Duration::from_secs(2), conn.receive_datagram()).await??;
        assert_eq!(&*echoed, &payload);
        worst = worst.max(sent.elapsed());
    }
    assert!(worst < Duration::from_millis(50), "worst datagram RTT {worst:?}");

    // Reliable control stream.
    let (mut tx, mut rx) = conn.open_bi().await?.await?;
    tx.write_all(b"hello sector").await?;
    let mut buf = [0u8; 12];
    tokio::time::timeout(Duration::from_secs(2), rx.read_exact(&mut buf)).await??;
    assert_eq!(&buf, b"hello sector");

    let status = server.status();
    assert!(status["net"]["datagrams_in"].as_u64().unwrap() >= 20);
    server.shutdown();
    Ok(())
}
