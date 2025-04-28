#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RouterInfo {
    pub model: Option<String>,
    pub name: Option<String>,
    pub matrix_count: Option<u32>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RouterMatrixInfo {
    pub input_count: u32,
    pub output_count: u32,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RouterLabel {
    pub id: u32,
    pub name: String,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct RouterPatch {
    pub from_input: u32,
    pub to_output: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RouterEvent {
    Connected,
    Disconnected,

    InfoUpdate(RouterInfo),
    MatrixInfoUpdate(u32, RouterMatrixInfo),
    InputLabelUpdate(u32, Vec<RouterLabel>),
    OutputLabelUpdate(u32, Vec<RouterLabel>),
    RouteUpdate(u32, Vec<RouterPatch>),
}

impl From<videohub::Label> for RouterLabel {
    fn from(item: videohub::Label) -> Self {
        Self {
            id: item.id,
            name: item.name,
        }
    }
}
impl Into<videohub::Label> for RouterLabel {
    fn into(self) -> videohub::Label {
        videohub::Label {
            id: self.id,
            name: self.name,
        }
    }
}

impl From<videohub::Route> for RouterPatch {
    fn from(item: videohub::Route) -> Self {
        Self {
            from_input: item.from_input,
            to_output: item.to_output,
        }
    }
}
impl Into<videohub::Route> for RouterPatch {
    fn into(self) -> videohub::Route {
        videohub::Route {
            from_input: self.from_input,
            to_output: self.to_output,
        }
    }
}
