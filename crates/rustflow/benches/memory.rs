//! Allocation accounting for component-scoped reactive owner lifecycles.
//!
//! This bench uses an instrumented process-global allocator only in this standalone
//! benchmark executable. It does not alter RustFlow library or application allocators.

use rustflow::{
    components::{ComponentDefinition, ComponentInstance, ComponentRender},
    reactivity::{Runtime, Scope, Signal},
};
use rustflow_view::View;
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, Stats, StatsAlloc};
use std::{alloc::System, hint::black_box, rc::Rc};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

type OwnerTree = (Runtime, Scope, Vec<Signal<String>>);

fn env_usize(name: &str, default: usize, minimum: usize, maximum: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| (minimum..=maximum).contains(value))
        .unwrap_or(default)
}

/// Bytes still outstanding according to allocator request counters at this snapshot.
/// This is not RSS and does not include allocator-internal fragmentation or reservations.
fn outstanding_requested_bytes(stats: Stats) -> i128 {
    stats.bytes_allocated as i128 - stats.bytes_deallocated as i128
        + stats.bytes_reallocated as i128
}

#[derive(Default)]
struct Totals {
    allocations: u128,
    deallocations: u128,
    reallocations: u128,
    bytes_allocated: u128,
    bytes_deallocated: u128,
    bytes_reallocated: i128,
    outstanding_after_setup: i128,
    outstanding_after_dispose: i128,
    max_outstanding_after_setup: i128,
    min_outstanding_after_dispose: Option<i128>,
    max_outstanding_after_dispose: Option<i128>,
}

impl Totals {
    fn record(&mut self, created: Stats, disposed: Stats) {
        self.allocations += disposed.allocations as u128;
        self.deallocations += disposed.deallocations as u128;
        self.reallocations += disposed.reallocations as u128;
        self.bytes_allocated += disposed.bytes_allocated as u128;
        self.bytes_deallocated += disposed.bytes_deallocated as u128;
        self.bytes_reallocated += disposed.bytes_reallocated as i128;

        let after_setup = outstanding_requested_bytes(created);
        let after_dispose = outstanding_requested_bytes(disposed);
        self.outstanding_after_setup += after_setup;
        self.outstanding_after_dispose += after_dispose;
        self.max_outstanding_after_setup = self.max_outstanding_after_setup.max(after_setup);
        self.min_outstanding_after_dispose = Some(
            self.min_outstanding_after_dispose
                .map_or(after_dispose, |value| value.min(after_dispose)),
        );
        self.max_outstanding_after_dispose = Some(
            self.max_outstanding_after_dispose
                .map_or(after_dispose, |value| value.max(after_dispose)),
        );
    }
}

fn measure_lifecycle<T>(
    name: &str,
    samples: usize,
    mut setup: impl FnMut() -> T,
    mut dispose: impl FnMut(T),
) {
    let mut totals = Totals::default();

    for _ in 0..samples {
        // Snapshot before setup. Stats snapshots themselves are stack values and do
        // not allocate; accumulated output is formatted only after all measurements.
        let region = Region::new(GLOBAL);
        let value = black_box(setup());
        let after_setup = region.change();
        dispose(black_box(value));
        let after_dispose = region.change();
        totals.record(after_setup, after_dispose);
    }

    let divisor = samples as f64;
    println!(
        "{{\"type\":\"memory_measurement\",\"name\":\"{}\",\"samples\":{},\"allocations_per_lifecycle\":{:.3},\"deallocations_per_lifecycle\":{:.3},\"reallocations_per_lifecycle\":{:.3},\"bytes_allocated_per_lifecycle\":{:.3},\"bytes_deallocated_per_lifecycle\":{:.3},\"bytes_reallocated_net_per_lifecycle\":{:.3},\"outstanding_requested_bytes_after_setup_mean\":{:.3},\"outstanding_requested_bytes_after_dispose_mean\":{:.3},\"outstanding_requested_bytes_after_setup_max\":{},\"outstanding_requested_bytes_after_dispose_min\":{},\"outstanding_requested_bytes_after_dispose_max\":{}}}",
        name,
        samples,
        totals.allocations as f64 / divisor,
        totals.deallocations as f64 / divisor,
        totals.reallocations as f64 / divisor,
        totals.bytes_allocated as f64 / divisor,
        totals.bytes_deallocated as f64 / divisor,
        totals.bytes_reallocated as f64 / divisor,
        totals.outstanding_after_setup as f64 / divisor,
        totals.outstanding_after_dispose as f64 / divisor,
        totals.max_outstanding_after_setup,
        totals.min_outstanding_after_dispose.unwrap_or_default(),
        totals.max_outstanding_after_dispose.unwrap_or_default()
    );
}

