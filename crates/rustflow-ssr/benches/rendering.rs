use rustflow::view::View;
use rustflow_ssr::render_to_string;
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

fn measure(name: &str, iterations: usize, samples: usize, view: &View) {
    for _ in 0..iterations.min(100) {
        black_box(render_to_string(view).expect("warm up SSR render"));
    }

    let mut observations = Vec::with_capacity(samples);
    for _ in 0..samples {
        let started = Instant::now();
        for _ in 0..iterations {
            let html = render_to_string(black_box(view)).expect("render SSR benchmark view");
            black_box(html);
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
    let output_bytes = render_to_string(view)
        .expect("measure serialized output")
        .len();

    println!(
        "{{\"type\":\"measurement\",\"name\":\"{}\",\"iterations_per_sample\":{},\"samples\":{},\"min_ns_per_render\":{:.3},\"median_ns_per_render\":{:.3},\"p90_ns_per_render\":{:.3},\"p95_ns_per_render\":{:.3},\"p99_ns_per_render\":{},\"renders_per_second_at_median\":{:.3},\"html_bytes\":{}}}",
        name,
        iterations,
        samples,
        observations[0],
        median,
        percentile(&observations, 0.90),
        percentile(&observations, 0.95),
        p99,
        1_000_000_000.0 / median,
        output_bytes
    );
}

fn list_view(count: usize) -> View {
    let children = (0..count).map(|index| {
        View::element("li", [View::text(format!("Item {index}"))]).expect("build list item")
    });
    View::element("ul", children).expect("build list")
}

fn wide_tree(width: usize) -> View {
    let children = (0..width).map(|index| {
        View::element(
            "article",
            [
                View::element("h2", [View::text(format!("Heading {index}"))])
                    .expect("build heading"),
                View::element("p", [View::text(format!("Description {index}"))])
                    .expect("build description"),
            ],
        )
        .expect("build article")
    });
    View::element("main", children).expect("build wide tree")
}

fn nested_tree(depth: usize) -> View {
    let mut view = View::text("leaf");
    for level in (0..depth).rev() {
        view = View::element(
            "section",
            [
                View::element("h2", [View::text(format!("Level {level}"))])
                    .expect("build nested heading"),
                view,
            ],
        )
        .expect("build nested section");
    }
    view
}

fn main() {
    let iterations = env_usize("RUSTFLOW_BENCH_ITERATIONS", 1_000, 1, 100_000);
    let samples = env_usize("RUSTFLOW_BENCH_SAMPLES", 25, 5, 200);

    println!(
        "{{\"type\":\"metadata\",\"suite\":\"ssr\",\"architecture\":\"{}\",\"os\":\"{}\",\"iterations_per_sample\":{},\"samples\":{},\"commit\":\"{}\"}}",
        std::env::consts::ARCH,
        std::env::consts::OS,
        iterations,
        samples,
        std::env::var("GITHUB_SHA")
            .or_else(|_| std::env::var("RUSTFLOW_BENCH_COMMIT"))
            .unwrap_or_else(|_| "unspecified".to_owned())
    );

    let simple = View::element(
        "main",
        [
            View::element("h1", [View::text("RustFlow SSR benchmark")])
                .expect("build simple heading"),
            View::element("p", [View::text("Static server-rendered content")])
                .expect("build simple paragraph"),
        ],
    )
    .expect("build simple tree");
    measure("ssr.simple_static_tree", iterations, samples, &simple);

    let dynamic = View::element(
        "main",
        [
            View::text("Before "),
            View::dynamic(|| Ok(View::element("strong", [View::text("dynamic content")])?)),
            View::text(" after"),
        ],
    )
    .expect("build dynamic SSR tree");
    measure("ssr.dynamic_content_tree", iterations, samples, &dynamic);

    for depth in [5usize, 20, 80] {
        let tree = nested_tree(depth);
        let name = format!("ssr.nested_tree_depth_{depth}");
        measure(&name, iterations, samples, &tree);
    }

    for width in [100usize, 1_000] {
        let tree = wide_tree(width);
        let name = format!("ssr.wide_tree_{width}_components");
        let work_cap = (100_000 / width).max(1);
        let per_sample = (iterations / (width / 100).max(1)).clamp(1, work_cap);
        measure(&name, per_sample, samples, &tree);
    }

    for count in [100usize, 1_000, 10_000] {
        let tree = list_view(count);
        let name = format!("ssr.list_{count}_items");
        let work_cap = (100_000 / count).max(1);
        let per_sample = (iterations / (count / 100).max(1)).clamp(1, work_cap);
        measure(&name, per_sample, samples, &tree);
    }
}
