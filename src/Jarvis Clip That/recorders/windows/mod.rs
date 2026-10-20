use serde::{Deserialize, Serialize};

pub(crate) mod d3d11;
pub(crate) mod wasapi;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VideoEnvironment {
    D3D11 { monitor: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "config", rename_all = "snake_case")]
pub enum AudioEnvironment {
    WasApi(WasAPI),
}


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WasAPI {
    WasApiDefaultSys,
    WasApiProcess { process_id: u32, include_tree: bool },
    WasApiDefaultInput,
}