//! Observability: Prometheus text metrics + optional OpenTelemetry.

use crate::metrics::ClusterMetrics;

/// Render Prometheus exposition format from a metrics snapshot.
pub fn prometheus_text(node: &str, m: &ClusterMetrics) -> String {
    let mut out = String::new();
    out.push_str("# HELP chimera_peers Online peer count\n");
    out.push_str("# TYPE chimera_peers gauge\n");
    out.push_str(&format!("chimera_peers{{node=\"{node}\"}} {}\n", m.peers));
    out.push_str("# HELP chimera_tasks_pending Pending tasks\n");
    out.push_str("# TYPE chimera_tasks_pending gauge\n");
    out.push_str(&format!(
        "chimera_tasks_pending{{node=\"{node}\"}} {}\n",
        m.pending_tasks
    ));
    out.push_str("# HELP chimera_tasks_running Running tasks\n");
    out.push_str("# TYPE chimera_tasks_running gauge\n");
    out.push_str(&format!(
        "chimera_tasks_running{{node=\"{node}\"}} {}\n",
        m.running_tasks
    ));
    out.push_str("# HELP chimera_tasks_completed_total Completed tasks\n");
    out.push_str("# TYPE chimera_tasks_completed_total counter\n");
    out.push_str(&format!(
        "chimera_tasks_completed_total{{node=\"{node}\"}} {}\n",
        m.completed_tasks
    ));
    out.push_str("# HELP chimera_cpu_util_pct CPU utilization\n");
    out.push_str("# TYPE chimera_cpu_util_pct gauge\n");
    out.push_str(&format!(
        "chimera_cpu_util_pct{{node=\"{node}\"}} {}\n",
        m.local_caps.cpu_util_pct
    ));
    out.push_str("# HELP chimera_fs_cache_hits_total ChimeraFS cache hits\n");
    out.push_str("# TYPE chimera_fs_cache_hits_total counter\n");
    out.push_str(&format!(
        "chimera_fs_cache_hits_total{{node=\"{node}\"}} {}\n",
        m.fs_cache_hits
    ));
    out.push_str("# HELP chimera_fs_cache_misses_total ChimeraFS cache misses\n");
    out.push_str("# TYPE chimera_fs_cache_misses_total counter\n");
    out.push_str(&format!(
        "chimera_fs_cache_misses_total{{node=\"{node}\"}} {}\n",
        m.fs_cache_misses
    ));
    out.push_str("# HELP chimera_mem_faults_total ChimeraMEM remote faults\n");
    out.push_str("# TYPE chimera_mem_faults_total counter\n");
    out.push_str(&format!(
        "chimera_mem_faults_total{{node=\"{node}\"}} {}\n",
        m.mem_faults
    ));
    out.push_str("# HELP chimera_migrations_total Wasm migrations\n");
    out.push_str("# TYPE chimera_migrations_total counter\n");
    out.push_str(&format!(
        "chimera_migrations_total{{node=\"{node}\"}} {}\n",
        m.migrations
    ));
    out.push_str("# HELP chimera_receipts_verified_total Verified compute receipts\n");
    out.push_str("# TYPE chimera_receipts_verified_total counter\n");
    out.push_str(&format!(
        "chimera_receipts_verified_total{{node=\"{node}\"}} {}\n",
        m.verified_receipts
    ));
    out.push_str("# HELP chimera_pipeline_bytes_read_total Pipeline bytes read\n");
    out.push_str("# TYPE chimera_pipeline_bytes_read_total counter\n");
    out.push_str(&format!(
        "chimera_pipeline_bytes_read_total{{node=\"{node}\"}} {}\n",
        m.bytes_read
    ));
    out.push_str("# HELP chimera_pipeline_bytes_written_total Pipeline bytes written\n");
    out.push_str("# TYPE chimera_pipeline_bytes_written_total counter\n");
    out.push_str(&format!(
        "chimera_pipeline_bytes_written_total{{node=\"{node}\"}} {}\n",
        m.bytes_written
    ));
    out
}

/// Sampling strategy: parent-based with default 5% when OTEL enabled.
/// Documented overhead target: <2% CPU at typical mesh load via sampling.
pub const DEFAULT_TRACE_SAMPLE_RATIO: f64 = 0.05;

#[cfg(feature = "otel")]
pub mod otel {
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_otlp::WithExportConfig;
    use opentelemetry_sdk::trace::{RandomIdGenerator, Sampler};
    use opentelemetry_sdk::Resource;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    use super::DEFAULT_TRACE_SAMPLE_RATIO;

    pub fn init(service_name: &str, endpoint: &str) -> anyhow::Result<()> {
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .build()?;
        let provider = opentelemetry_sdk::trace::TracerProvider::builder()
            .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
            .with_sampler(Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(
                DEFAULT_TRACE_SAMPLE_RATIO,
            ))))
            .with_id_generator(RandomIdGenerator::default())
            .with_resource(Resource::new(vec![opentelemetry::KeyValue::new(
                "service.name",
                service_name.to_string(),
            )]))
            .build();
        let tracer = provider.tracer("chimera");
        let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);
        tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::from_default_env())
            .with(telemetry)
            .with(tracing_subscriber::fmt::layer())
            .try_init()
            .ok();
        Ok(())
    }
}

#[cfg(not(feature = "otel"))]
pub mod otel {
    pub fn init(_service_name: &str, _endpoint: &str) -> anyhow::Result<()> {
        tracing::debug!("otel feature disabled — using default tracing subscriber only");
        Ok(())
    }
}
