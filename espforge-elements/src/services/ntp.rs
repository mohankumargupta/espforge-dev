//! `<ntp>`: an outgoing SNTP client over a network component.
//!
//! A service, so it claims no pin: it binds to a component (`network="$home"`,
//! a `<wifi_station>`) and reaches the embassy-net stack through it. Queries
//! are made from the app on demand; nothing runs in the background.

use espf::context::ParsedNode;
use espf::diag::node_span;
use espforge_diagnostics::codes;
use linkme::distributed_slice;
use quote::{format_ident, quote};

use crate::attrs::Issue;
use crate::element::{Element, EmitCx, LowerCx, Lowered, Reported, is_rust_ident};
use crate::generated::ntp::Attrs;
use crate::model::{Board, Ident, Runtime, Service};
use crate::registry::ELEMENTS;
use crate::services::{invalid_element, invalid_id, network_stack, span_of, unknown_network};

/// The largest UDP port number.
const MAX_PORT: u32 = 65535;

/// The `<ntp>` element.
pub struct NtpElement;

#[distributed_slice(ELEMENTS)]
static NTP: &dyn Element = &NtpElement;

impl Element for NtpElement {
    fn tag(&self) -> &'static str {
        "ntp"
    }

    fn lower(&self, node: &ParsedNode<'_>, cx: &mut LowerCx) -> Result<(), Reported> {
        let attrs = Attrs::from_node(node).map_err(|issue| describe(cx, node, issue))?;
        let id = cx.ident(node);
        let network = Ident::new(attrs.network.name, span_of(node, "network"));

        if !is_rust_ident(&id.rust_name()) {
            return Err(invalid_id(cx, node, "ntp", &id));
        }
        if cx.runtime() != Runtime::Embassy {
            return Err(cx.report(
                codes::SERVICE_REQUIRES_EMBASSY,
                node_span(node),
                "NTP client requires embassy",
                format!(
                    "<ntp id=\"{}\"> needs the embassy runtime; change \
                     <runtime type=\"..\" /> to embassy",
                    id.raw
                ),
            ));
        }
        if !cx.has_network_provider(network.as_str()) {
            return Err(unknown_network(cx, node, "ntp", &id, &network));
        }

        if attrs.server.value.is_empty() {
            return Err(cx.report(
                codes::SERVICE_INVALID,
                span_of(node, "server"),
                "invalid ntp server",
                format!(
                    "<ntp id=\"{}\"> server=\"\" must name a host or an IP address",
                    id.raw
                ),
            ));
        }

        let port = attrs.port.value;
        if port == 0 || port > MAX_PORT {
            return Err(cx.report(
                codes::SERVICE_INVALID,
                span_of(node, "port"),
                "invalid ntp port",
                format!(
                    "<ntp id=\"{}\"> port=\"{port}\" must be in 1..={MAX_PORT}",
                    id.raw
                ),
            ));
        }

        cx.push_service(Service::Ntp(NtpService {
            id,
            network,
            server: attrs.server.value.to_string(),
            port: port as u16,
            timeout_ms: attrs.timeout_ms.value,
        }));
        Ok(())
    }
}

/// One lowered `<ntp>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NtpService {
    pub id: Ident,
    /// The `<wifi_station>` (or other network component) it rides on.
    pub network: Ident,
    /// Host name or IP address of the server it queries by default.
    pub server: String,
    pub port: u16,
    pub timeout_ms: u32,
}

impl Lowered for NtpService {
    fn id(&self) -> &Ident {
        &self.id
    }

    fn emit(&self, _board: &Board, cx: &mut EmitCx) {
        let name = format_ident!("{}", self.id.rust_name());
        let stack = network_stack(&self.network);
        let server = &self.server;
        let port = self.port;
        let timeout = self.timeout_ms;

        cx.service_field(quote! {
            pub #name: espforge_runtime::NtpClient,
        });
        cx.service_init(quote! {
            #name: espforge_runtime::NtpClient::new(
                #stack,
                espforge_runtime::NtpConfig {
                    server: #server,
                    port: #port,
                    timeout_ms: #timeout as u64,
                },
            ),
        });
    }
}

/// This element's wording for a schema-level problem.
fn describe(cx: &mut LowerCx, node: &ParsedNode<'_>, issue: Issue<'_>) -> Reported {
    invalid_element(cx, node, "ntp", issue)
}
