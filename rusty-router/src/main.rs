use std::collections::HashMap;

use serde_json;
use rusty_router_model::RustyRouter;

fn main() {
    let config = rusty_router_model::Router {
        network_interfaces: vec![("Mine".to_string(), rusty_router_model::NetworkInterface {
            device: "eth0".to_string(),
            network_interface_type: rusty_router_model::NetworkInterfaceType::GenericInterface,
        })].into_iter().collect(),
        vrfs: HashMap::new(),
    };
    let nl = rusty_router_netlink::NetlinkRustyRouter::new(config, Box::new(rusty_router_netlink::socket::DefaultNetlinkSocket::new().unwrap()));

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
