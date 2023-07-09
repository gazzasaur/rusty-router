use async_trait::async_trait;
use env_logger;
use log::{error, warn};
use rusty_router_model::{
    InetPacketNetworkInterface, NetworkEventHandler, RustyRouter, RustyRouterInstance,
};
use rusty_router_proto_common::prelude::ProtocolError;
use rusty_router_proto_ip::{IpV4Header, Ipv4Netmask};
use rusty_router_proto_ospfv2::constants::OSPF_PROTOCOL_NUMBER;
use rusty_router_proto_ospfv2::packet::OspfHelloPacketBuilder;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::Arc;
use std::{error::Error, net::Ipv4Addr};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::Mutex;

struct Ni {
    local_address: Ipv4Addr,
    receiver: Receiver<Vec<u8>>,
    connection: Box<dyn InetPacketNetworkInterface + Send + Sync>,
}
impl Ni {
    pub async fn new(
        instance: Box<dyn RustyRouterInstance>,
        interface_name: &String,
        local_address: Ipv4Addr,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let (sender, receiver) = tokio::sync::mpsc::channel(100);
        let nih = Nih {
            sender: Arc::new(Mutex::new(Some(sender))),
        };
        let connection = instance
            .connect_ipv4(
                interface_name,
                local_address,
                OSPF_PROTOCOL_NUMBER,
                vec!["224.0.0.5".parse()?],
                Box::from(nih),
            )
            .await?;
        Ok(Self {
            local_address,
            receiver,
            connection,
        })
    }

    pub async fn process(&mut self) {
        let _: Result<(), Box<dyn Error>> = async {
            let packet = if let Some(packet) = self.receiver.recv().await {
                packet
            } else {
                return Ok(());
            };

            let ip = IpV4Header::try_from(&packet[..])?;
            if ip.get_source_address() == self.local_address {
                return Ok(());
            }

            let response = OspfHelloPacketBuilder::new(
                "2.2.2.2".parse()?,
                "0.0.0.0".parse()?,
                Ipv4Netmask::new(4294967292),
                10,
                2,
                5,
                40,
                "172.16.10.1".parse()?,
                "0.0.0.0".parse()?,
                vec!["1.1.1.1".parse()?],
            )
            .build()?;

            self.connection
                .send(
                    "224.0.0.5".parse()?,
                    &Result::<Vec<u8>, ProtocolError>::from(&response)?,
                )
                .await?;
            Ok(())
        }
        .await;
    }
}

struct Nih {
    sender: Arc<Mutex<Option<Sender<Vec<u8>>>>>,
}
#[async_trait]
impl NetworkEventHandler for Nih {
    async fn on_recv(&self, data: Vec<u8>) {
        self.sender
            .lock()
            .await
            .as_mut()
            .filter(|sender| sender.try_send(data).is_ok());
    }

