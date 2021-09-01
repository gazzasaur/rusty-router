use env_logger;
use log::{error, warn};
use std::error::Error;
use std::sync::Arc;
use std::collections::HashMap;

use rusty_router_model::RustyRouter;


#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    env_logger::init();
    
    // Check the effective set for required capabilitites.
    // Do not attempt to raise these as it must be done per thread, which we are not going to do in the tokio runtime.
    // TODO Manage Tokio runtimes so capabilities may be reliably raised or dropped.
    let required_capabilities: Vec<caps::Capability> = vec![caps::Capability::CAP_NET_RAW, caps::Capability::CAP_NET_ADMIN];

    let permitted_capabilities = caps::read(None, caps::CapSet::Effective)?;
    for capability in &required_capabilities {
        if !permitted_capabilities.contains(&capability) {
            error!("Required capability must be raised in effective set at launch: {:?}", capability);
            return Ok(());
        }
    }

    // As we cannot reliably drop anything on all threads, we will warn if we find capabilities we do not need.
    let permitted_capabilities = caps::read(None, caps::CapSet::Permitted)?;
    for capability in permitted_capabilities {
        if !&required_capabilities.contains(&capability) {
            warn!("Detected a capability that is not required: {:?}", capability);
        }
    }

    let config = rusty_router_model::Router::new(
        vec![("iface0".to_string(), rusty_router_model::NetworkInterface::new(
            "eth0".to_string(),
            rusty_router_model::NetworkInterfaceType::GenericInterface,
        )), ("iface1".to_string(), rusty_router_model::NetworkInterface::new(
            "eth1".to_string(),
            rusty_router_model::NetworkInterfaceType::GenericInterface,
        )), ("iface2".to_string(), rusty_router_model::NetworkInterface::new(
            "dummy0".to_string(),
            rusty_router_model::NetworkInterfaceType::GenericInterface,
        ))].into_iter().collect(),
        vec![("Inside".to_string(), rusty_router_model::RouterInterface::new(
            None,
            "iface0".to_string(),
            vec![],
        )), ("Outside".to_string(), rusty_router_model::RouterInterface::new(
            None,
            "iface1".to_string(),
            vec![],
        )), ("Unused".to_string(), rusty_router_model::RouterInterface::new(
            None,
            "doesnotexist".to_string(),
            vec![],
        ))].into_iter().collect(),
        HashMap::new(),
    );

    let socket = match rusty_router_platform_linux::netlink::DefaultNetlinkSocket::new() {
        Ok(socket) => socket,
        Err(_) => return Ok(()),
    };
    let nl = rusty_router_platform_linux::LinuxRustyRouter::new(config, Arc::new(socket))?;

    if let Ok(mut interfaces) = nl.list_network_interfaces().await {
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

    if let Ok(addresses) = nl.list_router_interfaces().await {
        println!("================================================================================");
        println!("Mapped Router Interfaces");
        println!("================================================================================");
        addresses.iter().for_each(|address| {
            if let Some(name) = address.get_name() {
                if address.get_network_interface_status().get_operational_state() != &rusty_router_model::NetworkInterfaceOperationalState::NotFound {
                    println!("{} ({})", name, address.get_network_interface_status().get_device());
                    for addr in address.get_addresses() {
                        println!("\t{}", addr);
                    }
                }
            }
        });
        println!();

        println!("================================================================================");
        println!("Missing Network Interfaces");
        println!("================================================================================");
        addresses.iter().for_each(|address| {
            if address.get_network_interface_status().get_operational_state() == &rusty_router_model::NetworkInterfaceOperationalState::NotFound {
                if let Some(name) = address.get_name() {
                    println!("{} ({})", name, address.get_network_interface_status().get_device());
                }
            }
        });
        println!();

        println!("================================================================================");
        println!("Unmapped Device");
        println!("================================================================================");
        addresses.iter().for_each(|address| {
            if address.get_name().is_none() {
                println!("{}", address.get_network_interface_status().get_device());
                for addr in address.get_addresses() {
                    println!("\t{}", addr);
                }
            }
        });
        println!();
    };

    Ok(())
    // rusty_router_platform_linux::socket::DefaultNetlinkSocket::new().unwrap().receive_messages(|_a| {
    //     println!("{:?}", _a);
    // }).await.unwrap();
}
