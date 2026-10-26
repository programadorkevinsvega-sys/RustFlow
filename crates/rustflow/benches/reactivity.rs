use rustflow::reactivity::Runtime;
use std::{hint::black_box, time::Instant};

fn measure(name: &str, iterations: usize, mut operation: impl FnMut(usize)) {
    // Warm up before recording elapsed time.
    for i in 0..(iterations.min(10_000)) {
        operation(i);
    }
    let start = Instant::now();
    for i in 0..iterations {
        operation(i);
    }
    let elapsed = start.elapsed();
    let ns_per_op = elapsed.as_nanos() as f64 / iterations as f64;
    println!(
        "{name}: iterations={iterations} elapsed_ns={} ns_per_op={ns_per_op:.2}",
        elapsed.as_nanos()
    );
}

fn main() {
    let iterations = std::env::var("RUSTFLOW_BENCH_ITERATIONS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(100_000);

    println!("RustFlow reactivity microbench");
    println!("rustc={} target={} iterations={iterations}",
        option_env!("RUSTC_VERSION").unwrap_or("record rustc --version separately"),
        std::env::consts::ARCH);

    let runtime = Runtime::new();
    let signal = runtime.signal_eq(0usize).expect("create signal");
    measure("signal_read", iterations, |_| {
        black_box(signal.get());
    });

    let write_signal = runtime.signal_eq(0usize).expect("create write signal");
    measure("signal_write", iterations, |i| {
        write_signal.set(black_box(i.wrapping_mul(2).wrapping_add(1)));
    });

    let scheduling_runtime = Runtime::new();
    let scheduling_signal = scheduling_runtime.signal_eq(0usize).expect("create scheduling signal");
    let observed = scheduling_signal.clone();
    let _effect = scheduling_runtime.effect(move || {
        black_box(observed.get());
    }).expect("create scheduling effect");
    measure("single_effect_schedule", iterations, |i| {
        scheduling_signal.set(black_box(i.wrapping_mul(2).wrapping_add(1)));
    });

    let fanout_runtime = Runtime::new();
    let fanout_signal = fanout_runtime.signal_eq(0usize).expect("create fan-out signal");
    let fanout = 256usize;
    for _ in 0..fanout {
        let observed = fanout_signal.clone();
        fanout_runtime.effect(move || {
            black_box(observed.get());
        }).expect("create fan-out effect");
    }
    let fanout_iterations = (iterations / 10).max(1);
    measure("high_fanout_256", fanout_iterations, |i| {
        // One changed value per flush; all 256 dependent effects should run.
        fanout_runtime.batch(|| {
            fanout_signal.set(black_box(i.wrapping_mul(2).wrapping_add(1)));
        }).expect("flush fan-out batch");
    });
}
