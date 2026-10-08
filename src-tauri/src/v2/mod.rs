//! Claude Proxy v2: new, testable gateway contracts.
//!
//! The legacy bridge remains the serving path while v2 contracts are proven in
//! CI and against a real Claude Desktop. Migrate one boundary at a time.
pub(crate) mod output_budget;
pub(crate) mod request_shape;
pub(crate) mod usage;
pub(crate) mod tool_stream;

pub(crate) use request_shape::RequestShape;
