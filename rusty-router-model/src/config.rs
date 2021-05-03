use std::collections::HashMap;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Router {
    pub network_interfaces: HashMap<String, NetworkInterface>,
    pub vrfs: HashMap<String, Vrf>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NetworkInterface {
    pub device: String,
    pub network_interface_type: NetworkInterfaceType,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Hash)]
pub enum NetworkInterfaceType {
    GenericInterface,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Vrf {
    vrf_type: VrfTable,
    router_interfaces: Vec<RouterInterface>,
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
    pub network_interface: String,
    pub ip_addresses: Vec<IpAddress>,
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

#[derive(Serialize, Deserialize, Debug)]
pub struct IpAddress (pub IpAddressType, pub String, pub u64);

#[derive(Serialize, Deserialize, Debug)]
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
            vrfs: HashMap::from_iter(vec![("Blue".to_string(), Vrf {
                vrf_type: VrfTable::VirtualTable(10),
                router_interfaces: vec![RouterInterface {
                    network_interface: "lo".to_string(),

                    ip_addresses: vec![IpAddress(IpAddressType::IpV4, "192.168.0.1".to_string(), 32)],
                }],
                static_routes: vec![StaticRoute {
                    prefix: IpAddress(IpAddressType::IpV4, "172.0.0.0".to_string(), 16),
                    next_hop: "10.10.10.10".to_string(),
                    metric: 100,
                }],
                priority: HashMap::from_iter(vec![(RouteSource::Static, 10)].drain(..)),
            })].drain(..)),
        };
        assert_eq!("{\"network_interfaces\":{\"red1\":{\"device\":\"eth0\",\"network_interface_type\":\"GenericInterface\"}},\"vrfs\":{\"Blue\":{\"vrf_type\":{\"VirtualTable\":10},\"router_interfaces\":[{\"network_interface\":\"lo\",\"ip_addresses\":[[\"IpV4\",\"192.168.0.1\",32]]}],\"static_routes\":[{\"prefix\":[\"IpV4\",\"172.0.0.0\",16],\"next_hop\":\"10.10.10.10\",\"metric\":100}],\"priority\":{\"Static\":10}}}}", serde_json::to_string(&config).unwrap());
    }
}
