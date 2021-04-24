use std::collections::HashMap;

pub struct NetlinkRouterCache {
    inteface_name_to_index: HashMap<String, u64>,
    inteface_index_to_name: HashMap<u64, String>,
}

impl NetlinkRouterCache {
    pub fn new() -> NetlinkRouterCache {
        NetlinkRouterCache {
            inteface_name_to_index: HashMap::new(),
            inteface_index_to_name: HashMap::new(),
        }
    }

    pub fn update_interface_cache(&mut self, index: u64, name: String) {
        self.inteface_index_to_name.insert(index, name.clone());
        self.inteface_name_to_index.insert(name, index);
    }

    pub fn lookup_interface_name(&self, index: &u64) -> Option<&String> {
        self.inteface_index_to_name.get(index)
    }

    pub fn lookup_interface_index(&self, name: &String) -> Option<&u64> {
        self.inteface_name_to_index.get(name)
    }
}
