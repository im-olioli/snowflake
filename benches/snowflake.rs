use criterion::{criterion_group, criterion_main};

mod bench {
    use std::hint::black_box;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    use std::time::Duration;
    use criterion::{BenchmarkId, Criterion, Throughput};
    use snowflake::*;
    use tokio::runtime::{Builder, Runtime};
    use tokio::task::JoinSet;

    const OPERATIONS_PER_TASK: u64 = 100000;

    fn create_multithreaded_runtime() -> (Runtime, usize) {
        init_slow(10);

        let available = thread::available_parallelism()
            .map(|value| value.get())
            .unwrap_or(1);

        let max_worker_threads = 1usize << FAST_GENERATOR_BITS;

        let worker_threads = available.min(max_worker_threads);

        println!(
            "Starting Tokio with {worker_threads} worker threads \
         ({available} available, maximum {max_worker_threads})"
        );

        let next_id = Arc::new(AtomicUsize::new(0));
        let worker_count = worker_threads;

        (Builder::new_multi_thread()
            .worker_threads(worker_count)
            .on_thread_start(move || {
                let next_id = Arc::clone(&next_id);

                let id = next_id.fetch_add(1, Ordering::Relaxed);

                if id < worker_count {
                    let generator = create_generator(10, id as u64);
                    init_thread_fast(&generator);

                    println!(
                        "initialized snowflake thread: {:?}, id={id}",
                        std::thread::current().id(),
                    );
                } else {
                    println!(
                        "runtime non-snowflake thread: {:?}",
                        std::thread::current().id(),
                    );
                }
            })
            .enable_all()
            .build()
            .expect("failed to build Tokio runtime"), worker_count)
    }

    pub fn fast_single(c: &mut Criterion) {
        let generator = create_generator(10, 10);
        init_thread_fast(&generator);
        c.bench_function("snowflake/fast_single", |b| {
            b.iter(|| {
                let snowflake = create_fast_snowflake().expect("Cannot create snowflake");
                let decoded = decode_fast_snowflake(snowflake);
                black_box(decoded)
            });
        });
    }

    pub fn fast_multithreaded(c: &mut Criterion) {
        let (runtime, worker_count) = create_multithreaded_runtime();

        let mut group = c.benchmark_group("snowflake/fast_multithreaded");
        group
            .warm_up_time(Duration::from_secs(5))
            .measurement_time(Duration::from_secs(30))
            .sample_size(50);

        let mut task_counts = vec![
            1,
            2,
            4,
            8,
            worker_count,
            worker_count.saturating_mul(2),
            worker_count.saturating_mul(4),
        ];

        task_counts.sort_unstable();
        task_counts.dedup();

        for task_count in task_counts {
            let task_count = task_count.max(1);
            let operations = task_count as u64 * OPERATIONS_PER_TASK;

            group.throughput(Throughput::Elements(operations));

            group.bench_with_input(
                BenchmarkId::new("tasks", task_count),
                &task_count,
                |b, &task_count| {
                    b.to_async(&runtime).iter(|| async move {
                        let mut tasks = JoinSet::new();

                        for _ in 0..task_count {
                            tasks.spawn(async {
                                let mut checksum = 0u64;

                                for _ in 0..OPERATIONS_PER_TASK {
                                    let snowflake = create_fast_snowflake().expect("Cannot create snowflake");
                                    let decode = decode_fast_snowflake(snowflake);
                                    black_box(decode);
                                    checksum ^= black_box(snowflake);
                                }

                                checksum
                            });
                        }

                        let mut combined_checksum = 0u64;

                        while let Some(result) = tasks.join_next().await {
                            combined_checksum ^= result.expect("Task panicked");
                        }

                        black_box(combined_checksum)
                    })
                }
            );
        }

        group.finish();
    }

    pub fn slow_single(c: &mut Criterion) {
        init_slow(10);
        c.bench_function("snowflake/slow_single", |b| {
            b.iter(|| {
                let snowflake = create_slow_snowflake().expect("Cannot create snowflake");
                let decoded = decode_slow_snowflake(snowflake);
                black_box(decoded)
            });
        });
    }

    pub fn slow_multithreaded(c: &mut Criterion) {
        let (runtime, worker_count) = create_multithreaded_runtime();

        let mut group = c.benchmark_group("snowflake/slow_multithreaded");
        group
            .warm_up_time(Duration::from_secs(5))
            .measurement_time(Duration::from_secs(30))
            .sample_size(50);

        let mut task_counts = vec![
            1,
            2,
            4,
            8,
            worker_count,
            worker_count.saturating_mul(2),
            worker_count.saturating_mul(4),
        ];

        task_counts.sort_unstable();
        task_counts.dedup();

        for task_count in task_counts {
            let task_count = task_count.max(1);
            let operations = task_count as u64 * OPERATIONS_PER_TASK;

            group.throughput(Throughput::Elements(operations));

            group.bench_with_input(
                BenchmarkId::new("tasks", task_count),
                &task_count,
                |b, &task_count| {
                    b.to_async(&runtime).iter(|| async move {
                        let mut tasks = JoinSet::new();

                        for _ in 0..task_count {
                            tasks.spawn(async {
                                let mut checksum = 0u64;

                                for _ in 0..OPERATIONS_PER_TASK {
                                    let snowflake = create_slow_snowflake().expect("Cannot create snowflake");
                                    let decode = decode_slow_snowflake(snowflake);
                                    black_box(decode);
                                    checksum ^= black_box(snowflake);
                                }

                                checksum
                            });
                        }

                        let mut combined_checksum = 0u64;

                        while let Some(result) = tasks.join_next().await {
                            combined_checksum ^= result.expect("Task panicked");
                        }

                        black_box(combined_checksum)
                    })
                }
            );
        }

        group.finish();
    }
}

criterion_group!(
    benches,
    bench::fast_single,
    bench::fast_multithreaded,
    bench::slow_single,
    bench::slow_multithreaded,
);

criterion_main!(benches);