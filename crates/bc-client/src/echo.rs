//! `?mode=echo`: transport smoke test. Pings the echo server and measures datagram RTT in an async
//! task, so the number reflects the network rather than the render loop's frame time.

use std::cell::RefCell;
use std::rc::Rc;

use bevy::prelude::*;

use crate::dev_hooks::DevStatus;
use crate::net::{NetState, now_ms};

#[derive(Default)]
struct EchoStats {
    seq: u32,
    last_ping: f64,
    pings: u32,
    pongs: u32,
    rtt_ms: f64,
    stream_ok: bool,
    stream_sent: bool,
    listening: bool,
}

#[derive(Default)]
struct Echo(Rc<RefCell<EchoStats>>);

pub struct EchoPlugin;

impl Plugin for EchoPlugin {
    fn build(&self, app: &mut App) {
        app.insert_non_send(Echo::default()).add_systems(Update, ping);
    }
}

fn ping(net: NonSend<NetState>, echo: NonSend<Echo>, mut dev: ResMut<DevStatus>) {
    let Some(t) = net.get() else { return };
    let mut s = echo.0.borrow_mut();
    if !s.listening {
        s.listening = true;
        let stats = echo.0.clone();
        let rx = t.clone();
        wasm_bindgen_futures::spawn_local(async move {
            while let Some(d) = rx.recv_datagram().await {
                if d.len() == 12 {
                    let sent = f64::from_le_bytes(d[4..12].try_into().unwrap_or([0; 8]));
                    let rtt = now_ms() - sent;
                    let mut s = stats.borrow_mut();
                    s.rtt_ms = if s.pongs == 0 { rtt } else { s.rtt_ms * 0.8 + rtt * 0.2 };
                    s.pongs += 1;
                }
            }
        });
    }
    if !s.stream_sent {
        t.send_control(b"hello sector".to_vec());
        s.stream_sent = true;
    }
    if t.take_control().as_slice() == b"hello sector" {
        s.stream_ok = true;
    }
    let now = now_ms();
    if now - s.last_ping > 100.0 {
        s.last_ping = now;
        s.seq += 1;
        let mut pkt = [0u8; 12];
        pkt[..4].copy_from_slice(&s.seq.to_le_bytes());
        pkt[4..].copy_from_slice(&now.to_le_bytes());
        if t.send_datagram(&pkt) {
            s.pings += 1;
        }
    }
    dev.set("pings", s.pings);
    dev.set("pongs", s.pongs);
    dev.set("rtt_ms", s.rtt_ms);
    dev.set("stream_ok", s.stream_ok);
}
