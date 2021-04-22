use std::error::Error;
use crate::model::Interface;

pub trait RustyRouter {
    fn list_interfaces(&self) -> Result<Vec<Interface>, Box<dyn Error>>;
}
