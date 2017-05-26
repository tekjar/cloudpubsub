use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};
// use std::net::{TcpStream, SocketAddr, Shutdown};
use std::collections::VecDeque;
use std::thread;
use std::io::Write;

use mqtt3::{PacketIdentifier, Packet, Connect, Protocol};
use slog::{Logger, Drain};
use slog_term;

use error::Result;
use stream::
NetworkStream;
use clientoptions::MqttOptions;
use callback::{Message, MqttCallback};
use super::MqttState;

#[derive(Debug)]
pub enum PublishRequest {
    Publish(Box<Message>),
    Shutdown,
    Disconnect,
}

pub struct Publisher {
    pub opts: MqttOptions,
    pub stream: NetworkStream,
    pub nw_request_rx: Receiver<PublishRequest>,
    pub state: MqttState,
    pub initial_connect: bool,
    pub await_pingresp: bool,
    pub last_flush: Instant,
    pub last_pkid: PacketIdentifier,
    pub callback: Option<MqttCallback>,
    pub outgoing_pub: VecDeque<(Box<Message>)>,
    pub no_of_reconnections: u32,
    pub logger: Logger,
}

impl Publisher {
    pub fn connect(opts: MqttOptions,
                   nw_request_rx: Receiver<PublishRequest>,
                   callback: Option<MqttCallback>)
                   -> Result<Self> {

        let mut publisher = Publisher {
            opts: opts,
            stream: NetworkStream::None,
            nw_request_rx: nw_request_rx,
            state: MqttState::Disconnected,
            initial_connect: true,
            await_pingresp: false,
            last_flush: Instant::now(),
            last_pkid: PacketIdentifier(0),

            outgoing_pub: VecDeque::new(),

            callback: callback,
            no_of_reconnections: 0,
            logger: publisher_logger(),
        };

        // Make initial tcp connection, send connect packet and
        // return if connack packet has errors. Doing this here
        // ensures that user doesn't have access to this object
        // before mqtt connection
        publisher.try_reconnect()?;
        //publisher.read_incoming()?;
        Ok(publisher)
    }

    /// Creates a Tcp Connection, Sends Mqtt connect packet and sets state to
    /// Handshake mode if Tcp write and Mqtt connect succeeds
    fn try_reconnect(&mut self) -> Result<()> {
        if !self.initial_connect {
            error!(self.logger, "  Will try Reconnect in 5 seconds");
            thread::sleep(Duration::new(5, 0));
        }

        let mut stream = NetworkStream::connect(&self.opts.addr, None, None)?;
        stream.set_read_timeout(Some(Duration::new(1, 0)))?;
        stream.set_write_timeout(Some(Duration::new(10, 0)))?;

        self.stream = stream;
        let connect = self.generate_connect_packet();
        let connect = Packet::Connect(connect);
        self.write_packet(connect)?;
        self.state = MqttState::Handshake;
        Ok(())
    }

    fn generate_connect_packet(&self) -> Box<Connect> {
        let keep_alive = if let Some(dur) = self.opts.keep_alive {
            dur
        } else {
            0
        };

        Box::new(Connect {
            protocol: Protocol::MQTT(4),
            keep_alive: keep_alive,
            client_id: self.opts.client_id.clone().unwrap(),
            clean_session: self.opts.clean_session,
            last_will: None,
            username: self.opts.credentials.clone().map(|u| u.0),
            password: self.opts.credentials.clone().map(|p| p.1),
        })
    }

    // NOTE: write_all() will block indefinitely by default if
    // underlying Tcp Buffer is full (during disconnections). This
    // is evident when test cases are publishing lot of data when
    // ethernet cable is unplugged (mantests/half_open_publishes_and_reconnections
    // but not during mantests/ping_reqs_in_time_and_reconnections due to low
    // frequency writes. 10 seconds migth be good default for write timeout ?)
    fn write_packet(&mut self, packet: Packet) -> Result<()> {
        if let Err(e) = self.stream.write_packet(&packet) {
            warn!(self.logger, "{:?}", e);
            return Err(e.into());
        }
        self.flush()?;
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        self.stream.flush()?;
        self.last_flush = Instant::now();
        Ok(())
    }
}


fn publisher_logger() -> Logger {
    use std::sync::Mutex;

    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = Mutex::new(drain).fuse();
    Logger::root(drain, o!("publisher" => env!("CARGO_PKG_VERSION")))
}