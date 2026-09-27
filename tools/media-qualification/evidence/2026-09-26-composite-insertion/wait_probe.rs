//! Environment diagnostic, not a product-performance qualification.
use std::sync::mpsc::sync_channel;
use std::thread;
use std::time::{Duration, Instant};

fn distribution(name: &str, mut samples: Vec<f64>) {
    samples.sort_by(f64::total_cmp);
    let count = samples.len();
    println!(
        "{{\"name\":\"{name}\",\"samples\":{count},\"p50_ms\":{},\"p95_ms\":{},\"max_ms\":{},\"above_16_ms\":{},\"above_50_ms\":{}}}",
        samples[count.div_ceil(2) - 1],
        samples[(count * 95).div_ceil(100) - 1],
        samples[count - 1],
        samples.iter().filter(|v| **v > 16.0).count(),
        samples.iter().filter(|v| **v > 50.0).count(),
    );
}

fn main() {
    let mut sleeps = Vec::new();
    for _ in 0..200 {
        let start = Instant::now();
        thread::sleep(Duration::from_millis(1));
        sleeps.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    let (send, receive) = sync_channel::<Instant>(1);
    let (ack, wait) = sync_channel::<()>(1);
    let producer = thread::spawn(move || {
        for _ in 0..200 {
            send.send(Instant::now()).unwrap();
            wait.recv_timeout(Duration::from_secs(5)).unwrap();
        }
    });
    let mut deliveries = Vec::new();
    for _ in 0..200 {
        let stamp = receive.recv_timeout(Duration::from_secs(5)).unwrap();
        deliveries.push(stamp.elapsed().as_secs_f64() * 1000.0);
        ack.send(()).unwrap();
    }
    producer.join().unwrap();
    distribution("requested_1ms_sleep", sleeps);
    distribution("timestamp_before_send_to_blocking_receive", deliveries);
}
