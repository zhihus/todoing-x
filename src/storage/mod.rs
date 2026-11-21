mod error;
mod store;

#[cfg(test)]
mod tests;

pub use error::StoreError;
pub use store::{IntegrityReport, Store};
