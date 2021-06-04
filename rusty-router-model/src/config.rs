use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Router {
    network_interfaces: HashMap<String, NetworkInterface>,
    router_interfaces: HashMap<String, RouterInterface>,
    vrfs: HashMap<String, Vrf>,
}
impl Router {
    pub fn new(network_interfaces: HashMap<String, NetworkInterface>, router_interfaces: HashMap<String, RouterInterface>, vrfs: HashMap<String, Vrf>) -> Router {
        Router { network_interfaces, router_interfaces, vrfs }
    }

    pub fn get_network_interfaces(&self) -> &HashMap<String, NetworkInterface> {
        &self.network_interfaces
    }

    pub fn get_router_interfaces(&self) -> &HashMap<String, RouterInterface> {
        &self.router_interfaces
    }

    pub fn get_vrfs(&self) -> &HashMap<String, Vrf> {
        &self.vrfs
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NetworkInterface {
    device: String,
    network_interface_type: NetworkInterfaceType,
}
impl NetworkInterface {
    pub fn new(device: String, network_interface_type: NetworkInterfaceType) -> NetworkInterface {
        NetworkInterface { device, network_interface_type }
    }

    pub fn get_device(&self) -> &String {
        &self.device
    }

    pub fn get_network_interface_type(&self) -> &NetworkInterfaceType {
        &self.network_interface_type
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Hash)]
pub enum NetworkInterfaceType {
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
pub struct RouterInterface {
    pub vrf: Option<String>,
    pub network_interface: String,
    pub ip_addresses: Vec<IpAddress>,
}
impl RouterInterface {
    pub fn new(vrf: Option<String>, network_interface: String, ip_addresses: Vec<IpAddress>) -> RouterInterface {
        RouterInterface { vrf, network_interface, ip_addresses }
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

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
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

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
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
            network_interfaces: HashMap::from_iter(vec![("red1".to_string(), NetworkInterface {
                device: "eth0".to_string(),
                network_interface_type: NetworkInterfaceType::GenericInterface,
            })]),
            router_interfaces: HashMap::from_iter(vec![("BlueInterface".to_string(), RouterInterface {
                vrf: None,
                network_interface: "lo".to_string(),
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
        assert_eq!("{\"network_interfaces\":{\"red1\":{\"device\":\"eth0\",\"network_interface_type\":\"GenericInterface\"}},\"router_interfaces\":{\"BlueInterface\":{\"vrf\":null,\"network_interface\":\"lo\",\"ip_addresses\":[[\"IpV4\",\"192.168.0.1\",32]]}},\"vrfs\":{\"Blue\":{\"table\":{\"VirtualTable\":10},\"static_routes\":[{\"prefix\":[\"IpV4\",\"172.0.0.0\",16],\"next_hop\":\"10.10.10.10\",\"metric\":100}],\"priority\":{\"Static\":10}}}}", serde_json::to_string(&config).unwrap());
    }
}
