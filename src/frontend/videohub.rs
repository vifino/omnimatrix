use crate::matrix::{MatrixRouter, RouterEvent};
use anyhow::Result;
use async_stream::try_stream;
use futures_util::pin_mut;
use futures_util::SinkExt;
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::Mutex;
use tokio::{
    net::{TcpListener, TcpStream},
    select,
};
use tokio_stream::{Stream, StreamExt};
use tokio_util::codec::Framed;
use tracing::{debug, error, info};
use videohub::*;

/// Holds the router and any cached protocol state
struct VideohubFrontendState {
    // add other cached state here
}

impl VideohubFrontendState {
    pub fn new() -> Self {
        Self {}
    }
}

/// Frontend bridging TCP‐Videohub clients to a MatrixRouter
pub struct VideohubFrontend<S> {
    pub router: Arc<S>,
    index: u32,
    state: Arc<Mutex<VideohubFrontendState>>,
    peer: Option<SocketAddr>,
}

impl<S> VideohubFrontend<S>
where
    S: MatrixRouter + Send + Sync + Clone + 'static,
{
    pub fn new(router: Arc<S>, index: u32) -> Self {
        Self {
            router,
            index,
            state: Arc::new(Mutex::new(VideohubFrontendState::new())),
            peer: None,
        }
    }

    /// Accept connections on existing TcpListener, spawning tasks per client
    #[tracing::instrument(skip(self, listener), fields(addr = ?listener.local_addr()?))]
    pub async fn serve(self, listener: TcpListener) -> Result<()> {
        info!("Serving on existing Listener");
        loop {
            let (socket, peer) = listener.accept().await?;
            info!(?peer, "Got connection");
            let mut frontend = self.clone();
            frontend.peer = Some(peer);
            tokio::spawn(async move {
                if let Err(e) = frontend.handle_connection(socket).await {
                    error!(?peer, error = ?e, "handle_connection returned error");
                }
            });
        }
    }

    /// Bind and accept connections, spawning tasks per client
    #[tracing::instrument(skip(self))]
    pub async fn listen(self, addr: SocketAddr) -> Result<()> {
        let listener = TcpListener::bind(addr).await?;
        info!("Listener bound successfully");
        loop {
            let (socket, peer) = listener.accept().await?;
            info!(?peer, "Got connection");
            let mut frontend = self.clone();
            frontend.peer = Some(peer);
            tokio::spawn(async move {
                if let Err(e) = frontend.handle_connection(socket).await {
                    error!(?peer, error = ?e, "handle_connection returned error");
                }
            });
        }
    }

    #[tracing::instrument(skip(self, socket), fields(?peer = self.peer.unwrap()))]
    async fn handle_connection(self, socket: TcpStream) -> Result<()> {
        let mut framed = Framed::new(socket, VideohubCodec::default());

        let mut ev_stream = self.router.event_stream().await?;

        debug!("Sending initial dump");
        let dump = self.create_initial_dump();
        pin_mut!(dump);
        while let Some(msg) = dump.next().await {
            framed.send(msg?).await?;
        }
        debug!("Dump done");

        loop {
            select! {
                // Client sent a message to us, expecting the response of a router.
                maybe = framed.next() => match maybe {
                    Some(Ok(msg)) => {
                        debug!(?msg, "Got message");
                        if let Some(reply) = self.handle_message(msg).await? {
                            debug!(?reply, "Replying");
                            framed.send(reply).await?;
                        }
                    }
                    Some(Err(e)) => return Err(e.into()),
                    None => break, // client closed
                },

                // Router (Backend) sent an event to us, translate and forward to client.
                Some(ev) = ev_stream.next() => {
                    debug!(?ev, "Got event");
                    if let Some(reply) = self.handle_event(ev).await? {
                        debug!(?reply, "Sending converted event");
                        framed.send(reply).await?;
                    }
                }
            }
        }
        info!("Closed connection");
        Ok(())
    }

    /// Create the initial dump expected by the client.
    fn create_initial_dump(&self) -> impl Stream<Item = Result<VideohubMessage>> + use<'_, S> {
        try_stream! {

            // 1) Say hello, send some version that should be appropriate to what we're doing.
            yield VideohubMessage::Preamble(Preamble {
                version: "2.7".into(),
            });

            // 2) Identify as a VIDEOHUB device.
            let mut di = DeviceInfo::default();
            let mut output_count = 0;
            let alive = self.router.is_alive().await?;
            di.present = Some(if alive { Present::Yes } else { Present::No });
            if alive {
                let si = self.router.get_router_info().await?;
                di.model_name = si.model;
                di.friendly_name = si.name;

                let mi = self.router.get_matrix_info(self.index).await?;
                output_count = mi.output_count;
                di.video_inputs = Some(mi.input_count);
                di.video_outputs = Some(output_count);

                // TODO: Is sending more fields necessary?
            }
            yield VideohubMessage::DeviceInfo(di);

            if alive {
                // 3) Input Labels
                yield self.gen_inputlabels().await?;

                // 4) Output Labels
                yield self.gen_outputlabels().await?;

                // 5) Output Locks - stub for now.
                let mut locks = Vec::new();
                for id in 0..output_count {
                    locks.push(Lock {
                        id,
                        state: LockState::Unlocked,
                    })
                }
                // 6) Video Output Routing - the juicy bits!
                yield self.gen_routing().await?;
           }
            // 7) That's all!
            yield VideohubMessage::EndPrelude;
        }
    }

    /// Generate InputLabels Message
    async fn gen_inputlabels(&self) -> Result<VideohubMessage> {
        let mut input_labels = self.router.get_input_labels(self.index).await?;
        input_labels.sort_by(|a, b| a.id.cmp(&b.id)); // Enforce 0 to X
        return Ok(VideohubMessage::InputLabels(
            input_labels.into_iter().map(|l| l.into()).collect(),
        ));
    }

    /// Generate OutputLabels Message
    async fn gen_outputlabels(&self) -> Result<VideohubMessage> {
        let mut output_labels = self.router.get_output_labels(self.index).await?;
        output_labels.sort_by(|a, b| a.id.cmp(&b.id)); // Enforce 0 to X
        return Ok(VideohubMessage::OutputLabels(
            output_labels.into_iter().map(|l| l.into()).collect(),
        ));
    }

    /// Generate VideoOutputRouting Message
    async fn gen_routing(&self) -> Result<VideohubMessage> {
        let mut routes = self.router.get_routes(self.index).await?;
        routes.sort_by(|a, b| a.to_output.cmp(&b.to_output)); // Enforce 0 to X
        return Ok(VideohubMessage::VideoOutputRouting(
            routes.into_iter().map(|r| r.into()).collect(),
        ));
    }

    /// Message handler: update state, optionally call router
    async fn handle_message(&self, msg: VideohubMessage) -> Result<Option<VideohubMessage>> {
        // TODO: handle PING locally, call self.router.get_routes() and such if needed
        Ok(match msg {
            VideohubMessage::Ping => Some(VideohubMessage::ACK),
            VideohubMessage::InputLabels(labels) => {
                if labels.is_empty() {
                    Some(self.gen_inputlabels().await?)
                } else {
                    let changed = labels.into_iter().map(|l| l.into()).collect();
                    self.router
                        .update_input_labels(self.index, changed)
                        .await?;
                    Some(VideohubMessage::ACK)
                }
            }
            VideohubMessage::OutputLabels(labels) => {
                if labels.is_empty() {
                    Some(self.gen_outputlabels().await?)
                } else {
                    let changed = labels.into_iter().map(|l| l.into()).collect();
                    self.router
                        .update_output_labels(self.index, changed)
                        .await?;
                    Some(VideohubMessage::ACK)
                }
            }
            VideohubMessage::VideoOutputRouting(routes) => {
                if routes.is_empty() {
                    Some(self.gen_routing().await?)
                } else {
                    let changed = routes.into_iter().map(|r| r.into()).collect();
                    self.router.update_routes(self.index, changed).await?;
                    Some(VideohubMessage::ACK)
                }
            }
            _ => Some(VideohubMessage::NAK),
        })
    }

    /// Event handler: update state, produce protocol message if desired
    /// Luckily, we don't need to filter out changes we did on our own, cause the Videohub protocol
    /// does the same on original devices.
    async fn handle_event(&self, event: RouterEvent) -> Result<Option<VideohubMessage>> {
        // TODO: translate stuff like route-change events
        Ok(match event {
            RouterEvent::RouteUpdate(idx, mut updates) => {
                if idx != self.index {
                    None
                } else {
                    updates.sort_by(|a, b| a.to_output.cmp(&b.to_output)); // Enforce 0 to X
                    Some(VideohubMessage::VideoOutputRouting(
                        updates.into_iter().map(|r| r.into()).collect(),
                    ))
                }
            }
            _ => None,
        })
    }
}