fn build_owner_tree(depth: usize, state_values_per_component: usize) -> OwnerTree {
    let runtime = Runtime::new();
    let root = runtime.root_scope();
    let top_owner = root.child().expect("create top component owner");
    let mut current_owner = top_owner.clone();
    let mut state_values = Vec::with_capacity(depth * state_values_per_component);

    for component_index in 0..depth {
        for state_index in 0..state_values_per_component {
            state_values.push(
                current_owner
                    .signal(format!("component-{component_index}-state-{state_index}"))
                    .expect("create component state signal"),
            );
        }

        if component_index + 1 < depth {
            current_owner = current_owner
                .child()
                .expect("create nested component owner");
        }
    }

    black_box(&state_values);
    (runtime, top_owner, state_values)
}

fn dispose_owner_tree((runtime, owner, state_values): OwnerTree) {
    owner.dispose().expect("dispose component owner tree");
    drop(state_values);
    drop(owner);
    drop(runtime);
}

type ComponentTree = (Runtime, Vec<ComponentInstance>);

fn measured_component_definition(state_values_per_component: usize) -> ComponentDefinition {
    ComponentDefinition::new("MeasuredMemoryComponent", move |context, _properties| {
        let states = (0..state_values_per_component)
            .map(|index| context.scope().signal(format!("component-state-{index}")))
            .collect::<Result<Vec<_>, _>>()?;
        let render: ComponentRender = Rc::new(move |_properties, _children| {
            for state in &states {
                black_box(state.get());
            }
            Ok(View::fragment([]))
        });
        Ok(render)
    })
    .expect("valid memory benchmark component definition")
}

fn build_component_tree(depth: usize, definition: &ComponentDefinition) -> ComponentTree {
    let runtime = Runtime::new();
    let mut parent_scope = runtime.root_scope();
    let mut instances = Vec::with_capacity(depth);

    for _ in 0..depth {
        let instance = ComponentInstance::mount(
            definition.clone(),
            runtime.clone(),
            &parent_scope,
            std::iter::empty::<rustflow_view::Property>(),
            std::iter::empty::<View>(),
        )
        .expect("mount measured component instance");
        parent_scope = instance.scope().clone();
        instances.push(instance);
    }

    drop(parent_scope);
    (runtime, instances)
}

fn dispose_component_tree((runtime, mut instances): ComponentTree) {
    for instance in instances.iter_mut().rev() {
        instance
            .unmount()
            .expect("unmount measured component instance");
    }
    drop(instances);
    drop(runtime);
}

fn main() {
    let samples = env_usize("RUSTFLOW_MEMORY_SAMPLES", 25, 5, 200);
    println!(
        "{{\"type\":\"metadata\",\"suite\":\"memory\",\"allocator\":\"stats_alloc over System\",\"architecture\":\"{}\",\"os\":\"{}\",\"samples\":{},\"commit\":\"{}\",\"measurement_note\":\"Requested allocation counters and owner-lifecycle outstanding-byte deltas; not RSS or allocator-resident pages.\"}}",
        std::env::consts::ARCH,
        std::env::consts::OS,
        samples,
        std::env::var("GITHUB_SHA")
            .or_else(|_| std::env::var("RUSTFLOW_BENCH_COMMIT"))
            .unwrap_or_else(|_| "unspecified".to_owned())
    );

    measure_lifecycle(
        "component_owner.depth_1_states_3",
        samples,
        || build_owner_tree(1, 3),
        dispose_owner_tree,
    );

    for depth in [5usize, 20, 80] {
        let name = format!("component_owner.depth_{depth}_states_3");
        measure_lifecycle(
            &name,
            samples,
            || build_owner_tree(depth, 3),
            dispose_owner_tree,
        );
    }

    // Component definitions are application-startup metadata, not per-instance
    // allocations. Reuse one definition across samples and measure mount/unmount only.
    let definition = measured_component_definition(3);
    measure_lifecycle(
        "component_instance.depth_1_states_3",
        samples,
        || build_component_tree(1, &definition),
        dispose_component_tree,
    );

    for depth in [5usize, 20, 80] {
        let name = format!("component_instance.depth_{depth}_states_3");
        measure_lifecycle(
            &name,
            samples,
            || build_component_tree(depth, &definition),
            dispose_component_tree,
        );
    }
}
