extern crate cloudpubsub;

use std::thread;
use std::time::Duration;

use cloudpubsub::{MqttOptions, MqttClient};

fn main() {
    let options = MqttOptions::new().set_client_id("publisher-1");
    let mut client = MqttClient::start(options, None).expect("Start Error");

    for i in 0..100 {
        client.publish("hello/world", vec![1, 2, 3, 4, 5]);
    }

    // verifies pingreqs and responses
    thread::sleep(Duration::from_secs(20));

    // disconnections because of pingreq delays will be know during
    // subsequent publishes
    for i in 0..100 {
        client.publish("hello/world", vec![1, 2, 3, 4, 5]);
    }

    thread::sleep(Duration::from_secs(31));
}
