use std::io;
use std::result;
use std::net::TcpStream;
use std::sync::mpsc::{RecvError, TrySendError, RecvTimeoutError};

use mqtt3::{self, ConnectReturnCode};
use openssl;

use publisher::PublishRequest;

pub type SslError = openssl::error::ErrorStack;
pub type HandShakeError = openssl::ssl::HandshakeError<TcpStream>;
pub type Result<T> = result::Result<T, Error>;

quick_error! {
    #[derive(Debug)]
    pub enum Error {
        Io(err: io::Error) {
            from()
            description("io error")
            display("I/O error: {}", err)
            cause(err)
        }
        TryRecv(err: RecvError) {
            from()
        }
        TrySend(err: TrySendError<PublishRequest>) {
            from()
        }
        RecvTimeout(err: RecvTimeoutError) {
            from()
        }
        Mqtt3(err: mqtt3::Error) {
            from()
            display("mqtt3 error: {:?}", err)
            description("Mqtt3 error {}")
        }
        Ssl(err: SslError) {
            from()
            display("ssl error: {:?}", err)
        }
        Handshake(err: HandShakeError) {
            from()
            display("handshake error: {:?}", err)
        }
        MqttConnectionRefused(e: ConnectReturnCode) {
            from()
        }
        NoConnectionThread
        Reconnect
        PingTimeout
        AwaitPingResp
        ConnectionAbort
    }
}
