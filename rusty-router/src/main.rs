use std::collections::HashMap;

use rusty_router_model::RustyRouter;

fn main() {
    let config = rusty_router_model::Router {
        network_interfaces: vec![("iface0".to_string(), rusty_router_model::NetworkInterface {
            device: "eth0".to_string(),
            network_interface_type: rusty_router_model::NetworkInterfaceType::GenericInterface,
        }), ("iface1".to_string(), rusty_router_model::NetworkInterface {
            device: "eth1".to_string(),
            network_interface_type: rusty_router_model::NetworkInterfaceType::GenericInterface,
        })].into_iter().collect(),
        vrfs: HashMap::new(),
    };
    let nl = rusty_router_netlink::NetlinkRustyRouter::new(config, Box::new(rusty_router_netlink::socket::DefaultNetlinkSocket::new().unwrap()));

    if let Ok(mut interfaces) = nl.list_network_interfaces() {
        interfaces.sort_by(|a, b| {
            if let rusty_router_model::NetworkInterfaceBinding::Bound(a_name) = &a.interface_binding {
                if let rusty_router_model::NetworkInterfaceBinding::Bound(b_name) = &b.interface_binding {
                    return a_name.cmp(b_name);
                }
            }
            a.device.cmp(&b.device)
        });

        println!("================================================================================");
        println!("Mapped Interfaces");
        println!("================================================================================");
        interfaces.iter().filter(|interface| interface.interface_binding != rusty_router_model::NetworkInterfaceBinding::Unbound).for_each(|interface| {
            if let rusty_router_model::NetworkInterfaceBinding::Bound(name) = &interface.interface_binding {
                println!("{}\n\t{}\t{}\t{}",
                    name,
                    interface.device,
                    if let rusty_router_model::NetworkDeviceBinding::Bound = interface.device_binding { "Bound" } else { "Unbound" },
                    interface.operational_state
                );
            }
        });
        println!();

        println!("================================================================================");
        println!("Available Devices");
        println!("================================================================================");
        interfaces.iter().filter(|interface| interface.interface_binding == rusty_router_model::NetworkInterfaceBinding::Unbound).for_each(|interface| {
            if let rusty_router_model::NetworkInterfaceBinding::Unbound = &interface.interface_binding {
                println!("{}\n\t{}\t{}",
                    interface.device,
                    if let rusty_router_model::NetworkDeviceBinding::Bound = interface.device_binding { "Bound" } else { "Unbound" },
                    interface.operational_state
                );
            }
        });
        println!();
    }
}
