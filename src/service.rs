//! Transport-agnostic agent API over the headless core. Each tool is a
//! `Workspace` method taking a JSON-deserializable input and returning a
//! serializable result or a [`ServiceError`](error::ServiceError). The MCP
//! server in the desktop binary is a thin adapter over this module.
pub mod banding;
pub mod design;
pub mod dto;
pub mod error;
pub mod fittings;
pub mod hardware;
pub mod project;
pub mod stock;
pub mod vision;
pub mod workspace;

pub use error::{ErrorCode, ServiceError, ServiceResult};
pub use workspace::{Workspace, WorkspaceConfig};
