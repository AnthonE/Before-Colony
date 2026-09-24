//! Browser WebTransport (via `web-transport-wasm`).
//!
//! Datagrams are polled every frame with a no-op waker. The crate keeps the underlying promise
//! subscribed between polls, so nothing is lost and no task per datagram is needed. The rare
//! control-stream traffic runs in two small `spawn_local` tasks.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use futures::StreamExt;
use futures::channel::mpsc;
use web_transport_wasm::{ClientBuilder, CongestionControl, Session};

#[derive(Clone)]
pub struct Transport {
    session: Session,
    ctrl_tx: mpsc::UnboundedSender<Vec<u8>>,
    ctrl_in: Rc<RefCell<Vec<u8>>>,
    closed: Rc<Cell<bool>>,
}

impl Transport {
    /// Connects to `url`, pinning the server's self-signed certificate by SHA-256 when given.
    pub async fn connect(url: &str, cert_hash: Option<Vec<u8>>) -> Result<Transport, String> {
        let url = url::Url::parse(url).map_err(|e| format!("bad url {url}: {e}"))?;
        let builder =
            ClientBuilder::new().with_unreliable(true).with_congestion_control(CongestionControl::LowLatency);
        let client = match cert_hash {
            Some(hash) => builder.with_server_certificate_hashes(vec![hash]),
            None => builder.with_system_roots(),
        };
        let session = client.connect(url).await.map_err(|e| format!("connect: {e}"))?;
        let (mut send, mut recv) =
            session.open_bi().await.map_err(|e| format!("open control stream: {e}"))?;

        let closed = Rc::new(Cell::new(false));
        let (ctrl_tx, mut ctrl_rx) = mpsc::unbounded::<Vec<u8>>();
        let writer_closed = closed.clone();
        wasm_bindgen_futures::spawn_local(async move {
            while let Some(bytes) = ctrl_rx.next().await {
                if send.write(&bytes).await.is_err() {
                    writer_closed.set(true);
                    break;
                }
            }
        });
        let ctrl_in = Rc::new(RefCell::new(Vec::new()));
        let inbox = ctrl_in.clone();
        let reader_closed = closed.clone();
        wasm_bindgen_futures::spawn_local(async move {
            loop {
                match recv.read(4096).await {
                    Ok(Some(chunk)) => inbox.borrow_mut().extend_from_slice(&chunk),
                    _ => {
                        reader_closed.set(true);
                        break;
                    }
                }
            }
        });
        Ok(Transport { session, ctrl_tx, ctrl_in, closed })
    }

    /// Hands a datagram to the browser; `false` if it had no room (the datagram is dropped).
    pub fn send_datagram(&self, bytes: &[u8]) -> bool {
        match self.session.try_send_datagram(bytes) {
            Ok(sent) => sent,
            Err(_) => {
                self.closed.set(true);
                false
            }
        }
    }

    /// Calls `f` for every datagram that has arrived since the last call.
    pub fn recv_datagrams(&self, mut f: impl FnMut(&[u8])) {
        let mut cx = Context::from_waker(Waker::noop());
        loop {
            match self.session.poll_recv_datagram(&mut cx) {
                Poll::Ready(Ok(bytes)) => f(&bytes),
                Poll::Ready(Err(_)) => {
                    self.closed.set(true);
                    break;
                }
                Poll::Pending => break,
            }
        }
    }

    /// Awaits the next datagram (for async consumers; the game loop uses [`recv_datagrams`]).
    pub async fn recv_datagram(&self) -> Option<Vec<u8>> {
        self.session.recv_datagram().await.ok().map(|b| b.to_vec())
    }

    pub fn send_control(&self, bytes: Vec<u8>) {
        let _ = self.ctrl_tx.unbounded_send(bytes);
    }

    /// Takes all control-stream bytes received so far.
    pub fn take_control(&self) -> Vec<u8> {
        std::mem::take(&mut *self.ctrl_in.borrow_mut())
    }

    pub fn is_closed(&self) -> bool {
        self.closed.get()
    }

    pub fn max_datagram_size(&self) -> usize {
        self.session.max_datagram_size()
    }
}
