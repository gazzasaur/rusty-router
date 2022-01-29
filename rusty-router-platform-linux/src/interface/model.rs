use tokio::time::Instant;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct CanonicalNetworkId {
    id: Option<u64>,
    name: Option<String>,
    device: Option<String>,
}
impl CanonicalNetworkId {
    pub fn new(id: Option<u64>, name: Option<String>, device: Option<String>) -> Self {
        Self { id, name, device }
    }

    pub fn id(&self) -> Option<u64> {
        self.id
    }

    pub fn name(&self) -> Option<&String> {
        self.name.as_ref()
    }

    pub fn device(&self) -> Option<&String> {
        self.device.as_ref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkStatusItem<T> {
    id: CanonicalNetworkId,
    status: T,
    refreshed: Instant,
}
impl<T> NetworkStatusItem<T> {
    pub fn new(id: CanonicalNetworkId, status: T) -> NetworkStatusItem<T> {
        NetworkStatusItem { id, refreshed: Instant::now(), status }
    }

    #[cfg(test)]
    pub fn new_with_refresh(id: CanonicalNetworkId, status: T, refreshed: Instant) -> NetworkStatusItem<T> {
        NetworkStatusItem { id, status, refreshed }
    }

    pub fn get_refreshed(&self) -> &Instant {
        &self.refreshed
    }

    pub fn get_id(&self) -> &CanonicalNetworkId {
        &self.id
    }

    pub fn get_status(&self) -> &T {
        &self.status
    }
}