impl<S> Clone for VideohubFrontend<S>
where
    S: MatrixRouter + Clone,
{
    fn clone(&self) -> Self {
        Self {
            router: Arc::clone(&self.router),
            index: self.index,
            state: self.state.clone(),
            peer: self.peer.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matrix::{DummyRouter, RouterPatch};
    use tokio_stream::StreamExt;
    use videohub::{Label, VideohubMessage};

    const IDX: u32 = 0;

    #[tokio::test]
    async fn initial_dump() {
        let dummy = Arc::new(DummyRouter::with_config(1, 2, 2));
        let frontend = VideohubFrontend::new(dummy, IDX);
        let dump = frontend.create_initial_dump();
        pin_mut!(dump);
        let mut items = Vec::new();
        while let Some(item) = dump.next().await {
            items.push(item.unwrap());
        }

        // Just making sure all the expected messages are there and in order.
        assert!(matches!(items[0], VideohubMessage::Preamble(..)));
        assert!(matches!(items[1], VideohubMessage::DeviceInfo(..)));
        assert!(matches!(items[2], VideohubMessage::InputLabels(..)));
        assert!(matches!(items[3], VideohubMessage::OutputLabels(..)));
        assert!(matches!(items[4], VideohubMessage::VideoOutputRouting(..)));
        assert_eq!(items[5], VideohubMessage::EndPrelude);
    }

    #[tokio::test]
    async fn ping_and_label_update() {
        let dummy = Arc::new(DummyRouter::with_config(1, 2, 2));
        let frontend = VideohubFrontend::new(Arc::clone(&dummy), IDX);

        // Ping!
        let resp = frontend
            .handle_message(VideohubMessage::Ping)
            .await
            .unwrap();
        assert_eq!(resp, Some(VideohubMessage::ACK));

        // Request labels.
        let resp = frontend
            .handle_message(VideohubMessage::InputLabels(vec![]))
            .await
            .unwrap();
        assert!(matches!(resp, Some(VideohubMessage::InputLabels(_))));

        // Update one label.
        let test_label = Label {
            id: 1,
            name: "Test Label".to_owned(),
        };
        let resp = frontend
            .handle_message(VideohubMessage::InputLabels(vec![test_label.clone()]))
            .await
            .unwrap();
        assert_eq!(resp, Some(VideohubMessage::ACK));

        // Assert Dummy actually got updated
        let actual = dummy.get_input_labels(IDX).await.unwrap();
        assert!(actual.contains(&test_label.into()));
    }

    #[tokio::test]
    async fn route_update_event() {
        let dummy = Arc::new(DummyRouter::with_config(1, 2, 2));
        let frontend = VideohubFrontend::new(dummy, IDX);

        // Simulate a route update event.
        let patches = vec![RouterPatch {
            from_input: 1,
            to_output: 0,
        }];
        let ev = RouterEvent::RouteUpdate(IDX, patches.clone());
        let maybe = frontend.handle_event(ev).await.unwrap();

        // Should produce a VideoOutputRouting message
        if let Some(VideohubMessage::VideoOutputRouting(rr)) = maybe {
            let converted: Vec<RouterPatch> = rr.into_iter().map(|p| p.into()).collect();
            assert_eq!(converted, patches);
        } else {
            panic!("expected VideoOutputRouting");
        }
    }
}
