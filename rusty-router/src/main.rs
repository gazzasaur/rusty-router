use serde_json;
use rusty_router_model::RustyRouter;

fn main() {
    let nl = rusty_router_netlink::NetlinkRustyRouter::new(Box::new(rusty_router_netlink::socket::DefaultNetlinkSocket::new().unwrap()));
    if let Ok(interfaces) = nl.list_network_interfaces() {
        if let Ok(output) = serde_json::to_string_pretty(&interfaces) {
            println!("{}", output);
        }
    }

    if let Ok(interfaces) = nl.list_router_interfaces() {
        if let Ok(output) = serde_json::to_string_pretty(&interfaces) {
            println!("{}", output);
        }
    }
}