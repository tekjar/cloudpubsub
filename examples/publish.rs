extern crate cloudpubsub;
extern crate pretty_env_logger;
extern crate rand;

use std::thread;
use std::time::Duration;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use rand::{thread_rng, Rng};

use cloudpubsub::{MqttOptions, MqttClient, MqttCallback};

fn main() {
    pretty_env_logger::init().unwrap();

    let options = MqttOptions::new().set_client_id("publisher-1")
                                    .set_clean_session(false)
                                    //.set_broker("dev-mqtt-broker.atherengineering.in:1883");
                                    .set_broker("localhost:1883");

    let count = Arc::new(AtomicUsize::new(0));
    let callback_count = count.clone();

    let counter_cb = move |_| {
        callback_count.fetch_add(1, Ordering::SeqCst);
    };
    let on_publish = MqttCallback::new().on_publish(counter_cb);

    let mut client = MqttClient::start(options, Some(on_publish)).expect("Start Error");

    for i in 0..100 {
        let len: usize = thread_rng().gen_range(0, 100_000);
        let mut v = vec![0; len];
        thread_rng().fill_bytes(&mut v);

        client.publish("hello/world", v);
    }

    // verifies pingreqs and responses
    thread::sleep(Duration::from_secs(30));

    // disconnections because of pingreq delays will be know during
    // subsequent publishes
    for i in 0..100 {
        let len: usize = thread_rng().gen_range(0, 100_000);
        let mut v = vec![0; len];
        thread_rng().fill_bytes(&mut v);

        client.publish("hello/world", v);
    }

    thread::sleep(Duration::from_secs(31));
    println!("Total Ack Count = {:?}", count);
}
