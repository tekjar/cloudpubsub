use std::str;
use std::sync::Arc;
use std::thread;
use std::sync::mpsc::{sync_channel, SyncSender};

use mqtt3::{self, QoS, TopicPath};

use error::{Result, Error};
use clientoptions::MqttOptions;
use publisher::{Publisher, PublishRequest};
use callback::{MqttCallback, Message};
use std::time::Duration;
use std::sync::mpsc::TrySendError;

pub struct MqttClient {
    pub nw_request_tx: SyncSender<PublishRequest>,
}

impl MqttClient {
    /// Connects to the broker and starts an event loop in a new thread.
    /// Returns 'Request' and handles reqests from it.
    /// Also handles network events, reconnections and retransmissions.
    pub fn start(opts: MqttOptions, callbacks: Option<MqttCallback>) -> Result<Self> {
        let (nw_request_tx, nw_request_rx) = sync_channel::<PublishRequest>(100);
        let mut connection = Publisher::connect(opts.clone(), nw_request_rx, callbacks)?;

        // This thread handles network reads (coz they are blocking) and
        // and sends them to event loop thread to handle mqtt state.
        thread::spawn(
            move || -> Result<()> {
                let _ = connection.run();
                error!("Network Thread Stopped !!!!!!!!!");
                Ok(())
            }
        );

        let client = MqttClient { nw_request_tx: nw_request_tx };

        Ok(client)
    }

    pub fn publish(&mut self, topic: &str, payload: Vec<u8>) -> Result<()> {
        let payload = Arc::new(payload);
        let mut ret_val;
        loop {
            let payload = payload.clone();
            ret_val = self._publish(topic, false, QoS::AtLeastOnce, payload, None);
            if let Err(Error::TrySend(ref e)) = ret_val {
                match e {
                    // break immediately if rx is dropped
                    &TrySendError::Disconnected(_) => return Err(Error::NoConnectionThread),
                    &TrySendError::Full(_) => {
                        warn!("Request Queue Full !!!!!!!!");
                        thread::sleep(Duration::new(2, 0));
                        continue;
                    }
                }
            } else {
                return ret_val;
            }
        }
    }

    fn _publish(&mut self, topic: &str, retain: bool, qos: QoS, payload: Arc<Vec<u8>>, userdata: Option<Arc<Vec<u8>>>) -> Result<()> {

        let topic = TopicPath::from_str(topic.to_string())?;

        let message = mqtt3::Message {
            topic: topic,
            retain: retain,
            qos: qos,
            payload: payload,
            pid: None,
        };

        let publish = Message {
            message: message,
            userdata: userdata,
        };
        let publish = Box::new(publish);

        // TODO: Check message sanity here and return error if not
        match qos {
            QoS::AtMostOnce | QoS::AtLeastOnce | QoS::ExactlyOnce => {
                self.nw_request_tx
                    .try_send(PublishRequest::Publish(publish))?
            }
        };

        Ok(())
    }
}
