use rustflow::reactivity::Runtime;
use std::{hint::black_box, time::Instant};

fn env_usize(name: &str, default: usize, minimum: usize, maximum: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| (minimum..=maximum).contains(value))
        .unwrap_or(default)
}

fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    let index = (((sorted.len() - 1) as f64) * fraction).ceil() as usize;
    sorted[index]
}

fn measure(name: &str, iterations: usize, samples: usize, mut operation: impl FnMut(usize)) {
    for index in 0..iterations.min(1_000) {
        operation(index);
    }

    let mut observations = Vec::with_capacity(samples);
    for sample in 0..samples {
        let started = Instant::now();
        for index in 0..iterations {
            operation(sample.saturating_mul(iterations).saturating_add(index));
        }
        let elapsed = started.elapsed().as_nanos().max(1) as f64;
        observations.push(elapsed / iterations as f64);
    }

    observations.sort_by(f64::total_cmp);
    let median = percentile(&observations, 0.50);
    let p99 = if samples >= 100 {
        format!("{:.3}", percentile(&observations, 0.99))
    } else {
        "null".to_owned()
    };

    println!(
        "{{\"type\":\"measurement\",\"name\":\"{}\",\"iterations_per_sample\":{},\"samples\":{},\"min_ns_per_op\":{:.3},\"median_ns_per_op\":{:.3},\"p90_ns_per_op\":{:.3},\"p95_ns_per_op\":{:.3},\"p99_ns_per_op\":{},\"throughput_ops_per_second_at_median\":{:.3}}}",
        name,
        iterations,
        samples,
        observations[0],
        median,
        percentile(&observations, 0.90),
        percentile(&observations, 0.95),
        p99,
        1_000_000_000.0 / median
    );
}

