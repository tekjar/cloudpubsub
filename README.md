# cloudpubsub

cloudpubsub is a customized version of mqtt client for higly reliable data transfer over unreliable networks by backing up data to disk during prolonged disconnections / slow network speeds. 


## reliability is acheived by

* backing up old data to disk when in memory queues are full. this helps keeping memory usage to minimum with out dropping any packets
* preserves publish sequence by transmitting old messages saved on disk first
* transmits pending data from disk even across reboots. this keeps data loss during reboots to minimum (only data which is inmemory will be lost)



**NOTE**: you can configure max number of messages that you can backup to disk before `cloudpubsub` starts dropping packets



## design considerations

Before reading below points, understand that `cloudpubsub` is not meant to be full fledged mqtt client. It doesn't not implement all the features of an mqtt client rather it strives to acheive (configurable) maximum reliability for data transmission using as little memory as possible

**ALL PUBLISHES AND SUBSCIBES ARE DONE ON QOS1**

This ensures that all your packets are transmitted atleast once. But duplicates are possible during reconnections

**PUBLISHES & SUBSCRIBES HAPPEN ON SEPERATE NETWORK CONNECTIONS OVER 2 THREADS**

The rationale behind this is that it's not really possible to do blocking reads and writes simultaneously on 2 different threads on a single network connection when TLS is involved. I considered these questions before deciding to use seperate n/w connections for pub & sub

**Q. Isn't it possible to clone `TlsStream`s?**

A. As far as I've read, no. When server requests a renegotiation, client's TLS `read()` call will do a write & TLS `write()` call can do a read. Since both the streams from 2 different threads share a single SSL object, and outstanding data in buffer causes races during renegotiations.

https://github.com/sfackler/rust-openssl/issues/338

**Q. What about creating read/write timeouts?**

A. Though  possible, this isn't a very scalable solution. `write`s might timeout when you write big data/during slow n/w speed and we have to take care of retransmittion of remaining data. Similar is the problem with `read`s. Partial data might be read before timing out and we wouldn't able to frame full packets in a go & we have to handle by buffering partial reads. Handling partial reads/ writes manually is very error prone

**Q. Maybe we could use tokio?**

A. I tried using tokio but hanlding automatic reconnections isn't trivial. More over we shouldn't do blocking file IOs in event loop for reading pending messages from disk.
