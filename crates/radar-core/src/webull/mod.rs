//! Webull Thailand market-data layer: REST, MQTT streaming and contract
//! conventions. Modules are added phase by phase (see PLAN.md).

#[cfg(feature = "client")]
pub mod auth;
pub mod contracts;
pub mod discovery;
pub mod pb;
pub mod quotes;
pub mod rest;
pub mod sign;
#[cfg(feature = "stream")]
pub mod stream;
#[cfg(feature = "client")]
pub mod transport;
