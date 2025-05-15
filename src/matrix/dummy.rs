use super::*;
use anyhow::{anyhow, Result};
use futures_core::stream::BoxStream;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};
use tracing::error;

/// Dummy router implementation for testing and mocking
#[derive(Clone)]
pub struct DummyRouter {
    state: Arc<Mutex<State>>,
    tx: broadcast::Sender<RouterEvent>,
}

struct State {
    is_alive: bool,
    info: RouterInfo,
    matrix_info: Vec<RouterMatrixInfo>,
    input_labels: Vec<Vec<RouterLabel>>,
    output_labels: Vec<Vec<RouterLabel>>,
    routes: Vec<Vec<RouterPatch>>,
}

impl DummyRouter {
    /// Create a dummy with given matrix_count, uniform input_count and output_count per matrix.
    pub fn with_config(matrix_count: usize, input_count: usize, output_count: usize) -> Self {
        let info = RouterInfo {
            model: Some(format!("DummyRouter {}x{}", input_count, output_count)),
            name: None,
            matrix_count: Some(matrix_count as u32),
        };
        let matrix_info = vec![
            RouterMatrixInfo {
                input_count: input_count as u32,
                output_count: output_count as u32,
            };
            matrix_count
        ];

        let input_labels: Vec<RouterLabel> = (0..input_count)
            .map(|n| RouterLabel {
                id: n as u32,
                name: format!("Input {}", n + 1),
            })
            .collect();

        let output_labels: Vec<RouterLabel> = (0..output_count)
            .map(|n| RouterLabel {
                id: n as u32,
                name: format!("Output {}", n + 1),
            })
            .collect();

        let patches: Vec<RouterPatch> = (0..output_count)
            .map(|n| RouterPatch {
                from_input: 0,
                to_output: n as u32,
            })
            .collect();

        let state = State {
            is_alive: true,
            info,
            matrix_info,
            input_labels: vec![input_labels; matrix_count],
            output_labels: vec![output_labels; matrix_count],
            routes: vec![patches; matrix_count],
        };
        let (tx, _) = broadcast::channel(16);
        DummyRouter {
            state: Arc::new(Mutex::new(state)),
            tx,
        }
    }

    /// Default dummy with a single 16×16 matrix.
    pub fn new() -> Self {
        Self::with_config(1, 16, 16)
    }

    /// Update the static info.
    pub fn set_info(&self, info: RouterInfo) {
        self.state.lock().unwrap().info = info;
    }

    /// Broadcast a new event to all subscribers.
    pub fn push_event(&self, ev: RouterEvent) {
        let _ = self.tx.send(ev);
    }

    /// Validate that matrix index is in range
    fn validate_index(st: &State, index: u32) -> Result<()> {
        if (index as usize) < st.matrix_info.len() {
            Ok(())
        } else {
            Err(anyhow!("Matrix index {} out of range", index))
        }
    }
}

impl MatrixRouter for DummyRouter {
    async fn is_alive(&self) -> Result<bool> {
        Ok(self.state.lock().unwrap().is_alive)
    }

    async fn get_router_info(&self) -> Result<RouterInfo> {
        Ok(self.state.lock().unwrap().info.clone())
    }

    async fn get_matrix_info(&self, index: u32) -> Result<RouterMatrixInfo> {
        let st = self.state.lock().unwrap();
        Self::validate_index(&st, index)?;
        Ok(st.matrix_info[index as usize].clone())
    }

    async fn get_input_labels(&self, index: u32) -> Result<Vec<RouterLabel>> {
        let st = self.state.lock().unwrap();
        Self::validate_index(&st, index)?;
        Ok(st.input_labels[index as usize].clone())
    }
    async fn get_output_labels(&self, index: u32) -> Result<Vec<RouterLabel>> {
        let st = self.state.lock().unwrap();
        Self::validate_index(&st, index)?;
        Ok(st.output_labels[index as usize].clone())
    }

    async fn update_input_labels(&self, index: u32, changed: Vec<RouterLabel>) -> Result<()> {
        let mut st = self.state.lock().unwrap();
        Self::validate_index(&st, index)?;
        let idx = index as usize;
        let mi = st.matrix_info[idx].clone();
        let mut changes_happened = false;
        for change in changed {
            if change.id >= mi.input_count {
                return Err(anyhow!("Can't update an input label outside of range!"));
            }
            st.input_labels[idx][change.id as usize].name = change.name;
            changes_happened = true;
        }

        // Broadcast the current labels if any changes occured.
        if changes_happened {
            if self
                .tx
                .send(RouterEvent::InputLabelUpdate(
                    index,
                    st.input_labels[idx].clone(),
                ))
                .is_err()
            {
                error!("InputLabelUpdate Event happened, but channel closed!")
            }
        }
        Ok(())
    }
    async fn update_output_labels(&self, index: u32, changed: Vec<RouterLabel>) -> Result<()> {
        let mut st = self.state.lock().unwrap();
        Self::validate_index(&st, index)?;
        let idx = index as usize;
        let mi = st.matrix_info[idx].clone();
        let mut changes_happened = false;
        for change in changed {
            if change.id >= mi.output_count {
                return Err(anyhow!("Can't update an output label outside of range!"));
            }
            st.output_labels[idx][change.id as usize].name = change.name;
            changes_happened = true;
        }

        // Broadcast the current labels if any changes occured.
        if changes_happened {
            if self
                .tx
                .send(RouterEvent::OutputLabelUpdate(
                    index,
                    st.output_labels[idx].clone(),
                ))
                .is_err()
            {
                error!("OutputLabelUpdate Event happened, but channel closed!")
            }
        }
        Ok(())
    }

