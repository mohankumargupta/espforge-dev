//! Network services: outgoing clients over an embassy-net stack.
//!
//! Distinct from `hw`: nothing here names a pin or a peripheral. A service
//! takes the [`Stack`](embassy_net::Stack) a component (`<wifi_station>`)
//! exposes and speaks a protocol over it.
//!
//! Compiled only behind the `http-client`/`https-client` features, so a project
//! with no service never pulls in `edge-http` or `embedded-tls`.

mod http_client;
pub use http_client::*;

#[cfg(feature = "https-client")]
mod https_client;
#[cfg(feature = "https-client")]
pub use https_client::*;
