extern crate cloudpubsub;

use std::thread;
use std::time::Duration;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use cloudpubsub::{MqttOptions, MqttClient, MqttCallback};

fn main() {
    let options = MqttOptions::new().set_client_id("publisher-1").set_broker("localhost:5000");
    let count = Arc::new(AtomicUsize::new(0));
    let callback_count = count.clone();

    let counter_cb = move |_| {
        callback_count.fetch_add(1, Ordering::SeqCst);
    };
    let on_publish = MqttCallback::new().on_publish(counter_cb);

    let mut client = MqttClient::start(options, Some(on_publish)).expect("Start Error");

    for i in 0..1000 {
        client.publish("hello/world", vec![1, 2, 3, 4, 5]);
    }

    // verifies pingreqs and responses
    thread::sleep(Duration::from_secs(30));

    // disconnections because of pingreq delays will be know during
    // subsequent publishes
    for i in 0..100 {
        client.publish("hello/world", vec![1, 2, 3, 4, 5]);
    }

    thread::sleep(Duration::from_secs(31));
    println!("Total Ack Count = {:?}", count);
}
