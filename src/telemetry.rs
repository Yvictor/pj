use opentelemetry::global;
use opentelemetry::metrics::{Counter, Gauge, Meter, MeterProvider};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider};
use opentelemetry_sdk::trace::TracerProvider;
use opentelemetry_sdk::{runtime, Resource};
use std::sync::OnceLock;
use std::time::Duration;
use tracing::info;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

const SERVICE_NAME: &str = "pj-proxy";

static METRICS: OnceLock<ProxyMetrics> = OnceLock::new();
static OTEL_RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

pub struct ProxyMetrics {
    pub connections_total: Counter<u64>,
    pub connections_active: Gauge<i64>,
    pub bytes_sent_total: Counter<u64>,
    pub bytes_received_total: Counter<u64>,
    pub connection_duration_seconds: opentelemetry::metrics::Histogram<f64>,
}

impl ProxyMetrics {
    fn new(meter: &Meter) -> Self {
        Self {
            connections_total: meter
                .u64_counter("pj_connections_total")
                .with_description("Total number of connections established")
                .build(),
            connections_active: meter
                .i64_gauge("pj_connections_active")
                .with_description("Number of currently active connections")
                .build(),
            bytes_sent_total: meter
                .u64_counter("pj_bytes_sent_total")
                .with_description("Total bytes sent to clients")
                .build(),
            bytes_received_total: meter
                .u64_counter("pj_bytes_received_total")
                .with_description("Total bytes received from clients")
                .build(),
            connection_duration_seconds: meter
                .f64_histogram("pj_connection_duration_seconds")
                .with_description("Connection duration in seconds")
                .build(),
        }
    }
}

pub fn get_metrics() -> &'static ProxyMetrics {
    METRICS.get().expect("Metrics not initialized. Call init_telemetry first.")
}

pub struct TelemetryConfig {
    pub otlp_endpoint: Option<String>,
    pub log_filter: String,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            otlp_endpoint: None,
            log_filter: "info".to_string(),
        }
    }
}

fn create_resource() -> Resource {
    Resource::new(vec![
        KeyValue::new(
            opentelemetry_semantic_conventions::attribute::SERVICE_NAME,
            SERVICE_NAME,
        ),
        KeyValue::new(
            opentelemetry_semantic_conventions::attribute::SERVICE_VERSION,
            env!("CARGO_PKG_VERSION"),
        ),
    ])
}

fn init_tracer_provider(endpoint: &str) -> Result<TracerProvider, opentelemetry::trace::TraceError> {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .with_timeout(Duration::from_secs(3))
        .build()?;

    let provider = TracerProvider::builder()
        .with_resource(create_resource())
        .with_batch_exporter(exporter, runtime::Tokio)
        .build();

    Ok(provider)
}

fn init_meter_provider(endpoint: &str) -> Result<SdkMeterProvider, opentelemetry_sdk::metrics::MetricError> {
    let exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .with_timeout(Duration::from_secs(3))
        .build()?;

    let reader = PeriodicReader::builder(exporter, runtime::Tokio)
        .with_interval(Duration::from_secs(10))
        .build();

    let provider = SdkMeterProvider::builder()
        .with_resource(create_resource())
        .with_reader(reader)
        .build();

    Ok(provider)
}

pub fn init_telemetry(config: TelemetryConfig) -> Result<(), Box<dyn std::error::Error>> {
    let env_filter = EnvFilter::new(&config.log_filter);

    match &config.otlp_endpoint {
        Some(endpoint) => {
            // Create a dedicated tokio runtime for OpenTelemetry
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()?;

            // Initialize OpenTelemetry within the runtime context
            let endpoint_clone = endpoint.clone();
            let (tracer_provider, meter_provider) = rt.block_on(async {
                let tracer_provider = init_tracer_provider(&endpoint_clone)
                    .map_err(|e| format!("Failed to init tracer: {}", e))?;
                let meter_provider = init_meter_provider(&endpoint_clone)
                    .map_err(|e| format!("Failed to init meter: {}", e))?;
                Ok::<_, Box<dyn std::error::Error>>((tracer_provider, meter_provider))
            })?;

            // Store the runtime to keep it alive
            let _ = OTEL_RUNTIME.set(rt);

            // Set global providers
            global::set_tracer_provider(tracer_provider.clone());
            global::set_meter_provider(meter_provider.clone());

            // Create tracer for tracing-opentelemetry layer
            let tracer = tracer_provider.tracer(SERVICE_NAME);

            // Initialize metrics
            let meter = meter_provider.meter(SERVICE_NAME);
            let _ = METRICS.set(ProxyMetrics::new(&meter));

            // Set up tracing subscriber with OpenTelemetry layer
            let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

            tracing_subscriber::registry()
                .with(env_filter)
                .with(tracing_subscriber::fmt::layer())
                .with(otel_layer)
                .init();

            info!(
                endpoint = %endpoint,
                "OpenTelemetry initialized with OTLP export"
            );
        }
        None => {
            // Initialize without OpenTelemetry - just tracing-subscriber for console logging
            // Still set up metrics for internal tracking (without export)
            let meter_provider = SdkMeterProvider::builder()
                .with_resource(create_resource())
                .build();

            global::set_meter_provider(meter_provider.clone());

            let meter = meter_provider.meter(SERVICE_NAME);
            let _ = METRICS.set(ProxyMetrics::new(&meter));

            tracing_subscriber::registry()
                .with(env_filter)
                .with(tracing_subscriber::fmt::layer())
                .init();

            info!("Telemetry initialized without OTLP export (PJ_OTLP_ENDPOINT not set)");
        }
    }

    Ok(())
}

pub fn shutdown_telemetry() {
    global::shutdown_tracer_provider();
}
