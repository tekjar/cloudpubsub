use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};
use std::net::Shutdown;
use std::collections::VecDeque;
use std::thread;
use std::io::{Write, ErrorKind};

use mqtt3::{self, QoS, PacketIdentifier, Packet, Connect, Connack, Protocol, ConnectReturnCode};
use slog::{Logger, Drain};
use slog_term;

use error::{Result, Error};
use stream::NetworkStream;
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
    pub fn connect(opts: MqttOptions, nw_request_rx: Receiver<PublishRequest>, callback: Option<MqttCallback>) -> Result<Self> {

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
        publisher.await()?;
        Ok(publisher)
    }

    pub fn run(&mut self) -> Result<()> {
        // @ Only read from `Network Request` channel when connected. Or else Empty
        // return.
        // @ Helps in case where Tcp connection happened but in MqttState::Handshake
        // state.
        if self.state == MqttState::Connected {
            loop {
                'publisher: loop {
                    match self.nw_request_rx.recv()? {
                        PublishRequest::Shutdown => self.stream.shutdown(Shutdown::Both)?,
                        PublishRequest::Disconnect => self.disconnect()?,
                        PublishRequest::Publish(m) => {
                            if let Err(e) = self.publish(m) {
                                error!(self.logger, "Publish error. Error = {:?}", e);
                                continue 'publisher;
                            }

                            // you'll know of disconnections immediately here even when writes
                            // doesn't error out immediately after disonnection
                            if let Err(e) = self.await() {
                                match e {
                                    Error::PingTimeout | Error::Reconnect => break 'publisher,
                                    Error::MqttConnectionRefused(_) => break 'publisher,
                                    _ => continue 'publisher,
                                }
                            }
                        }
                    };
                }

                'reconnect: loop {
                    match self.try_reconnect() {
                        Ok(_) => break 'reconnect,
                        Err(e) => {
                            error!(self.logger, "Couldn't connect. Error = {:?}", e);
                            if self.initial_connect {
                                error!(self.logger, "Stopping publisher. Error = {:?}", e);
                                return Err(e);
                            } else {
                                continue 'reconnect;
                            }
                        }
                    }
                }
            }
        }

        error!(self.logger, "Stopping publisher run loop");
        Ok(())
    }

    /// Creates a Tcp Connection, Sends Mqtt connect packet and sets state to
    /// Handshake mode if Tcp write and Mqtt connect succeeds
    fn try_reconnect(&mut self) -> Result<()> {
        if !self.initial_connect {
            error!(self.logger, "  Will try Reconnect in 5 seconds");
            thread::sleep(Duration::new(5, 0));
        }

        let mut stream = NetworkStream::connect(&self.opts.addr, None, None)?;
        stream.set_read_timeout(Some(Duration::new(3, 0)))?;
        // stream.set_write_timeout(Some(Duration::new(10, 0)))?;

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

        Box::new(
            Connect {
                protocol: Protocol::MQTT(4),
                keep_alive: keep_alive,
                client_id: self.opts.client_id.clone().expect("No Client Id"),
                clean_session: self.opts.clean_session,
                last_will: None,
                username: self.opts.credentials.clone().map(|u| u.0),
                password: self.opts.credentials.clone().map(|p| p.1),
            }
        )
    }

    pub fn await(&mut self) -> Result<()> {
        let packet = self.stream.read_packet();

        if let Ok(packet) = packet {
            if let Err(Error::MqttConnectionRefused(e)) = self.handle_packet(packet) {
                Err(Error::MqttConnectionRefused(e))
            } else {
                Ok(())
            }
        } else if let Err(Error::Mqtt3(mqtt3::Error::Io(e))) = packet {
            match e.kind() {
                ErrorKind::TimedOut | ErrorKind::WouldBlock => {
                    error!(self.logger, "Timeout waiting for ack. Error = {:?}", e);
                    self.unbind();
                    Err(Error::Reconnect)
                }
                _ => {
                    // Socket error are readily available here as soon as
                    // broker closes its socket end. (But not inbetween n/w disconnection
                    // and socket close at broker [i.e ping req timeout])

                    // UPDATE: Lot of publishes are being written by the time this notified
                    // the eventloop thread. Setting disconnect_block = true during write failure
                    error!(self.logger, "* Error receiving packet. Error = {:?}", e);
                    self.unbind();
                    Err(Error::Reconnect)
                }
            }
        } else {
            error!(self.logger, "** Error receiving packet. Error = {:?}", packet);
            self.unbind();
            Err(Error::Reconnect)
        }
    }

    fn handle_packet(&mut self, packet: Packet) -> Result<()> {
        match self.state {
            MqttState::Handshake => {
                if let Packet::Connack(connack) = packet {
                    self.handle_connack(connack)
                } else {
                    error!(self.logger, "Invalid Packet in Handshake State --> {:?}", packet);
                    Err(Error::ConnectionAbort)
                }
            }
            MqttState::Connected => {
                match packet {
                    Packet::Pingresp => {
                        self.await_pingresp = false;
                        Ok(())
                    }
                    Packet::Disconnect => Ok(()),
                    Packet::Puback(puback) => self.handle_puback(puback),
                    _ => {
                        error!(self.logger, "Invalid Packet in Connected State --> {:?}", packet);
                        Ok(())
                    }
                }
            }
            MqttState::Disconnected => {
                error!(self.logger, "Invalid Packet in Disconnected State --> {:?}", packet);
                Err(Error::ConnectionAbort)
            }
        }
    }

    ///  Checks Mqtt connack packet's status code and sets Mqtt state
    /// to `Connected` if successful
    fn handle_connack(&mut self, connack: Connack) -> Result<()> {
        let code = connack.code;

        if code != ConnectReturnCode::Accepted {
            error!(self.logger, "Failed to connect. Error = {:?}", code);
            return Err(Error::MqttConnectionRefused(code));
        }

        if self.initial_connect {
            self.initial_connect = false;
        }

        self.state = MqttState::Connected;

        // Retransmit QoS1,2 queues after reconnection when clean_session = false
        if !self.opts.clean_session {
            // self.force_retransmit();
        }

        Ok(())
    }

    fn publish(&mut self, publish_message: Box<Message>) -> Result<()> {
        let pkid = self.next_pkid();
        let publish_message = publish_message.transform(Some(pkid), None);
        let payload_len = publish_message.message.payload.len();
        let mut size_exceeded = false;

        match publish_message.message.qos {
            QoS::AtLeastOnce => {
                if payload_len > self.opts.storepack_sz {
                    size_exceeded = true;
                    warn!(self.logger, "Dropping packet: Size limit exceeded");
                } else {
                    self.outgoing_pub.push_back(publish_message.clone());
                }

                if self.outgoing_pub.len() > 50 * 50 {
                    warn!(self.logger, ":( :( Outgoing Publish Queue Length growing bad --> {:?}", self.outgoing_pub.len());
                }
            }
            _ => panic!("Invalid QoS"),
        }

        let packet = Packet::Publish(publish_message.message.to_pub(None, false));

        match publish_message.message.qos {
            QoS::AtMostOnce if !size_exceeded => self.write_packet(packet)?,
            QoS::AtLeastOnce | QoS::ExactlyOnce if !size_exceeded => {
                if self.state == MqttState::Connected {
                    self.write_packet(packet)?;
                } else {
                    warn!(self.logger, "State = {:?}. Skip network write", self.state);
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_puback(&mut self, pkid: PacketIdentifier) -> Result<()> {
        // debug!(self.logger, "*** PubAck --> Pkid({:?})\n--- Publish Queue =\n{:#?}\n\n", pkid, self.outgoing_pub);
        debug!(self.logger, "Received puback for: {:?}", pkid);

        let m = match self.outgoing_pub
                          .iter()
                          .position(|x| x.message.pid == Some(pkid)) {
            Some(i) => {
                if let Some(m) = self.outgoing_pub.remove(i) {
                    Some(*m)
                } else {
                    None
                }
            }
            None => {
                error!(self.logger, "Oopssss..unsolicited ack --> {:?}\n", pkid);
                None
            }
        };

        if let Some(val) = m {
            if let Some(ref callback) = self.callback {
                if let Some(ref on_publish) = callback.on_publish {
                    let on_publish = on_publish.clone();
                    // self.pool.execute(move || on_publish(val));
                }
            }
        }

        debug!(
            self.logger,
            "Pub Q Len After Ack @@@ {:?}",
            self.outgoing_pub.len()
        );
        Ok(())
    }

    pub fn disconnect(&mut self) -> Result<()> {
        let disconnect = Packet::Disconnect;
        self.write_packet(disconnect)?;
        Ok(())
    }

    fn ping(&mut self) -> Result<()> {
        // debug!("client state --> {:?}, await_ping --> {}", self.state,
        // self.await_ping);

        match self.state {
            MqttState::Connected => {
                if let Some(keep_alive) = self.opts.keep_alive {
                    let elapsed = self.last_flush.elapsed();

                    if elapsed >= Duration::from_millis(((keep_alive * 1000) as f64 * 0.9) as u64) {
                        if elapsed >= Duration::new((keep_alive + 1) as u64, 0) {
                            return Err(Error::PingTimeout);
                        }

                        // @ Prevents half open connections. Tcp writes will buffer up
                        // with out throwing any error (till a timeout) when internet
                        // is down. Eventhough broker closes the socket, EOF will be
                        // known only after reconnection.
                        // We just unbind the socket if there in no pingresp before next ping
                        // (What about case when pings aren't sent because of constant publishes
                        // ?. A. Tcp write buffer gets filled up and write will be blocked for 10
                        // secs and then error out because of timeout.)
                        if self.await_pingresp {
                            return Err(Error::AwaitPingResp);
                        }

                        let ping = Packet::Pingreq;
                        self.await_pingresp = true;
                        self.write_packet(ping)?;
                    }
                }
            }

            MqttState::Disconnected | MqttState::Handshake => {
                error!(
                    self.logger,
                    "I won't ping. Client is in disconnected/handshake state"
                )
            }
        }
        Ok(())
    }

    fn unbind(&mut self) {
        let _ = self.stream.shutdown(Shutdown::Both);
        self.await_pingresp = false;
        self.state = MqttState::Disconnected;

        // remove all the state
        if self.opts.clean_session {
            self.outgoing_pub.clear();
        }

        error!(self.logger, "  Disconnected {:?}", self.opts.client_id);
    }

    // http://stackoverflow.com/questions/11115364/mqtt-messageid-practical-implementation
    #[inline]
    fn next_pkid(&mut self) -> PacketIdentifier {
        let PacketIdentifier(mut pkid) = self.last_pkid;
        if pkid == 65535 {
            pkid = 0;
        }
        self.last_pkid = PacketIdentifier(pkid + 1);
        self.last_pkid
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
