//! Videohub Backend
//!
//! Acts as a client and speaks to a peer that implements the Videohub Ethernet Control Protocol.

use crate::matrix::*;
use anyhow::{anyhow, Result};
use futures_core::stream::BoxStream;
use futures_util::{SinkExt, StreamExt};
use std::{collections::VecDeque, net::SocketAddr, sync::Arc};
use tokio::{
    net::TcpStream,
    select,
    sync::{broadcast, mpsc, oneshot, RwLock},
};
use tokio_stream::wrappers::BroadcastStream;
use tokio_util::codec::Framed;
use tracing::{error, info};
use videohub::{VideohubCodec, VideohubMessage};

/// Which part of the cache changed?
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CacheEvent {
    InputLabels,
    OutputLabels,
    Routes,
    Disconnected,
}

/// In‐memory cache of last‐seen state.
#[derive(Default)]
struct Cache {
    info: RouterInfo,
    matrix_info: RouterMatrixInfo,
    input_labels: Option<Vec<RouterLabel>>,
    output_labels: Option<Vec<RouterLabel>>,
    routes: Option<Vec<RouterPatch>>,
}

/// Commands sent into the single reader loop.
enum Command {
    /// Send msg and capture next ACK/NAK in resp.
    Ack {
        msg: VideohubMessage,
        resp: oneshot::Sender<bool>,
    },
    /// Just send msg.
    Send { msg: VideohubMessage },
}

/// A MatrixRouter speaking Videohub over TCP with caching.
pub struct VideohubRouter {
    /// send commands into the reader loop
    cmd_tx: mpsc::UnboundedSender<Command>,
    /// shared cache
    cache: Arc<RwLock<Cache>>,
    /// broadcast cache updates
    cache_tx: broadcast::Sender<CacheEvent>,
}

fn update_labels(
    opt: &mut Option<Vec<RouterLabel>>,
    changes: Vec<RouterLabel>,
    max_idx: u32,
) -> Result<()> {
    let mut current = opt.replace(vec![]).unwrap_or_default();
    for new in changes {
        if new.id >= max_idx {
            return Err(anyhow!("Label is out of index!"));
        }
        if let Some(idx) = current.iter().position(|l| l.id == new.id) {
            current[idx].name = new.name;
        } else {
            current.push(new);
        }
    }
    opt.replace(current);
    Ok(())
}

fn update_routes(
    opt: &mut Option<Vec<RouterPatch>>,
    changes: Vec<RouterPatch>,
    max_input_idx: u32,
    max_output_idx: u32,
) -> Result<()> {
    let mut current = opt.replace(vec![]).unwrap_or_default();
    for new in changes {
        if new.to_output >= max_output_idx || new.from_input >= max_input_idx {
            return Err(anyhow!("Patch is out of index!"));
        }
        if let Some(idx) = current.iter().position(|p| p.to_output == new.to_output) {
            current[idx].from_input = new.from_input;
        } else {
            current.push(new);
        }
    }
    opt.replace(current);
    Ok(())
}

impl VideohubRouter {
    /// Connect, consume only Preamble + DeviceInfo, spawn the reader loop.
    #[tracing::instrument]
    pub async fn connect(addr: SocketAddr) -> Result<Self> {
        info!("Connecting to Videohub Router");
        let socket = TcpStream::connect(addr).await?;
        let mut framed = Framed::new(socket, VideohubCodec::default());

        // Channels and cache.
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let cache = Arc::new(RwLock::new(Cache::default()));
        let (tx_cache, _) = broadcast::channel(32);

        // Read initial Preamble and DeviceInfo.
        let mut seen_pre = false;
        let mut seen_di = false;
        while !(seen_pre && seen_di) {
            let msg = framed
                .next()
                .await
                .ok_or_else(|| anyhow!("EOF during connect"))??;
            if let VideohubMessage::Preamble(_) = msg {
                seen_pre = true;
            }
            if let VideohubMessage::DeviceInfo(di) = msg.clone() {
                seen_di = true;
                let mut c = cache.write().await;
                c.info = RouterInfo {
                    model: di.model_name.clone(),
                    name: di.friendly_name.clone(),
                    matrix_count: Some(1),
                };
                c.matrix_info = RouterMatrixInfo {
                    input_count: di.video_inputs.ok_or_else(|| {
                        anyhow!("Videohub Device does not contain video input count")
                    })?,
                    output_count: di.video_outputs.ok_or_else(|| {
                        anyhow!("Videohub Device does not contain video output count")
                    })?,
                };
                info!(
                    "Found {}x{} Router",
                    c.matrix_info.input_count, c.matrix_info.output_count
                );
            }
        }

        // 4) build client + spawn loop
        let client = Self {
            cmd_tx,
            cache: cache.clone(),
            cache_tx: tx_cache.clone(),
        };
        tokio::spawn(Self::event_loop(cmd_rx, framed, cache, tx_cache));
        Ok(client)
    }

