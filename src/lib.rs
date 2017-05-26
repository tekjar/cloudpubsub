extern crate rand;
extern crate mqtt3;
#[macro_use]
extern crate slog;
extern crate slog_term;
#[macro_use]
extern crate quick_error;
extern crate openssl;

pub mod error;
pub mod stream;
pub mod clientoptions;
pub mod callback;
pub mod publisher;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MqttState {
    Handshake,
    Connected,
    Disconnected,
}
