use super::*;
use anyhow::{anyhow, Result};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};

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
    labels: Vec<RouterLabels>,
    routes: Vec<Vec<u32>>,
}

impl DummyRouter {
    /// Create a dummy with given matrix_count, uniform input_count and output_count per matrix.
    pub fn with_config(matrix_count: usize, input_count: usize, output_count: usize) -> Self {
        let info = RouterInfo {
            model: None,
            name: None,
            matrix_count: Some(matrix_count as u32),
        };
        let matrix_info = vec![
            RouterMatrixInfo {
                input_count: Some(input_count as u32),
                output_count: Some(output_count as u32)
            };
            matrix_count
        ];
        let labels = vec![RouterLabels::default(); matrix_count];
        let routes = vec![vec![0; output_count]; matrix_count];
        let state = State {
            is_alive: true,
            info,
            matrix_info,
            labels,
            routes,
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

    async fn get_router_matrix_info(&self, index: u32) -> Result<RouterMatrixInfo> {
        let st = self.state.lock().unwrap();
        Self::validate_index(&st, index)?;
        Ok(st.matrix_info[index as usize].clone())
    }

    async fn get_labels(&self, index: u32) -> Result<RouterLabels> {
        let st = self.state.lock().unwrap();
        Self::validate_index(&st, index)?;
        Ok(st.labels[index as usize].clone())
    }

    async fn update_labels(&self, index: u32, changed: RouterLabels) -> Result<()> {
        let mut st = self.state.lock().unwrap();
        Self::validate_index(&st, index)?;
        let idx = index as usize;
        if !changed.input.is_empty() {
            st.labels[idx].input = changed.input;
        }
        if !changed.output.is_empty() {
            st.labels[idx].output = changed.output;
        }
        Ok(())
    }

    async fn get_routes(&self, index: u32) -> Result<Vec<Patch>> {
        let st = self.state.lock().unwrap();
        Self::validate_index(&st, index)?;
        let row = &st.routes[index as usize];
        Ok(row
            .iter()
            .enumerate()
            .map(|(out, &inp)| Patch {
                from_input: inp,
                to_output: out as u32,
            })
            .collect())
    }

    async fn update_routes(&self, index: u32, changes: Vec<Patch>) -> Result<()> {
        let mut st = self.state.lock().unwrap();
        Self::validate_index(&st, index)?;
        let idx = index as usize;
        let outputs = st.matrix_info[idx].output_count.unwrap_or(0) as usize;
        let inputs = st.matrix_info[idx].input_count.unwrap_or(0) as usize;
        for p in changes {
            let out = p.to_output as usize;
            let inp = p.from_input as usize;
            if inp >= inputs || out >= outputs {
                return Err(anyhow!("Patch {:?} out of bounds for matrix {}", p, index));
            }
            st.routes[idx][out] = p.from_input;
        }
        Ok(())
    }

    async fn event_stream(&self) -> Result<impl futures_core::Stream<Item = RouterEvent>> {
        let bs = BroadcastStream::new(self.tx.subscribe());
        let simple = bs.filter_map(|r| match r {
            Ok(e) => Some(e),
            Err(_) => None,
        });
        Ok(simple)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_stream::StreamExt;

    #[tokio::test]
    async fn constructor_and_bounds() {
        let dummy = DummyRouter::with_config(2, 3, 4);
        let mi = dummy.get_router_matrix_info(0).await.unwrap();
        assert_eq!(mi.input_count, Some(3));
        assert_eq!(mi.output_count, Some(4));
        assert!(dummy.get_router_matrix_info(1).await.is_ok());
        assert!(dummy.get_router_matrix_info(5).await.is_err());
    }

    #[tokio::test]
    async fn patch_bounds_and_routing() {
        let dummy = DummyRouter::with_config(1, 2, 2);
        let p = Patch {
            from_input: 1,
            to_output: 1,
        };
        dummy.update_routes(0, vec![p]).await.unwrap();
        let routes = dummy.get_routes(0).await.unwrap();
        assert!(routes.contains(&p));
        let bad = Patch {
            from_input: 5,
            to_output: 0,
        };
        assert!(dummy.update_routes(0, vec![bad]).await.is_err());
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

