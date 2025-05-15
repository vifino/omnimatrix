use crate::matrix::*;
use anyhow::{anyhow, Result};
use futures_core::stream::BoxStream;
use ndi_sdk::{FindInstance, RouteInstance, Source};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};
use tracing::{debug, error};

#[derive(Clone)]
pub struct NDIRouter {
    group: Arc<Vec<String>>,
    state: Arc<Mutex<State>>,
    tx: broadcast::Sender<RouterEvent>,
}

struct State {
    info: RouterInfo,
    matrix_info: RouterMatrixInfo,
    input_labels: Vec<RouterLabel>,
    output_labels: Vec<RouterLabel>,
    routes: Vec<RouterPatch>,
    source_map: HashMap<String, String>,
    route_instances: Vec<RouteInstance>,
}

impl NDIRouter {
    pub fn new(
        name: &str,
        group: Vec<&str>,
        max_inputs: usize,
        output_count: usize,
    ) -> Result<Self> {
        let name = name.to_string();
        let group: Arc<Vec<String>> = Arc::new(group.into_iter().map(String::from).collect());

        let info = RouterInfo {
            model: Some("NDIRouter".into()),
            name: Some(name.clone()),
            matrix_count: Some(1),
        };
        let matrix_info = RouterMatrixInfo {
            input_count: max_inputs as u32,
            output_count: output_count as u32,
        };

        let input_labels: Vec<RouterLabel> = (0..max_inputs)
            .map(|i| RouterLabel {
                id: i as u32,
                name: String::new(),
            })
            .collect();

        let output_labels: Vec<RouterLabel> = (0..output_count)
            .map(|i| RouterLabel {
                id: i as u32,
                name: format!("{} {}", name, i + 1),
            })
            .collect();

        let routes = (0..output_count)
            .map(|i| RouterPatch {
                from_input: 0,
                to_output: i as u32,
            })
            .collect();

        let mut ris = Vec::with_capacity(output_count);
        let group_ref: Vec<&str> = group.iter().map(|e| e.as_ref()).collect();
        for lbl in output_labels.iter() {
            let ri = RouteInstance::create(&lbl.name, &group_ref)?;
            ris.push(ri);
        }

        let state = Arc::new(Mutex::new(State {
            info,
            matrix_info,
            input_labels,
            output_labels,
            routes,
            source_map: HashMap::new(),
            route_instances: ris,
        }));

        let (tx, _) = broadcast::channel(16);

        let router = NDIRouter {
            group: group.clone(),
            state: state.clone(),
            tx: tx.clone(),
        };

        router.spawn_worker();
        Ok(router)
    }

    fn assert_matrix_zero(index: u32) -> Result<()> {
        if index != 0 {
            return Err(anyhow!("Only matrix 0 supported"));
        }
        Ok(())
    }

    fn own_output_names(st: &State) -> Vec<&str> {
        st.output_labels.iter().map(|l| l.name.as_str()).collect()
    }

    /// Should we skip this source?
    fn is_own(source: &Source, own_names: &[&str]) -> bool {
        if !source.url_address.starts_with("127.0.0.1") {
            return false;
        }

        own_names
            .iter()
            .any(|own| source.ndi_name.ends_with(&format!(" ({})", own)))
    }

    /// Patch output to input, both in state as with NDI
    fn patch_output(st: &mut State, output: u32, input: u32) -> Result<()> {
        let name = &st.input_labels[input as usize].name;
        if name.is_empty() {
            // No label -> No Source -> Clear.
            st.route_instances[output as usize].clear()?;
            debug!("Cleared NDI Output {}", output);
        } else {
            let url = st
                .source_map
                .get(name)
                .ok_or_else(|| anyhow!("No such source '{}'", name))?;
            let src = Source {
                ndi_name: name.clone(),
                url_address: url.clone(),
            };
            st.route_instances[output as usize].change(&src)?;
            debug!("Patched NDI Output {} to Input {}", output, input);
        }
        st.routes[output as usize].from_input = input;
        Ok(())
    }

