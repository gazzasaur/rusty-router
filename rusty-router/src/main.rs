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
        }), ("iface2".to_string(), rusty_router_model::NetworkInterface {
            device: "dummy0".to_string(),
            network_interface_type: rusty_router_model::NetworkInterfaceType::GenericInterface,
        })].into_iter().collect(),
        router_interfaces: vec![("Inside".to_string(), rusty_router_model::RouterInterface {
            vrf: None,
            network_interface: "iface0".to_string(),
            ip_addresses: vec![],
        }), ("Outside".to_string(), rusty_router_model::RouterInterface {
            vrf: None,
            network_interface: "iface1".to_string(),
            ip_addresses: vec![],
        })].into_iter().collect(),
        vrfs: HashMap::new(),
    };
    let nl = rusty_router_netlink::NetlinkRustyRouter::new(config, Box::new(rusty_router_netlink::socket::DefaultNetlinkSocket::new().unwrap()));

    if let Ok(mut interfaces) = nl.list_network_interfaces() {
        interfaces.sort_by(|a, b| {
            if let Some(a_name) = a.get_name() {
                if let Some(b_name) = b.get_name() {
                    return a_name.cmp(b_name);
                }
            }
            a.get_device().cmp(&b.get_device())
        });

        println!("================================================================================");
        println!("Mapped Interfaces");
        println!("================================================================================");
        interfaces.iter().for_each(|interface| {
            if let Some(name) = interface.get_name() {
                println!("{}\t{}\t{}", name, interface.get_device(), interface.get_operational_state());
            }
        });
        println!();

        println!("================================================================================");
        println!("Available Devices");
        println!("================================================================================");
        interfaces.iter().for_each(|interface| {
            if let None = interface.get_name() {
                println!("{}\t{}", interface.get_device(), interface.get_operational_state());
            }
        });
        println!();
    }

    if let Ok(addresses) = nl.list_router_interfaces() {
        println!("================================================================================");
        println!("Mapped Router Interfaces");
        println!("================================================================================");
        addresses.iter().for_each(|address| {
            if let Some(name) = address.get_name() {
                println!("{}", name);
                for addr in address.get_addresses() {
                    println!("\t{}", addr);
                }
            }
        });
        println!();

        println!("================================================================================");
        println!("Unused Network Interfaces");
        println!("================================================================================");
        addresses.iter().for_each(|address| {
            if address.get_name().is_none() && !address.get_network_interface_status().get_name().is_none() {
                if let Some(name) = address.get_network_interface_status().get_name() { println!("{}", name) }
                for addr in address.get_addresses() {
                    println!("\t{}", addr);
                }
            }
        });
        println!();

        println!("================================================================================");
        println!("Unmapped Device");
        println!("================================================================================");
        addresses.iter().for_each(|address| {
            if address.get_name().is_none() && address.get_network_interface_status().get_name().is_none() {
                println!("{}", address.get_network_interface_status().get_device());
                for addr in address.get_addresses() {
                    println!("\t{}", addr);
                }
            }
        });
        println!();
    }
}