    /// The single reader/select loop.
    #[tracing::instrument(skip(cmd_rx, framed, cache, cache_tx))]
    async fn event_loop(
        mut cmd_rx: mpsc::UnboundedReceiver<Command>,
        framed: Framed<TcpStream, VideohubCodec>,
        cache: Arc<RwLock<Cache>>,
        cache_tx: broadcast::Sender<CacheEvent>,
    ) {
        let mut pending_commands: VecDeque<oneshot::Sender<bool>> = VecDeque::new();
        let (mut sink, mut stream) = framed.split();

        loop {
            select! {
                // Commands to send
                cmd = cmd_rx.recv() => {
                    match cmd {
                        Some(Command::Send { msg }) => {
                            let _ = sink.send(msg).await;
                        },
                        Some(Command::Ack { msg, resp }) => {
                            // Queue the responder, then actually send the command.
                            pending_commands.push_back(resp);
                            let _ = sink.send(msg).await;
                        },
                        None => {
                            info!("Command receiver closed, stopping");
                            let _ = cache_tx.send(CacheEvent::Disconnected);
                            break;
                        }
                     }
                }

                // Incoming frames
                frame = stream.next() => {
                    let Some(msg) = frame else {
                        info!("Peer closed connection, stopping");
                        let _ = cache_tx.send(CacheEvent::Disconnected);
                        break;
                    };
                    let Ok(msg) = msg else {
                        error!(error = ?msg.unwrap_err(), "Videohub Codec encountered error");
                        break;
                    };

                    // First handle ACK/NAK if any pending
                    if matches!(msg, VideohubMessage::ACK | VideohubMessage::NAK) {
                        if let Some(tx) = pending_commands.pop_front() {
                            let ok = msg == VideohubMessage::ACK;
                            let _ = tx.send(ok);
                        }
                        continue;
                    }

                    // Then update cache
                    let mut c = cache.write().await;
                    match msg {
                        VideohubMessage::DeviceInfo(di) => {
                            if let Some(model) = di.model_name {
                                c.info.model = Some(model);
                            };
                            if let Some(name) = di.friendly_name {
                                c.info.name = Some(name);
                            };

                            if let Some(in_count) = di.video_inputs {
                                c.matrix_info.input_count = in_count;
                            };
                            if let Some(out_count) = di.video_outputs {
                                c.matrix_info.output_count = out_count;
                            };
                        }
                        VideohubMessage::InputLabels(ls) => {
                            let updates = ls.into_iter()
                                  .map(|l| l.into())
                                  .collect();

                            let count = c.matrix_info.input_count;
                            if let Err(e) = update_labels(&mut c.input_labels, updates, count) {
                                error!(error = ?e, "Failed to update labels from received InputLabels message");
                            };
                            let _ = cache_tx.send(CacheEvent::InputLabels);
                        }
                        VideohubMessage::OutputLabels(ls) => {
                            let updates = ls.into_iter()
                                  .map(|l| l.into())
                                  .collect();

                            let count = c.matrix_info.output_count;
                            if let Err(e) = update_labels(&mut c.output_labels, updates, count) {
                                error!(error = ?e, "Failed to update labels from received OutputLabels message");
                            };
                            let _ = cache_tx.send(CacheEvent::OutputLabels);
                        }
                        VideohubMessage::VideoOutputRouting(rs) => {
                            let updates = rs.into_iter()
                                  .map(|p| p.into())
                                  .collect();

                            let in_count = c.matrix_info.input_count;
                            let out_count = c.matrix_info.input_count;
                            if let Err(e) = update_routes(&mut c.routes, updates, in_count, out_count) {
                                error!(error = ?e, "Failed to update routes from received VideoOutputRouting message");
                            };
                            let _ = cache_tx.send(CacheEvent::Routes);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// Send a message expecting ACK/NAK.
    async fn request_acked(&self, msg: VideohubMessage) -> Result<bool> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::Ack { msg, resp: tx })
            .map_err(|_| anyhow!("request channel closed"))?;
        Ok(rx.await.unwrap_or(false))
    }

    /// Send a message and wait for a specific cache event.
    async fn request_and_wait_cache(&self, msg: VideohubMessage, want: CacheEvent) -> Result<()> {
        self.cmd_tx
            .send(Command::Send { msg })
            .map_err(|_| anyhow!("request channel closed"))?;
        let mut rx = self.cache_tx.subscribe();
        while let Ok(ev) = rx.recv().await {
            if ev == want {
                return Ok(());
            }
        }
        Err(anyhow!("no cache event {:?}", want))
    }
}

impl MatrixRouter for VideohubRouter {
    async fn is_alive(&self) -> Result<bool> {
        Ok(self.request_acked(VideohubMessage::Ping).await?)
    }

    async fn get_router_info(&self) -> Result<RouterInfo> {
        let c = self.cache.read().await;
        Ok(c.info.clone())
    }

    async fn get_matrix_info(&self, _idx: u32) -> Result<RouterMatrixInfo> {
        let c = self.cache.read().await;
        Ok(c.matrix_info.clone())
    }

    async fn get_input_labels(&self, _idx: u32) -> Result<Vec<RouterLabel>> {
        {
            let c = self.cache.read().await;
            if let Some(ls) = &c.input_labels {
                return Ok(ls.clone());
            }
        }
        self.request_and_wait_cache(
            VideohubMessage::InputLabels(vec![]),
            CacheEvent::InputLabels,
        )
        .await?;
        let c = self.cache.read().await;
        Ok(c.input_labels.clone().unwrap())
    }

    async fn get_output_labels(&self, _idx: u32) -> Result<Vec<RouterLabel>> {
        {
            let c = self.cache.read().await;
            if let Some(ls) = &c.output_labels {
                return Ok(ls.clone());
            }
        }
        self.request_and_wait_cache(
            VideohubMessage::OutputLabels(vec![]),
            CacheEvent::OutputLabels,
        )
        .await?;
        let c = self.cache.read().await;
        Ok(c.output_labels.clone().unwrap())
    }

    async fn update_input_labels(&self, _idx: u32, changed: Vec<RouterLabel>) -> Result<()> {
        let lbs = changed.clone().into_iter().map(|l| l.into()).collect();
        let ok = self
            .request_acked(VideohubMessage::InputLabels(lbs))
            .await?;
        if ok {
            let mut c = self.cache.write().await;
            let count = c.matrix_info.input_count;
            update_labels(&mut c.input_labels, changed, count)?;
            Ok(())
        } else {
            Err(anyhow!("NAK"))
        }
    }

    async fn update_output_labels(&self, _idx: u32, changed: Vec<RouterLabel>) -> Result<()> {
        let lbs = changed.clone().into_iter().map(|l| l.into()).collect();
        let ok = self
            .request_acked(VideohubMessage::OutputLabels(lbs))
            .await?;
        if ok {
            let mut c = self.cache.write().await;
            let count = c.matrix_info.input_count;
            update_labels(&mut c.input_labels, changed, count)?;
            Ok(())
        } else {
            Err(anyhow!("NAK"))
        }
    }

    async fn get_routes(&self, _idx: u32) -> Result<Vec<RouterPatch>> {
        {
            let c = self.cache.read().await;
            if let Some(r) = &c.routes {
                return Ok(r.clone());
            }
        }
        self.request_and_wait_cache(
            VideohubMessage::VideoOutputRouting(vec![]),
            CacheEvent::Routes,
        )
        .await?;
        let c = self.cache.read().await;
        Ok(c.routes.clone().unwrap())
    }

    async fn update_routes(&self, _idx: u32, changed: Vec<RouterPatch>) -> Result<()> {
        let rs = changed.clone().into_iter().map(|p| p.into()).collect();
        let ok = self
            .request_acked(VideohubMessage::VideoOutputRouting(rs))
            .await?;
        if ok {
            let mut c = self.cache.write().await;
            let in_count = c.matrix_info.input_count;
            let out_count = c.matrix_info.output_count;
            update_routes(&mut c.routes, changed, in_count, out_count)?;
            Ok(())
        } else {
            Err(anyhow!("NAK"))
        }
    }

    async fn event_stream<'a>(&'a self) -> Result<BoxStream<'a, RouterEvent>> {
        let rx = self.cache_tx.subscribe();
        let cache = Arc::clone(&self.cache);
        let bs = BroadcastStream::new(rx)
            .filter_map(move |res| {
                let cache = cache.clone();
                async move {
                    if let Ok(ev) = res {
                        let guard = cache.read().await;
                        match ev {
                            CacheEvent::InputLabels => {
                                let input_labels = guard.input_labels.clone().unwrap_or_default();
                                Some(RouterEvent::InputLabelUpdate(0, input_labels))
                            }
                            CacheEvent::OutputLabels => {
                                let output_labels = guard.output_labels.clone().unwrap_or_default();
                                Some(RouterEvent::OutputLabelUpdate(0, output_labels))
                            }
                            CacheEvent::Routes => {
                                let routes = guard.routes.clone().unwrap_or_default();
                                Some(RouterEvent::RouteUpdate(0, routes))
                            }
                            CacheEvent::Disconnected => Some(RouterEvent::Disconnected),
                        }
                    } else {
                        None
                    }
                }
            })
            .boxed();
        Ok(bs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::VideohubFrontend;
    use crate::matrix::{DummyRouter, RouterEvent, RouterLabel, RouterPatch};
    use anyhow::Result;
    use futures_util::StreamExt;
    use std::net::SocketAddr;
    use std::sync::Arc;
    use tokio::net::TcpListener;
    use tokio::spawn;
    use tokio::time::{timeout, Duration};

    /// Start a frontend with DummyRouter on an ephemeral port, return its address and router.
    async fn spawn_frontend() -> Result<(SocketAddr, DummyRouter)> {
        let dummy = DummyRouter::with_config(1, 3, 3);
        let fe = VideohubFrontend::new(Arc::new(dummy.clone()), 0);
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        spawn(async move {
            // Accept only one connection.
            fe.serve(listener).await.unwrap();
        });
        Ok((addr, dummy))
    }

    #[tokio::test]
    async fn ping_and_matrix_info() -> Result<()> {
        let (addr, _dummy) = spawn_frontend().await?;
        let client = VideohubRouter::connect(addr).await?;

        assert!(client.is_alive().await?);

        let mi = client.get_matrix_info(0).await?;
        assert_eq!(mi.input_count, 3);
        assert_eq!(mi.output_count, 3);
        Ok(())
    }

    #[tokio::test]
    async fn labels_roundtrip() -> Result<()> {
        let (addr, dummy) = spawn_frontend().await?;
        let client = VideohubRouter::connect(addr).await?;

        // Assert baseline is working.
        let in0 = client.get_input_labels(0).await?;
        assert_eq!(in0.len(), 3);

        // Change a label.
        let new = RouterLabel {
            id: 1,
            name: "X".into(),
        };
        client.update_input_labels(0, vec![new.clone()]).await?;

        // Backend sees it despite cache.
        let in1 = client.get_input_labels(0).await?;
        assert!(in1.contains(&new));

        // Frontend applied it to Dummy.
        let dlabels = dummy.get_input_labels(0).await?;
        assert!(dlabels.contains(&new));

        Ok(())
    }

    #[tokio::test]
    async fn routes_roundtrip() -> Result<()> {
        let (addr, dummy) = spawn_frontend().await?;
        let client = VideohubRouter::connect(addr).await?;
        let r0 = client.get_routes(0).await?;
        assert_eq!(r0.len(), 3);

        // update one route
        let p = RouterPatch {
            from_input: 2,
            to_output: 1,
        };
        client.update_routes(0, vec![p.clone()]).await?;

        // dummy sees it
        let dr = dummy.get_routes(0).await?;
        assert!(dr.contains(&p));

        // backend sees it
        let r1 = client.get_routes(0).await?;
        assert!(r1.contains(&p));
        Ok(())
    }

    #[tokio::test]
    async fn event_stream_routes() -> Result<()> {
        let (addr, dummy) = spawn_frontend().await?;
        let client = VideohubRouter::connect(addr).await?;
        // cause a route change in dummy
        let p = RouterPatch {
            from_input: 1,
            to_output: 0,
        };

        // Ensure we get a clean event stream.
        let _ = dummy.get_routes(0).await?;
        let mut es = client.event_stream().await?;

        dummy.push_event(RouterEvent::RouteUpdate(0, vec![p.clone()]));
        let mut found = false;
        for _ in 0..5 {
            let ev = timeout(Duration::from_secs(1), es.next())
                .await?
                .expect("Expecting an event!");
            if let RouterEvent::RouteUpdate(0, elems) = ev {
                if elems.contains(&p) {
                    found = true;
                    break;
                };
            };
        }
        assert!(found);
        Ok(())
    }
}
