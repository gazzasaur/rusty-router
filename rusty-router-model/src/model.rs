use std::collections::HashMap;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Router {
    network_interfaces: HashMap<String, NetworkInterface>,
    vrf: HashMap<String, Vrf>,
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
    priority: HashMap<RouteSource, u64>,
    static_routes: Vec<StaticRoute>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum VrfTable {
    Base,
    VirtualTable(u64),
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Hash)]
pub enum RouteSource {
    Static,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct StaticRoute {
    prefix: IpPrefix,
    next_hop: NextHop,
    metric: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum IpPrefix {
    IpV4(String, u64),
    IpV6(String, u64),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum NextHop {
    IpV4(String),
    IpV6(String),
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
            vrf: HashMap::from_iter(vec![("Blue".to_string(), Vrf {
                vrf_type: VrfTable::VirtualTable(10),
                priority: HashMap::from_iter(vec![(RouteSource::Static, 10)].drain(..)),
                static_routes: vec![StaticRoute {
                    prefix: IpPrefix::IpV4("172.0.0.0".to_string(), 16),
                    next_hop: NextHop::IpV4("10.10.10.10".to_string()),
                    metric: 100,
                }],
            })].drain(..)),
        };
        assert_eq!("{\"network_interfaces\":{\"red1\":{\"device\":\"eth0\",\"network_interface_type\":\"GenericInterface\"}},\"vrf\":{\"Blue\":{\"vrf_type\":{\"VirtualTable\":10},\"priority\":{\"Static\":10},\"static_routes\":[{\"prefix\":{\"IpV4\":[\"172.0.0.0\",16]},\"next_hop\":{\"IpV4\":\"10.10.10.10\"},\"metric\":100}]}}}", serde_json::to_string(&config).unwrap());
    }
}
