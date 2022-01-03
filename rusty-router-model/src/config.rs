use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Router {
    network_links: HashMap<String, NetworkLink>,
    network_interfaces: HashMap<String, NetworkInterface>,
    vrfs: HashMap<String, Vrf>,
}
impl Router {
    pub fn new(network_links: HashMap<String, NetworkLink>, network_interfaces: HashMap<String, NetworkInterface>, vrfs: HashMap<String, Vrf>) -> Router {
        Router { network_links, network_interfaces, vrfs }
    }

    pub fn get_network_links(&self) -> &HashMap<String, NetworkLink> {
        &self.network_links
    }

    pub fn get_network_interfaces(&self) -> &HashMap<String, NetworkInterface> {
        &self.network_interfaces
    }

    pub fn get_vrfs(&self) -> &HashMap<String, Vrf> {
        &self.vrfs
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NetworkLink {
    device: String,
    network_link_type: NetworkLinkType,
}
impl NetworkLink {
    pub fn new(device: String, network_link_type: NetworkLinkType) -> NetworkLink {
        NetworkLink { device, network_link_type }
    }

    pub fn get_device(&self) -> &String {
        &self.device
    }

    pub fn get_network_link_type(&self) -> &NetworkLinkType {
        &self.network_link_type
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Hash)]
pub enum NetworkLinkType {
    GenericInterface,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Vrf {
    table: VrfTable,
    static_routes: Vec<StaticRoute>,
    priority: HashMap<RouteSource, u64>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum VrfTable {
    Base,
    VirtualTable(u64),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NetworkInterface {
    vrf: Option<String>,
    network_link: String,
    ip_addresses: Vec<IpAddress>,
}
impl NetworkInterface {
    pub fn new(vrf: Option<String>, network_link: String, ip_addresses: Vec<IpAddress>) -> NetworkInterface {
        NetworkInterface { vrf, network_link, ip_addresses }
    }

    pub fn get_vrf(&self) -> &Option<String> {
        &self.vrf
    }

    pub fn get_network_link(&self) -> &String {
        &self.network_link
    }

    pub fn get_ip_addresses(&self) -> &Vec<IpAddress> {
        &self.ip_addresses
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Hash)]
pub enum RouteSource {
    Static,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct StaticRoute {
    prefix: IpAddress,
    next_hop: String,
    metric: u64,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct IpAddress (pub IpAddressType, pub String, pub u64);
impl IpAddress {
    pub fn new(family: IpAddressType, address: String, prefix: u64) -> IpAddress {
        IpAddress(family, address, prefix)
    }
}
impl Display for IpAddress {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{}/{}", self.1, self.2)
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub enum IpAddressType {
    IpV4,
    IpV6,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::iter::FromIterator;

    #[test]
    fn it_parses_config() {
        let config = Router {
            network_links: HashMap::from_iter(vec![("red1".to_string(), NetworkLink {
                device: "eth0".to_string(),
                network_link_type: NetworkLinkType::GenericInterface,
            })]),
            network_interfaces: HashMap::from_iter(vec![("BlueInterface".to_string(), NetworkInterface {
                vrf: None,
                network_link: "lo".to_string(),
                ip_addresses: vec![IpAddress(IpAddressType::IpV4, "192.168.0.1".to_string(), 32)],
            })]),
        vrfs: HashMap::from_iter(vec![("Blue".to_string(), Vrf {
                table: VrfTable::VirtualTable(10),
                static_routes: vec![StaticRoute {
                    prefix: IpAddress(IpAddressType::IpV4, "172.0.0.0".to_string(), 16),
                    next_hop: "10.10.10.10".to_string(),
                    metric: 100,
                }],
                priority: HashMap::from_iter(vec![(RouteSource::Static, 10)].drain(..)),
            })].drain(..)),
        };
        assert_eq!("{\"network_links\":{\"red1\":{\"device\":\"eth0\",\"network_link_type\":\"GenericInterface\"}},\"network_interfaces\":{\"BlueInterface\":{\"vrf\":null,\"network_link\":\"lo\",\"ip_addresses\":[[\"IpV4\",\"192.168.0.1\",32]]}},\"vrfs\":{\"Blue\":{\"table\":{\"VirtualTable\":10},\"static_routes\":[{\"prefix\":[\"IpV4\",\"172.0.0.0\",16],\"next_hop\":\"10.10.10.10\",\"metric\":100}],\"priority\":{\"Static\":10}}}}", serde_json::to_string(&config).unwrap());
    }
}
