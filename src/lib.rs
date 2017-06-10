extern crate rand;
extern crate mqtt3;
#[macro_use]
extern crate slog;
extern crate slog_term;
#[macro_use]
extern crate quick_error;
extern crate openssl;
#[macro_use]
extern crate lazy_static;
extern crate threadpool;

pub mod error;
pub mod stream;
pub mod clientoptions;
pub mod callback;
pub mod publisher;
pub mod client;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MqttState {
    Handshake,
    Connected,
    Disconnected,
}

pub use clientoptions::MqttOptions;
pub use client::MqttClient;
pub use callback::MqttCallback;