    fn spawn_worker(&self) {
        let state = self.state.clone();
        let tx = self.tx.clone();

        tokio::spawn(async move {
            let mut finder = match FindInstance::create(None) {
                Ok(f) => f,
                Err(e) => {
                    error!("FindInstance failed: {:?}", e);
                    return;
                }
            };

            loop {
                {
                    let sources = finder.get_current_sources().unwrap_or_default();

                    let mut st = state.lock().unwrap();

                    let own_names = Self::own_output_names(&st);
                    let mut current = HashMap::new();
                    for s in sources {
                        if !Self::is_own(&s, &own_names) {
                            current.insert(s.ndi_name.clone(), s.url_address.clone());
                        }
                    }

                    let mut actually_changed = false;
                    let old: Vec<_> = st.source_map.keys().cloned().collect();

                    // Removed NDI sources
                    for ndi_name in old {
                        if !current.contains_key(&ndi_name) {
                            // clear its input slot
                            if let Some(pos) =
                                st.input_labels.iter_mut().position(|l| l.name == ndi_name)
                            {
                                st.input_labels[pos].name.clear();
                                // unpatch any outputs on that input
                                for out in 0..st.routes.len() {
                                    if st.routes[out].from_input as usize == pos {
                                        if let Err(e) = Self::patch_output(&mut st, out as u32, 0) {
                                            error!("Failed to patch output {} with removed source to source 0: {:?}", out, e);
                                        }
                                    }
                                }
                            }
                            st.source_map.remove(&ndi_name);
                            debug!(?ndi_name, "Removed NDI Source");
                            actually_changed = true;
                        }
                    }

                    // New sources and URL changes
                    for (ndi_name, url) in current.iter() {
                        match st.source_map.get::<String>(ndi_name) {
                            None => {
                                // New source, find blank label slot.
                                if let Some(slot) =
                                    st.input_labels.iter_mut().find(|l| l.name.is_empty())
                                {
                                    let id = slot.id;
                                    slot.name = ndi_name.clone();
                                    st.source_map.insert(ndi_name.clone(), url.clone());
                                    actually_changed = true;
                                    debug!(?ndi_name, input = ?id, "New NDI Source");
                                }
                            }
                            Some(old_url) if old_url != url => {
                                // URL changed, re-route any outputs
                                st.source_map.insert(ndi_name.clone(), url.clone());
                                let input_index = st
                                    .input_labels
                                    .iter()
                                    .position(|l| &l.name == ndi_name)
                                    .unwrap();
                                debug!(?ndi_name, input = ?input_index, "Updated NDI Source URL");
                                for patch in &st.routes {
                                    if patch.from_input as usize == input_index {
                                        let out = patch.to_output as usize;
                                        let src = Source {
                                            ndi_name: ndi_name.clone(),
                                            url_address: url.clone(),
                                        };
                                        if let Err(e) = st.route_instances[out].change(&src) {
                                            error!("Re-route failed on {}: {:?}", out, e);
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }

                    if actually_changed {
                        let _ = tx.send(RouterEvent::InputLabelUpdate(0, st.input_labels.clone()));
                    }
                }

                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        });
    }
}

impl MatrixRouter for NDIRouter {
    async fn is_alive(&self) -> Result<bool> {
        Ok(true)
    }

    async fn get_router_info(&self) -> Result<RouterInfo> {
        Ok(self.state.lock().unwrap().info.clone())
    }

    async fn get_matrix_info(&self, index: u32) -> Result<RouterMatrixInfo> {
        Self::assert_matrix_zero(index)?;
        Ok(self.state.lock().unwrap().matrix_info.clone())
    }

    async fn get_input_labels(&self, index: u32) -> Result<Vec<RouterLabel>> {
        Self::assert_matrix_zero(index)?;
        Ok(self.state.lock().unwrap().input_labels.clone())
    }

    async fn get_output_labels(&self, index: u32) -> Result<Vec<RouterLabel>> {
        Self::assert_matrix_zero(index)?;
        Ok(self.state.lock().unwrap().output_labels.clone())
    }

    async fn update_input_labels(&self, _: u32, _: Vec<RouterLabel>) -> Result<()> {
        Err(anyhow!("NDI inputs auto-managed"))
    }

    async fn update_output_labels(&self, index: u32, changed: Vec<RouterLabel>) -> Result<()> {
        Self::assert_matrix_zero(index)?;
        let mut st = self.state.lock().unwrap();
        let mut actually_changed = false;
        for label in changed {
            let i = label.id as usize;
            if i >= st.output_labels.len() {
                return Err(anyhow!("Output {} out of range", i));
            }
            if st.output_labels[i].name != label.name {
                // only recreate on actual rename
                let group_ref: Vec<&str> = self.group.iter().map(|e| e.as_ref()).collect();
                let ri = RouteInstance::create(&label.name, &group_ref)?;
                st.route_instances[i] = ri;
                st.output_labels[i].name = label.name.clone();
                actually_changed = true;
            }
        }
        if actually_changed {
            let _ = self
                .tx
                .send(RouterEvent::OutputLabelUpdate(0, st.output_labels.clone()));
        }
        Ok(())
    }

    async fn get_routes(&self, index: u32) -> Result<Vec<RouterPatch>> {
        Self::assert_matrix_zero(index)?;
        Ok(self.state.lock().unwrap().routes.clone())
    }

    async fn update_routes(&self, index: u32, changes: Vec<RouterPatch>) -> Result<()> {
        Self::assert_matrix_zero(index)?;
        let mut st = self.state.lock().unwrap();
        let mut actually_changed = false;

        for p in changes {
            let output = p.to_output;
            let input = p.from_input;
            if output as usize >= st.routes.len() || input >= st.matrix_info.input_count {
                return Err(anyhow!("Patch {:?} out of bounds", p));
            }
            Self::patch_output(&mut st, output, input)?;
            actually_changed = true;
        }

        if actually_changed {
            let _ = self.tx.send(RouterEvent::RouteUpdate(0, st.routes.clone()));
        }
        Ok(())
    }

    async fn event_stream<'a>(&'a self) -> Result<BoxStream<'a, RouterEvent>> {
        let bs = BroadcastStream::new(self.tx.subscribe());
        let filtered = bs.filter_map(|r| r.ok());
        Ok(futures_util::StreamExt::boxed(filtered))
    }
}
