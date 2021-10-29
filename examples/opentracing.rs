use std::error::Error;
use std::thread;
use std::time::Duration;

use tracing::{instrument, span, trace, warn};
use tracing_subscriber::prelude::*;

#[instrument]
#[inline]
fn expensive_work() -> &'static str {
    span!(tracing::Level::INFO, "expensive_step_1")
        .in_scope(|| thread::sleep(Duration::from_millis(25)));
    span!(tracing::Level::INFO, "expensive_step_2")
        .in_scope(|| thread::sleep(Duration::from_millis(25)));

    "success"
}

fn main() -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    // Install an otel pipeline with a simple span processor that exports data one at a time when
    // spans end. See the `install_batch` option on each exporter's pipeline builder to see how to
    // export in batches.
    let tracer = opentelemetry_jaeger::new_pipeline()
        .with_service_name("tifs-report")
        .install_simple()?;

    tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(tracer))
        .try_init()?;

    let root = span!(tracing::Level::INFO, "app_start", work_units = 2);
    let _enter = root.enter();

    let work_result = expensive_work();

    span!(tracing::Level::INFO, "faster_work")
        .in_scope(|| thread::sleep(Duration::from_millis(10)));

    warn!("About to exit!");
    trace!("status: {}", work_result);

    Ok(())
}
