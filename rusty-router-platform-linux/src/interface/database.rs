use std::collections::HashMap;

use rusty_router_model::{NetworkInterfaceStatus, NetworkLinkStatus};

use super::model::{CanonicalNetworkId, NetworkStatusItem};

pub struct InterfaceManagerDatabase {
    network_link_index: u64,
    network_links_by_name: HashMap<String, u64>,
    network_links_by_device_index: HashMap<u64, u64>,
    network_links_by_device_name: HashMap<String, u64>,
    network_links: HashMap<u64, NetworkStatusItem<NetworkLinkStatus>>,

    network_interface_index: u64,
    network_interfaces_by_name: HashMap<String, u64>,
    network_interfaces_by_device_index: HashMap<u64, u64>,
    network_interfaces_by_device_name: HashMap<String, u64>,
    network_interfaces: HashMap<u64, NetworkStatusItem<NetworkInterfaceStatus>>,
}
impl InterfaceManagerDatabase {
    pub fn new() -> InterfaceManagerDatabase {
        InterfaceManagerDatabase {
            network_link_index: 0,
            network_links_by_name: HashMap::new(),
            network_links_by_device_index: HashMap::new(),
            network_links_by_device_name: HashMap::new(),
            network_links: HashMap::new(),

            network_interface_index: 0,
            network_interfaces_by_name: HashMap::new(),
            network_interfaces_by_device_index: HashMap::new(),
            network_interfaces_by_device_name: HashMap::new(),
            network_interfaces: HashMap::new(),
        }
    }

    pub fn list_link_status(&self) -> Vec<NetworkLinkStatus> {
        let mut data: Vec<NetworkLinkStatus> = self
            .network_links
            .iter()
            .map(|(_, value)| value.get_status().clone())
            .collect();
        data.sort_by(|first, second| first.get_name().cmp(second.get_name()));
        data
    }

    pub fn take_link_status_items(&mut self) -> Vec<NetworkStatusItem<NetworkLinkStatus>> {
        self.network_link_index = 0;
        self.network_links_by_name.clear();
        self.network_links_by_device_name.clear();
        self.network_links_by_device_index.clear();
        self.network_links.drain().map(|(_, item)| item).collect()
    }

    pub fn set_link_status_item(
        &mut self,
        link: NetworkStatusItem<NetworkLinkStatus>,
    ) -> Vec<NetworkStatusItem<NetworkLinkStatus>> {
        let mut deleted_items = Vec::new();
        self.remove_link_status_item(link.get_id(), &mut deleted_items);

        let index = self.network_link_index;
        self.network_link_index += 1;

        link.get_id()
            .name()
            .and_then(|name| self.network_links_by_name.insert(name.clone(), index));
        link.get_id().id().and_then(|device_index| {
            self.network_links_by_device_index
                .insert(device_index, index)
        });
        link.get_id().device().and_then(|device_name| {
            self.network_links_by_device_name
                .insert(device_name.clone(), index)
        });
        self.network_links.insert(index, link);
        deleted_items
    }

    pub fn list_interface_status(&self) -> Vec<NetworkInterfaceStatus> {
        let mut data: Vec<NetworkInterfaceStatus> = self
            .network_interfaces
            .iter()
            .map(|(_, value)| value.get_status().clone())
            .collect();
        data.sort_by(|first, second| first.get_name().cmp(second.get_name()));
        data
    }

    pub fn get_interface_status_item_by_device_index(
        &self,
        device_index: &u64,
    ) -> Option<&NetworkStatusItem<NetworkInterfaceStatus>> {
        self.network_interfaces_by_device_index
            .get(device_index)
            .and_then(|index| self.network_interfaces.get(index))
    }

    pub fn take_interface_status_items(
        &mut self,
    ) -> Vec<NetworkStatusItem<NetworkInterfaceStatus>> {
        self.network_interface_index = 0;
        self.network_interfaces_by_name.clear();
        self.network_interfaces_by_device_name.clear();
        self.network_interfaces_by_device_index.clear();
        self.network_interfaces
            .drain()
            .map(|(_, item)| item)
            .collect()
    }

    pub fn set_interface_status_item(
        &mut self,
        interface: NetworkStatusItem<NetworkInterfaceStatus>,
    ) -> Vec<NetworkStatusItem<NetworkInterfaceStatus>> {
        let mut deleted_items = Vec::new();
        self.remove_interface_status_item(interface.get_id(), &mut deleted_items);

        let index = self.network_interface_index;
        self.network_interface_index += 1;

        interface
            .get_id()
            .name()
            .and_then(|name| self.network_interfaces_by_name.insert(name.clone(), index));
        interface.get_id().device().and_then(|device_name| {
            self.network_interfaces_by_device_name
                .insert(device_name.clone(), index)
        });
        interface.get_id().id().and_then(|device_index| {
            self.network_interfaces_by_device_index
                .insert(device_index, index)
        });
        self.network_interfaces.insert(index, interface);
        deleted_items
    }

    pub fn remove_link_status_item(
        &mut self,
        id: &CanonicalNetworkId,
        deleted_items: &mut Vec<NetworkStatusItem<NetworkLinkStatus>>,
    ) {
        id.name()
            .and_then(|name| self.network_links_by_name.remove(name))
            .and_then(|index| self.network_links.remove(&index))
            .into_iter()
            .for_each(|item| {
                self.remove_link_status_item(item.get_id(), deleted_items);
                deleted_items.push(item);
            });
        id.device()
            .and_then(|name| self.network_links_by_device_name.remove(name))
            .and_then(|index| self.network_links.remove(&index))
            .into_iter()
            .for_each(|item| {
                self.remove_link_status_item(item.get_id(), deleted_items);
                deleted_items.push(item);
            });
        id.id()
            .and_then(|device_index| self.network_links_by_device_index.remove(&device_index))
            .and_then(|index| self.network_links.remove(&index))
            .into_iter()
            .for_each(|item| {
                self.remove_link_status_item(item.get_id(), deleted_items);
                deleted_items.push(item);
            });
    }

    pub fn remove_interface_status_item(
        &mut self,
        id: &CanonicalNetworkId,
        deleted_items: &mut Vec<NetworkStatusItem<NetworkInterfaceStatus>>,
    ) {
        id.name()
            .and_then(|name| self.network_interfaces_by_name.remove(name))
            .and_then(|index| self.network_interfaces.remove(&index))
            .into_iter()
            .for_each(|item| {
                self.remove_interface_status_item(item.get_id(), deleted_items);
                deleted_items.push(item);
            });
        id.device()
            .and_then(|name| self.network_interfaces_by_device_name.remove(name))
            .and_then(|index| self.network_interfaces.remove(&index))
            .into_iter()
            .for_each(|item| {
                self.remove_interface_status_item(item.get_id(), deleted_items);
                deleted_items.push(item);
            });
        id.id()
            .and_then(|device_index| {
                self.network_interfaces_by_device_index
                    .remove(&device_index)
            })
            .and_then(|index| self.network_interfaces.remove(&index))
            .into_iter()
            .for_each(|item| {
                self.remove_interface_status_item(item.get_id(), deleted_items);
                deleted_items.push(item);
            });
    }
}