    async fn get_routes(&self, index: u32) -> Result<Vec<RouterPatch>> {
        let st = self.state.lock().unwrap();
        Self::validate_index(&st, index)?;
        let row = &st.routes[index as usize];
        Ok(row.clone())
    }

    async fn update_routes(&self, index: u32, changes: Vec<RouterPatch>) -> Result<()> {
        let mut st = self.state.lock().unwrap();
        Self::validate_index(&st, index)?;
        let idx = index as usize;
        let outputs = st.matrix_info[idx].output_count as usize;
        let inputs = st.matrix_info[idx].input_count as usize;
        let mut changes_happened = false;
        for p in changes {
            let out = p.to_output as usize;
            let inp = p.from_input as usize;
            if inp >= inputs || out >= outputs {
                return Err(anyhow!("Patch {:?} out of bounds for matrix {}", p, index));
            }
            st.routes[idx][out].from_input = p.from_input;
            changes_happened = true;
        }

        // Broadcast
        if changes_happened {
            if self
                .tx
                .send(RouterEvent::RouteUpdate(index, st.routes[idx].clone()))
                .is_err()
            {
                error!("RouteUpdate event happened, but channel closed!")
            }
        }
        Ok(())
    }

    async fn event_stream<'a>(&'a self) -> Result<BoxStream<'a, RouterEvent>> {
        let bs = BroadcastStream::new(self.tx.subscribe());
        let simple = bs.filter_map(|r| r.ok());
        Ok(futures_util::StreamExt::boxed(simple))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_stream::StreamExt;

    #[tokio::test]
    async fn constructor_and_bounds() {
        let dummy = DummyRouter::with_config(2, 3, 4);
        let mi = dummy.get_matrix_info(0).await.unwrap();
        assert_eq!(mi.input_count, 3);
        assert_eq!(mi.output_count, 4);
        assert!(dummy.get_matrix_info(1).await.is_ok());
        assert!(dummy.get_matrix_info(5).await.is_err());
    }

    #[tokio::test]
    async fn patch_bounds_and_routing() {
        let dummy = DummyRouter::with_config(1, 2, 2);
        let mut stream = dummy.event_stream().await.unwrap();
        let p = RouterPatch {
            from_input: 1,
            to_output: 1,
        };
        dummy.update_routes(0, vec![p]).await.unwrap();

        let routes = dummy.get_routes(0).await.unwrap();
        assert!(routes.contains(&p));

        let event = stream
            .next()
            .await
            .expect("Expected a RouteUpdate event here!");
        let route_update = match event {
            RouterEvent::RouteUpdate(0, routes) => routes,
            _ => panic!("RouterEvent wasn't RouteUpdate!"),
        };
        assert!(
            route_update.contains(&p),
            "RouteUpdate doesn't contain patch"
        );

        let bad = RouterPatch {
            from_input: 5,
            to_output: 0,
        };
        assert!(dummy.update_routes(0, vec![bad]).await.is_err());
    }

    #[tokio::test]
    async fn input_labels() {
        let dummy = DummyRouter::with_config(1, 2, 2);
        let mut stream = dummy.event_stream().await.unwrap();
        let l = RouterLabel {
            id: 0,
            name: "Test Case".to_owned(),
        };
        dummy.update_input_labels(0, vec![l.clone()]).await.unwrap();

        let labels = dummy.get_input_labels(0).await.unwrap();
        assert!(labels.contains(&l));

        let event = stream
            .next()
            .await
            .expect("Expected an InputLabelUpdate event here!");
        let label_update = match event {
            RouterEvent::InputLabelUpdate(0, labels) => labels,
            _ => panic!("RouterEvent wasn't InputLabelUpdate!"),
        };
        assert!(
            label_update.contains(&l),
            "InputLabelUpdate doesn't contain label"
        );

        let bad = RouterLabel {
            id: 5,
            name: "Bad".to_string(),
        };
        assert!(dummy.update_input_labels(0, vec![bad]).await.is_err());
    }
    #[tokio::test]
    async fn output_labels() {
        let dummy = DummyRouter::with_config(1, 2, 2);
        let mut stream = dummy.event_stream().await.unwrap();
        let l = RouterLabel {
            id: 0,
            name: "Test Case".to_owned(),
        };
        dummy
            .update_output_labels(0, vec![l.clone()])
            .await
            .unwrap();

        let labels = dummy.get_output_labels(0).await.unwrap();
        assert!(labels.contains(&l));

        let event = stream
            .next()
            .await
            .expect("Expected an OutputLabelUpdate event here!");
        let label_update = match event {
            RouterEvent::OutputLabelUpdate(0, labels) => labels,
            _ => panic!("RouterEvent wasn't OutputLabelUpdate!"),
        };
        assert!(
            label_update.contains(&l),
            "OutputLabelUpdate doesn't contain label"
        );

        let bad = RouterLabel {
            id: 5,
            name: "Bad".to_string(),
        };
        assert!(dummy.update_output_labels(0, vec![bad]).await.is_err());
    }

    #[tokio::test]
    async fn event_stream() {
        let dummy = DummyRouter::new();
        let mut stream = dummy.event_stream().await.unwrap();
        dummy.push_event(RouterEvent::Connected);
        assert_eq!(stream.next().await, Some(RouterEvent::Connected));
        dummy.push_event(RouterEvent::Disconnected);
        assert_eq!(stream.next().await, Some(RouterEvent::Disconnected));
    }
}
