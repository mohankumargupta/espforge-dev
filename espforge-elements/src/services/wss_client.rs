//! `<wss_client>`: an outgoing secure WebSocket client over a network
//! component.
//!
//! Like `<ws_client>`, but the socket is wrapped in TLS 1.3 (`embedded-tls`),
//! so the receive buffer has a higher floor: a TLS record envelope does not fit
//! in less than 4096 bytes. `insecure` is a build warning, not an error.

use espf::context::ParsedNode;
use espf::diag::node_span;
use espforge_diagnostics::codes;
use linkme::distributed_slice;
use quote::{format_ident, quote};

use crate::attrs::Issue;
use crate::element::{Element, EmitCx, LowerCx, Lowered, Reported, is_rust_ident};
use crate::generated::wss_client::Attrs;
use crate::model::{Board, Ident, Runtime, Service};
use crate::registry::ELEMENTS;
use crate::services::{
    buffer_too_small, invalid_element, invalid_id, network_stack, span_of, unknown_network,
};

/// TLS 1.3 records and cipher suites fail on undersized record envelopes.
const MIN_RX_BUFFER: u32 = 4096;

/// The `<wss_client>` element.
pub struct WssClientElement;

#[distributed_slice(ELEMENTS)]
static WSS_CLIENT: &dyn Element = &WssClientElement;

impl Element for WssClientElement {
    fn tag(&self) -> &'static str {
        "wss_client"
    }

    fn lower(&self, node: &ParsedNode<'_>, cx: &mut LowerCx) -> Result<(), Reported> {
        let attrs = Attrs::from_node(node).map_err(|issue| describe(cx, node, issue))?;
        let id = cx.ident(node);
        let network = Ident::new(attrs.network.name, span_of(node, "network"));

        if !is_rust_ident(&id.rust_name()) {
            return Err(invalid_id(cx, node, "wss_client", &id));
        }
        if cx.runtime() != Runtime::Embassy {
            return Err(cx.report(
                codes::SERVICE_REQUIRES_EMBASSY,
                node_span(node),
                "WebSocket client requires embassy",
                format!(
                    "<wss_client id=\"{}\"> needs the embassy runtime; change \
                     <runtime type=\"..\" /> to embassy",
                    id.raw
                ),
            ));
        }
        if !cx.has_network_provider(network.as_str()) {
            return Err(unknown_network(cx, node, "wss_client", &id, &network));
        }
        if attrs.rx_buffer_size.value < MIN_RX_BUFFER {
            return Err(buffer_too_small(
                cx,
                node,
                "wss_client",
                &id,
                "rx_buffer_size",
                attrs.rx_buffer_size.value,
                MIN_RX_BUFFER,
            ));
        }

        let insecure = attrs.insecure.value;
        if insecure {
            cx.warn(
                codes::SERVICE_INSECURE_WARNING,
                span_of(node, "insecure"),
                "insecure TLS verification enabled",
                "server certificates will not be validated",
            );
        }

        cx.push_service(Service::WssClient(WssClientService {
            id,
            network,
            rx_buffer_size: attrs.rx_buffer_size.value,
            tx_buffer_size: attrs.tx_buffer_size.value,
            timeout_ms: attrs.timeout_ms.value,
            insecure,
        }));
        Ok(())
    }
}

/// One lowered `<wss_client>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WssClientService {
    pub id: Ident,
    pub network: Ident,
    pub rx_buffer_size: u32,
    pub tx_buffer_size: u32,
    pub timeout_ms: u32,
    pub insecure: bool,
}

impl Lowered for WssClientService {
    fn id(&self) -> &Ident {
        &self.id
    }

    fn emit(&self, _board: &Board, cx: &mut EmitCx) {
        let name = format_ident!("{}", self.id.rust_name());
        let stack = network_stack(&self.network);
        let rx = self.rx_buffer_size;
        let tx = self.tx_buffer_size;
        let timeout = self.timeout_ms;
        let insecure = self.insecure;

        cx.service_field(quote! {
            pub #name: espforge_runtime::WssClient,
        });
        cx.service_init(quote! {
            #name: espforge_runtime::WssClient::new(
                #stack,
                espforge_runtime::WssClientConfig {
                    rx_buffer_size: #rx as usize,
                    tx_buffer_size: #tx as usize,
                    timeout_ms: #timeout as u64,
                },
                #insecure,
            ),
        });
    }
}

/// This element's wording for a schema-level problem.
fn describe(cx: &mut LowerCx, node: &ParsedNode<'_>, issue: Issue<'_>) -> Reported {
    invalid_element(cx, node, "wss_client", issue)
}
