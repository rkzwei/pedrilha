pub mod migrations;
pub mod models;
pub mod connection;

/// Re-export the database connection type.
pub use connection::Database;