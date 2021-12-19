use std::{sync::{Arc, RwLock}, collections::{HashMap, LinkedList}, iter::FromIterator};

use uuid::Uuid;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RegistrationManagerError {
    #[error("Failed to insert item")]
    RetryableFailure(),
}

pub struct Registration<T> {
    id: String,
    registrations: Arc<RwLock<HashMap<String, T>>>,
}
impl<T> Registration<T> {
    pub fn new(id: String, registrations: Arc<RwLock<HashMap<String, T>>>) -> Registration<T> {
        Registration { id, registrations }
    }
}
impl<T> Drop for Registration<T> {
    fn drop(&mut self) {
        let mut registrations = self.registrations.write().unwrap();
        registrations.remove(&self.id);
    }
}

/**
 * An atomic reference counting registration manager.
 * This trades some steady state performance and memory for avoiding blocking on get-action calls and avoiding lock poisoning.
 */
pub struct ArcRegistrationManager<T> {
    registrations: Arc<RwLock<HashMap<String, Arc<T>>>>,
}
impl<T> ArcRegistrationManager<T> {
    pub fn new() -> ArcRegistrationManager<T> {
        ArcRegistrationManager { registrations: Arc::new(RwLock::new(HashMap::new())) }
    }

    pub fn register(&mut self, item: Arc<T>) -> Result<Registration<Arc<T>>, RegistrationManagerError> {
        let id = Uuid::new_v4().to_string();
        let registration = Registration::new(id.clone(), self.registrations.clone());
        self.perform_registration(id, item)?;
        Ok(registration)
    }

    /**
     * Provides a snapshot of the current registrations.
     */
    pub fn get_registered_items(&self) -> LinkedList<Arc<T>> {
        // Panic is the lock is poisoned.  This lock does not wrap any condition that may panic.
        let registrations = self.registrations.read().unwrap();
        LinkedList::from_iter(registrations.values().map(|x| x.clone()))
    }

    fn perform_registration(&mut self, id: String, item: Arc<T>) -> Result<(), RegistrationManagerError> {
        // Panic is the lock is poisoned.  This lock does not wrap any condition that may panic.
        let mut registrations = self.registrations.write().unwrap();
        if registrations.contains_key(&id) {
            return Err(RegistrationManagerError::RetryableFailure());
        }
        registrations.insert(id, item);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use super::{ArcRegistrationManager, RegistrationManagerError};

    #[test]
    pub fn test_register() -> Result<(), RegistrationManagerError> {
        let mut subject: ArcRegistrationManager<u128> = super::ArcRegistrationManager::new();
        let registration = subject.register(Arc::new(12))?;
        let registrations = subject.get_registered_items();
        assert_eq!(registrations.len(), 1);
        match registrations.front() {
            Some(x) => assert_eq!(x.as_ref(), &12),
            None => assert!(false, "Failure path"),
        }

        drop(registration);
        assert_eq!(subject.get_registered_items().len(), 0);
        Ok(())
    }
}