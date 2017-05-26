use std::sync::Arc;
use error::Result;
use mqtt3;

#[derive(Debug, Clone)]
pub struct Message {
    pub message: mqtt3::Message,
    pub userdata: Option<Arc<Vec<u8>>>,
}

impl Message {
    pub fn from_pub(publish: Box<mqtt3::Publish>) -> Result<Box<Message>> {
        let m = mqtt3::Message::from_pub(publish)?;
        let message = Message {
            message: *m,
            userdata: None,
        };
        Ok(Box::new(message))
    }

    pub fn transform(&self, pid: Option<mqtt3::PacketIdentifier>, qos: Option<mqtt3::QoS>) -> Box<Message> {
        Box::new(Message{ message: *self.message.transform(pid, qos), userdata: None})
    }
}

type MessageSendableFn = Box<Fn(Message) + Send + Sync>;
type PublishSendableFn = Box<Fn(Message) + Send + Sync>;

pub struct MqttCallback {
    pub on_message: Option<Arc<MessageSendableFn>>,
    pub on_publish: Option<Arc<PublishSendableFn>>,
}

impl MqttCallback {
    pub fn new() -> Self {
        MqttCallback {
            on_message: None,
            on_publish: None,
        }
    }

    pub fn on_message<F>(mut self, cb: F) -> Self
        where F: Fn(Message) + Sync + Send + 'static
    {
        self.on_message = Some(Arc::new(Box::new(cb)));
        self
    }

    pub fn on_publish<F>(mut self, cb: F) -> Self
        where F: Fn(Message) + Sync + Send + 'static
    {
        self.on_publish = Some(Arc::new(Box::new(cb)));
        self
    }
}