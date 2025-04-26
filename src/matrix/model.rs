#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RouterInfo {
    pub model: Option<String>,
    pub name: Option<String>,
    pub matrix_count: Option<u32>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RouterMatrixInfo {
    pub input_count: Option<u32>,
    pub output_count: Option<u32>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RouterLabel {
    pub id: u32,
    pub name: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RouterLabels {
    pub input: Vec<RouterLabel>,
    pub output: Vec<RouterLabel>,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct Patch {
    pub from_input: u32,
    pub to_output: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RouterEvent {
    Connected,
    Disconnected,

    InfoUpdate(RouterInfo),
    MatrixInfoUpdate(u32, RouterMatrixInfo),
    LabelUpdate(u32, RouterLabels),
    RouteUpdate(u32, Vec<Patch>),
}