fn main() {
    let iterations = env_usize("RUSTFLOW_BENCH_ITERATIONS", 10_000, 1, 1_000_000);
    let samples = env_usize("RUSTFLOW_BENCH_SAMPLES", 25, 5, 200);

    println!(
        "{{\"type\":\"metadata\",\"suite\":\"reactivity\",\"architecture\":\"{}\",\"os\":\"{}\",\"iterations_per_sample\":{},\"samples\":{},\"commit\":\"{}\"}}",
        std::env::consts::ARCH,
        std::env::consts::OS,
        iterations,
        samples,
        std::env::var("GITHUB_SHA")
            .or_else(|_| std::env::var("RUSTFLOW_BENCH_COMMIT"))
            .unwrap_or_else(|_| "unspecified".to_owned())
    );

    let runtime = Runtime::new();
    let root = runtime.root_scope();

    measure("runtime.create_and_drop", iterations, samples, |_| {
        black_box(Runtime::new());
    });

    let read_signal = root.signal(0usize).expect("create read signal");
    measure("signal.read", iterations, samples, |_| {
        black_box(read_signal.get());
    });

    let write_signal = root.signal(0usize).expect("create write signal");
    measure("signal.set_changed_value", iterations, samples, |index| {
        write_signal.set(black_box(index.wrapping_add(1)));
    });

    let update_signal = root.signal(0usize).expect("create update signal");
    measure("signal.update", iterations, samples, |_| {
        update_signal.update(|value| *value = value.wrapping_add(1));
    });

    let scheduled_signal = root.signal(0usize).expect("create scheduled signal");
    let scheduled_observer = scheduled_signal.clone();
    let _scheduled_effect = root
        .effect(move || {
            black_box(scheduled_observer.get());
        })
        .expect("create scheduled effect");
    measure(
        "effect.single_subscriber_set_and_flush",
        iterations,
        samples,
        |index| scheduled_signal.set(black_box(index.wrapping_add(1))),
    );

    let choose_left = root.signal(true).expect("create dependency selector");
    let left = root.signal(1usize).expect("create left dependency");
    let right = root.signal(2usize).expect("create right dependency");
    let selector = choose_left.clone();
    let left_observer = left.clone();
    let right_observer = right.clone();
    let _dynamic_effect = root
        .effect(move || {
            let selected = if selector.get() {
                left_observer.get()
            } else {
                right_observer.get()
            };
            black_box(selected);
        })
        .expect("create dynamic dependency effect");
    measure(
        "effect.dynamic_dependency_switch",
        iterations,
        samples,
        |index| choose_left.set(index % 2 == 0),
    );

    let memo_source = root.signal(1usize).expect("create memo source");
    let memo_source_for_compute = memo_source.clone();
    let memo = root
        .memo(move || memo_source_for_compute.get().wrapping_mul(2))
        .expect("create memo");
    measure("memo.cached_read", iterations, samples, |_| {
        black_box(memo.get());
    });
    measure("memo.source_update_and_recompute", iterations, samples, |index| {
        memo_source.set(index.wrapping_add(1));
        black_box(memo.get());
    });

    let batch_signals = (0..16)
        .map(|value| root.signal(value).expect("create batch signal"))
        .collect::<Vec<_>>();
    let batch_observers = batch_signals.clone();
    let _batch_effect = root
        .effect(move || {
            for signal in &batch_observers {
                black_box(signal.get());
            }
        })
        .expect("create batch observer");
    let batch_iterations = (iterations / 16).max(1).min(10_000);
    measure(
        "batch.sixteen_individual_writes",
        batch_iterations,
        samples,
        |index| {
            for (offset, signal) in batch_signals.iter().enumerate() {
                signal.set(index.wrapping_add(offset));
            }
        },
    );
    measure(
        "batch.sixteen_coalesced_writes",
        batch_iterations,
        samples,
        |index| {
            runtime
                .batch(|| {
                    for (offset, signal) in batch_signals.iter().enumerate() {
                        signal.set(index.wrapping_add(offset));
                    }
                })
                .expect("flush batch");
        },
    );

    for fanout in [1usize, 16, 64, 256, 1_024] {
        let fanout_runtime = Runtime::new();
        let fanout_root = fanout_runtime.root_scope();
        let fanout_signal = fanout_root.signal(0usize).expect("create fan-out signal");
        let mut effects = Vec::with_capacity(fanout);
        for _ in 0..fanout {
            let observed = fanout_signal.clone();
            effects.push(
                fanout_root
                    .effect(move || {
                        black_box(observed.get());
                    })
                    .expect("create fan-out effect"),
            );
        }

        let fanout_iterations = (iterations / fanout).max(1).min(10_000);
        let name = format!("load.shared_signal_fanout_{fanout}");
        measure(&name, fanout_iterations, samples, |index| {
            fanout_signal.set(index.wrapping_add(1));
        });
        black_box(effects.len());
        fanout_root.dispose().expect("dispose fan-out scope");
    }

    let scale_runtime = Runtime::new();
    let scale_scope = scale_runtime.root_scope().child().expect("create scale scope");
    let scale_signals = (0..1_000usize)
        .map(|value| scale_scope.signal(value).expect("create scale signal"))
        .collect::<Vec<_>>();
    let scale_observers = scale_signals.clone();
    let _scale_effect = scale_scope
        .effect(move || {
            let total = scale_observers.iter().map(|signal| signal.get()).sum::<usize>();
            black_box(total);
        })
        .expect("create scale effect");
    measure(
        "load.one_effect_with_1000_signal_dependencies",
        10,
        samples,
        |index| {
            scale_runtime
                .batch(|| {
                    for (offset, signal) in scale_signals.iter().enumerate() {
                        signal.set(index.wrapping_add(offset));
                    }
                })
                .expect("flush scale batch");
        },
    );
    scale_scope.dispose().expect("dispose scale scope");

    let lifecycle_runtime = Runtime::new();
    let lifecycle_root = lifecycle_runtime.root_scope();
    measure(
        "lifecycle.owner_signal_create_and_dispose",
        iterations.min(10_000),
        samples,
        |index| {
            let owner = lifecycle_root.child().expect("create signal owner");
            let signal = owner.signal(index).expect("create owner signal");
            black_box(signal);
            owner.dispose().expect("dispose signal owner");
        },
    );
    measure(
        "lifecycle.owner_effect_create_and_dispose",
        (iterations / 10).max(1).min(1_000),
        samples,
        |index| {
            let owner = lifecycle_root.child().expect("create effect owner");
            let signal = owner.signal(index).expect("create effect signal");
            let observed = signal.clone();
            let effect = owner
                .effect(move || {
                    black_box(observed.get());
                })
                .expect("create disposable effect");
            effect.dispose().expect("dispose effect");
            owner.dispose().expect("dispose effect owner");
        },
    );
    measure(
        "lifecycle.nested_owner_tree_cleanup",
        (iterations / 100).max(1).min(100),
        samples,
        |index| {
            let owner = lifecycle_root.child().expect("create root owner");
            let middle = owner.child().expect("create middle owner");
            let leaf = middle.child().expect("create leaf owner");
            let signal = leaf.signal(index).expect("create leaf signal");
            let observed = signal.clone();
            let _effect = leaf
                .effect(move || {
                    black_box(observed.get());
                })
                .expect("create tree effect");
            owner.dispose().expect("dispose nested owner tree");
        },
    );

    root.dispose().expect("dispose reactivity benchmark root");
}
