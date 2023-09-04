use std::task::Context;

use bencher::*;
use futures::task::noop_waker_ref;
use tokio::task::{LocalSet, spawn_local};

/// A string function with a numeric return value.
fn bench_localset_baseline(b: &mut Bencher) {
    let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
    b.iter(|| {
        rt.block_on(async {
            let local = LocalSet::new();
            local.run_until(async {
                spawn_local(async {});
                spawn_local(async {});
            }).await;
        });
    });
}

/// A string function with a numeric return value.
fn bench_tokio_mpsc(b: &mut Bencher) {
    let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
    b.iter(|| {
        rt.block_on(async {
            let (tx, mut rx) = tokio::sync::mpsc::channel(1);
            let local = LocalSet::new();
            local.run_until(async {
                const COUNT: usize = 10;
                spawn_local(async move {
                    for _ in 0..COUNT {
                        tx.send(1).await.unwrap();
                    }
                });
                spawn_local(async move {
                    for _ in 0..COUNT {
                        rx.recv().await.unwrap();
                    }
                });
            }).await;
        });
    });
}

/// A string function with a numeric return value.
fn bench_spsc_unbuffered(b: &mut Bencher) {
    let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
    b.iter(|| {
        rt.block_on(async {
            let (mut tx, mut rx) = unsync_channel::spsc::unbuffered::channel();
            let local = LocalSet::new();
            local.run_until(async {
                const COUNT: usize = 10;
                spawn_local(async move {
                    for _ in 0..COUNT {
                        tx.send(1).await.unwrap();
                    }
                });
                spawn_local(async move {
                    for _ in 0..COUNT {
                        rx.recv().await.unwrap();
                    }
                });
            }).await;
        });
    });
}

fn bench_tokio_mpsc_try(b: &mut Bencher) {
    let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
    const COUNT: usize = 10;
    rt.block_on(async {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        b.iter(|| {
            for _ in 0..COUNT {
                tx.try_send(1).unwrap();
                rx.try_recv().unwrap();
            }
        })
    });
}

fn bench_tokio_mpsc_poll(b: &mut Bencher) {
    let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
    const COUNT: usize = 10;
    rt.block_on(tokio::task::unconstrained(async {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        b.iter(|| {
            let mut cx = Context::from_waker(noop_waker_ref());
            for _ in 0..COUNT {
                tx.try_send(1).unwrap();
                _ = rx.poll_recv(&mut cx);
            }
        })
    }));
}

fn bench_spsc_unbuffered_try(b: &mut Bencher) {
    let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
    const COUNT: usize = 10;
    rt.block_on(async {
        let (mut tx, mut rx) = unsync_channel::spsc::unbuffered::channel();
        b.iter(|| {
            for _ in 0..COUNT {
                tx.try_send(1).unwrap();
                rx.try_recv().unwrap();
            }
        })
    });
}

fn bench_spsc_unbuffered_poll(b: &mut Bencher) {
    let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
    const COUNT: usize = 10;
    rt.block_on(async {
        let (mut tx, mut rx) = unsync_channel::spsc::unbuffered::channel();
        b.iter(|| {
            let mut cx = Context::from_waker(noop_waker_ref());
            for _ in 0..COUNT {
                _ = tx.poll_send(&mut cx, &mut Some(1));
                _ = rx.poll_recv(&mut cx);
            }
        })
    });
}

benchmark_group!(
    benches,
    bench_localset_baseline,
    bench_tokio_mpsc,
    bench_tokio_mpsc_try,
    bench_tokio_mpsc_poll,
    bench_spsc_unbuffered,
    bench_spsc_unbuffered_try,
    bench_spsc_unbuffered_poll,
);

benchmark_main!(benches);