    async fn on_error(&self, message: String) {
        error!("Error {:?}", message);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    env_logger::init();

    // Check the effective set for required capabilitites.
    // Do not attempt to raise these as it must be done per thread, which we are not going to do in the tokio runtime.
    // TODO Manage Tokio runtimes so capabilities may be reliably raised or dropped.
    let required_capabilities: Vec<caps::Capability> = vec![
        caps::Capability::CAP_NET_RAW,
        caps::Capability::CAP_NET_ADMIN,
    ];

    let permitted_capabilities = caps::read(None, caps::CapSet::Effective)?;
    for capability in &required_capabilities {
        if !permitted_capabilities.contains(&capability) {
            error!(
                "Required capability must be raised in effective set at launch: {:?}",
                capability
            );
            return Ok(());
        }
    }

    // As we cannot reliably drop anything on all threads, we will warn if we find capabilities we do not need.
    let permitted_capabilities = caps::read(None, caps::CapSet::Permitted)?;
    for capability in permitted_capabilities {
        if !&required_capabilities.contains(&capability) {
            warn!(
                "Detected a capability that is not required: {:?}",
                capability
            );
        }
    }

    let config = rusty_router_model::Router::new(
        vec![
            (
                "iface0".to_string(),
                rusty_router_model::NetworkLink::new(
                    "vlan.10".to_string(),
                    rusty_router_model::NetworkLinkType::GenericInterface,
                ),
            ),
            (
                "iface1".to_string(),
                rusty_router_model::NetworkLink::new(
                    "eth1".to_string(),
                    rusty_router_model::NetworkLinkType::GenericInterface,
                ),
            ),
            (
                "iface2".to_string(),
                rusty_router_model::NetworkLink::new(
                    "dummy0".to_string(),
                    rusty_router_model::NetworkLinkType::GenericInterface,
                ),
            ),
            (
                "iface3".to_string(),
                rusty_router_model::NetworkLink::new(
                    "lo".to_string(),
                    rusty_router_model::NetworkLinkType::GenericInterface,
                ),
            ),
        ]
        .into_iter()
        .collect(),
        vec![
            (
                "Inside".to_string(),
                rusty_router_model::NetworkInterface::new(None, "iface0".to_string(), vec![]),
            ),
            (
                "Outside".to_string(),
                rusty_router_model::NetworkInterface::new(None, "iface1".to_string(), vec![]),
            ),
            (
                "Unused".to_string(),
                rusty_router_model::NetworkInterface::new(None, "doesnotexist".to_string(), vec![]),
            ),
            (
                "Loopback".to_string(),
                rusty_router_model::NetworkInterface::new(None, "iface3".to_string(), vec![]),
            ),
        ]
        .into_iter()
        .collect(),
        HashMap::new(),
    );

    let socket_factory = rusty_router_platform_linux::netlink::DefaultNetlinkSocketFactory::new();
    let nlp = rusty_router_platform_linux::LinuxRustyRouter::new(config, Arc::new(socket_factory))
        .await?;
    let nl = nlp.fetch_instance().await?;

    if let Ok(mut interfaces) = nl.list_network_links().await {
        interfaces.sort_by(|a, b| {
            if let Some(a_name) = a.get_name() {
                if let Some(b_name) = b.get_name() {
                    return a_name.cmp(b_name);
                }
            }
            a.get_device().cmp(&b.get_device())
        });

        println!(
            "================================================================================"
        );
        println!("Mapped Link");
        println!(
            "================================================================================"
        );
        interfaces.iter().for_each(|interface| {
            if let Some(name) = interface.get_name() {
                println!(
                    "{}\t{}\t{}",
                    name,
                    interface.get_device(),
                    interface.get_operational_state()
                );
            }
        });
        println!();

        println!(
            "================================================================================"
        );
        println!("Available Devices");
        println!(
            "================================================================================"
        );
        interfaces.iter().for_each(|interface| {
            if let None = interface.get_name() {
                println!(
                    "{}\t{}",
                    interface.get_device(),
                    interface.get_operational_state()
                );
            }
        });
        println!();
    }

    if let Ok(addresses) = nl.list_network_interfaces().await {
        println!(
            "================================================================================"
        );
        println!("Mapped Router Interfaces");
        println!(
            "================================================================================"
        );
        addresses.iter().for_each(|address| {
            if let Some(name) = address.get_name() {
                if address.get_network_link_status().get_operational_state()
                    != &rusty_router_model::NetworkLinkOperationalState::NotFound
                {
                    println!(
                        "{} ({})",
                        name,
                        address.get_network_link_status().get_device()
                    );
                    for addr in address.get_addresses() {
                        println!("\t{}", addr);
                    }
                }
            }
        });
        println!();

        println!(
            "================================================================================"
        );
        println!("Missing Network Interfaces");
        println!(
            "================================================================================"
        );
        addresses.iter().for_each(|interface| {
            if interface.get_network_link_status().get_operational_state()
                == &rusty_router_model::NetworkLinkOperationalState::NotFound
            {
                if let Some(name) = interface.get_name() {
                    println!(
                        "{} ({})",
                        name,
                        interface.get_network_link_status().get_device()
                    );
                }
            }
        });
        println!();

        println!(
            "================================================================================"
        );
        println!("Unmapped Interface");
        println!(
            "================================================================================"
        );
        addresses.iter().for_each(|interface| {
            if interface.get_name().is_none() {
                println!("{}", interface.get_network_link_status().get_device());
                for addr in interface.get_addresses() {
                    println!("\t{}", addr);
                }
            }
        });
        println!();
    };

    let (sender, mut _receiver) = tokio::sync::mpsc::channel(100);
    nl.subscribe(sender).await;

    let mut n = Ni::new(
        nlp.fetch_instance().await?,
        &"Inside".into(),
        "172.16.10.2".parse()?,
    )
    .await?;

    loop {
        n.process().await;
        // if let Some(data) = receiver.recv().await {
        //     println!("{:?}", data);
        // }
    }
}
