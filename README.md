# unsync_channel

A `!Sync`, `!Send` channel designed for maximum throughput on single-threaded runtimes. 

## spsc::unbuffered

A single-producer, single-consumer, single-item channel. Benchmarks suggest that this has much lower overhead than
`tokio::sync::mpsc::channel`: in the case of a `LocalSet` sending and receiving between two tasks, it
takes about 75ns to send 10 items. For the equivalent tokio channel, it takes approximately 412ns.

Using the try_send and try_recv APIs, this queue takes 13ns to send and receive an item, versus 399ns for
the equivalent tokio channel.

Note that this channel has room for one and only one item. Should the item in the channel be unreceived, send attempts
will block. As this is a SPSC-style channel, however, fairness is not a concern.
