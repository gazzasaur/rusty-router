use std::collections::HashMap;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Router {
    vrfs: HashMap<String, Vrf>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Vrf {
    priority: HashMap<RouteSource, u64>,
    static_routes: Vec<StaticRoute>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Hash)]
pub enum RouteSource {
    STATIC,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct StaticRoute {
    prefix: IpPrefix,
    next_hop: NetworkInterface,
    metric: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum IpPrefix {
    IPV4(String, u64),
    IPV6(String, u64),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum NetworkInterface {
    IPV4(String),
    IPV6(String),
    SERIAL(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::iter::FromIterator;

    #[test]
    fn it_parses_config() {
        let config = Router {
            vrfs: HashMap::from_iter(vec![("base".to_string(), Vrf {
                priority: HashMap::from_iter(vec![(RouteSource::STATIC, 10)].drain(..)),
                static_routes: vec![StaticRoute {
                    prefix: IpPrefix::IPV4("172.0.0.0".to_string(), 16),
                    next_hop: NetworkInterface::SERIAL("10".to_string()),
                    metric: 100,
                }],
            })].drain(..)),
        };
        assert_eq!("{\"vrfs\":{\"base\":{\"priority\":{\"STATIC\":10},\"static_routes\":[{\"prefix\":{\"IPV4\":[\"172.0.0.0\",16]},\"next_hop\":{\"SERIAL\":\"10\"},\"metric\":100}]}}}", serde_json::to_string(&config).unwrap());
    }
}
