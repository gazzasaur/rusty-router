use std::fmt::{Display, Formatter};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct NetworkInterfaceStatus {
    device: String,
    device_binding: NetworkDeviceBinding,
    interface_binding: NetworkInterfaceBinding,
    operational_state: NetworkInterfaceOperationalState,
}
impl NetworkInterfaceStatus {
    pub fn new(device: String, device_binding: NetworkDeviceBinding, interface_binding: NetworkInterfaceBinding, operational_state: NetworkInterfaceOperationalState) -> NetworkInterfaceStatus {
        NetworkInterfaceStatus {
            device,
            device_binding,
            interface_binding,
            operational_state,
        }
    }

    pub fn get_device(&self) -> &String {
        &self.device
    }

    pub fn get_device_binding(&self) -> &NetworkDeviceBinding {
        &self.device_binding
    }

    pub fn get_interface_binding(&self) -> &NetworkInterfaceBinding {
        &self.interface_binding
    }

    pub fn get_operational_state(&self) -> &NetworkInterfaceOperationalState {
        &self.operational_state
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum NetworkDeviceBinding {
    Bound,
    Unbound,
}
impl Display for NetworkDeviceBinding {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match &self {
            NetworkDeviceBinding::Unbound => write!(f, "Unbound"),
            NetworkDeviceBinding::Bound => write!(f, "Bound"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum NetworkInterfaceBinding {
    Unbound,
    Bound(String),
}
impl Display for NetworkInterfaceBinding {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match &self {
            NetworkInterfaceBinding::Unbound => write!(f, "Unbound"),
            // Only display Bound.  A developer can pull the contained value to display.
            NetworkInterfaceBinding::Bound(_) => write!(f, "Bound"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum NetworkInterfaceOperationalState {
    Up,
    Down,
    Unknown,
}
impl Display for NetworkInterfaceOperationalState {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        match &self {
            NetworkInterfaceOperationalState::Up => write!(f, "Up"),
            NetworkInterfaceOperationalState::Down => write!(f, "Down"),
            NetworkInterfaceOperationalState::Unknown => write!(f, "Unknown"),
        }
    }
}