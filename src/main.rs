use axum::Router;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use dotenv::dotenv;
use snowflake::{FAST_GENERATOR_BITS, GeneratorState, create_fast_snowflake, create_generator, init_thread_fast};
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::{env, thread};
use tokio::net::TcpListener;
use tokio::runtime::Builder;

fn main() {
    dotenv().ok();

    let worker_id: u64 = env::var("WORKER_ID")
        .expect("WORKER_ID environment variable is not set")
        .parse()
        .expect("WORKER_ID must be a valid u64");

    let binding_addr: IpAddr = env::var("BINDING_ADDR")
        .expect("BINDING_ADDR environment variable is not set")
        .parse()
        .expect("BINDING_ADDR must be a valid IP address");

    let http_port: u16 = env::var("HTTP_PORT")
        .expect("HTTP_PORT environment variable is not set")
        .parse()
        .expect("HTTP_PORT must be a valid u16 port number");

    let https_addr: SocketAddr = SocketAddr::new(binding_addr, http_port);

    let available = thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(1);

    let worker_threads: usize = env::var("WORKER_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(available);

    let next_id = Arc::new(AtomicUsize::new(0));
    let worker_count = worker_threads;

    const MAX_PROCESSES: u64 = 1u64 << FAST_GENERATOR_BITS - 1;
    let generators: Vec<Arc<Mutex<GeneratorState>>> = (0..MAX_PROCESSES).map(|id| {
        create_generator(worker_id, id)
    }).collect();

    let rt = Builder::new_multi_thread()
         .worker_threads(worker_count)
         .on_thread_start(move || {
             let next_id = Arc::clone(&next_id);
             let id = next_id.fetch_add(1, Ordering::Relaxed);
             init_thread_fast(&generators[id%MAX_PROCESSES as usize]);
         })
         .enable_io()
         .build()
         .expect("failed to build Tokio runtime");

    rt.block_on(async_main(https_addr)).expect("Got error in async main");
}

async fn async_main(https_addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    let app = Router::new()
        .route("/health", get(|| async { (StatusCode::OK, "I am alive!").into_response() }))
        .route("/snowflake", get(snowflake));

    let listener = TcpListener::bind(https_addr).await?;

    axum::serve(listener, app).await?;

    Ok(())
}

async fn snowflake() -> impl IntoResponse {
    let snowflake = create_fast_snowflake().expect("Generator error");
    (StatusCode::OK, snowflake.to_string()).into_response()
}

