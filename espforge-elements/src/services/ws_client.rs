//! `<ws_client>`: an outgoing WebSocket client over a network component.
//!
//! A service, so it claims no pin: it binds to a component (`network="$home"`,
//! a `<wifi_station>`) and reaches the embassy-net stack through it. Unlike the
//! HTTP clients it has no background task either — a session is opened from the
//! app and held for as long as it wants.

use espf::context::ParsedNode;
use espf::diag::node_span;
use espforge_diagnostics::codes;
use linkme::distributed_slice;
use quote::{format_ident, quote};

use crate::attrs::Issue;
use crate::element::{Element, EmitCx, LowerCx, Lowered, Reported, is_rust_ident};
use crate::generated::ws_client::Attrs;
use crate::model::{Board, Ident, Runtime, Service};
use crate::registry::ELEMENTS;
use crate::services::{
    buffer_too_small, invalid_element, invalid_id, network_stack, span_of, unknown_network,
};

/// The minimum receive buffer a WebSocket handshake and frame fit in.
const MIN_RX_BUFFER: u32 = 512;

/// The `<ws_client>` element.
pub struct WsClientElement;

#[distributed_slice(ELEMENTS)]
static WS_CLIENT: &dyn Element = &WsClientElement;

impl Element for WsClientElement {
    fn tag(&self) -> &'static str {
        "ws_client"
    }

    fn lower(&self, node: &ParsedNode<'_>, cx: &mut LowerCx) -> Result<(), Reported> {
        let attrs = Attrs::from_node(node).map_err(|issue| describe(cx, node, issue))?;
        let id = cx.ident(node);
        let network = Ident::new(attrs.network.name, span_of(node, "network"));

        if !is_rust_ident(&id.rust_name()) {
            return Err(invalid_id(cx, node, "ws_client", &id));
        }
        if cx.runtime() != Runtime::Embassy {
            return Err(cx.report(
                codes::SERVICE_REQUIRES_EMBASSY,
                node_span(node),
                "WebSocket client requires embassy",
                format!(
                    "<ws_client id=\"{}\"> needs the embassy runtime; change \
                     <runtime type=\"..\" /> to embassy",
                    id.raw
                ),
            ));
        }
        if !cx.has_network_provider(network.as_str()) {
            return Err(unknown_network(cx, node, "ws_client", &id, &network));
        }
        if attrs.rx_buffer_size.value < MIN_RX_BUFFER {
            return Err(buffer_too_small(
                cx,
                node,
                "ws_client",
                &id,
                "rx_buffer_size",
                attrs.rx_buffer_size.value,
                MIN_RX_BUFFER,
            ));
        }

        cx.push_service(Service::WsClient(WsClientService {
            id,
            network,
            rx_buffer_size: attrs.rx_buffer_size.value,
            tx_buffer_size: attrs.tx_buffer_size.value,
            timeout_ms: attrs.timeout_ms.value,
        }));
        Ok(())
    }
}

/// One lowered `<ws_client>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WsClientService {
    pub id: Ident,
    /// The `<wifi_station>` (or other network component) it rides on.
    pub network: Ident,
    pub rx_buffer_size: u32,
    pub tx_buffer_size: u32,
    pub timeout_ms: u32,
}

impl Lowered for WsClientService {
    fn id(&self) -> &Ident {
        &self.id
    }

    fn emit(&self, _board: &Board, cx: &mut EmitCx) {
        let name = format_ident!("{}", self.id.rust_name());
        let stack = network_stack(&self.network);
        let rx = self.rx_buffer_size;
        let tx = self.tx_buffer_size;
        let timeout = self.timeout_ms;

        cx.service_field(quote! {
            pub #name: espforge_runtime::WsClient,
        });
        cx.service_init(quote! {
            #name: espforge_runtime::WsClient::new(
                #stack,
                espforge_runtime::WsClientConfig {
                    rx_buffer_size: #rx as usize,
                    tx_buffer_size: #tx as usize,
                    timeout_ms: #timeout as u64,
                },
            ),
        });
    }
}

/// This element's wording for a schema-level problem.
fn describe(cx: &mut LowerCx, node: &ParsedNode<'_>, issue: Issue<'_>) -> Reported {
    invalid_element(cx, node, "ws_client", issue)
}
