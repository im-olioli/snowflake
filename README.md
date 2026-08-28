# Snowflake
My Rust implementation of Discord- and Twitter-style Snowflake IDs.
Additionally, in `./pg_snowflake` is the whole Postgres extension adding snowflake custom types and functions for decoding Snowflake IDs.

## Implementation
This implementation provides two methods for generating Snowflake IDs:

* **Fast — [Discord](https://docs.discord.com/developers/reference#snowflakes)-style:** multiple generators per worker. 
* **Slow — [Twitter](https://github.com/twitter-archive/snowflake/tree/snowflake-2010)-style:** a single shared generator per worker.

The implementation uses a custom epoch of 2026-08-01T00:00:00Z (`1785542400000` ms since the Unix epoch).
Decode functions return `timestamp_ms` (`created_at` in pg_snowflake) as a Unix timestamp in milliseconds.

### Fast
The fast implementation allows multiple generators to exist within a single worker.

Each generator can be assigned to a single thread or shared by a group of threads. 
This makes it possible to split threads between multiple independent generators
instead of having all threads contend for one shared generator.

For example, a worker may use:

* one generator per thread,
* one generator per group of threads,
* or any other distribution of threads across multiple generators.

Use this method when higher throughput and reduced contention are important.

### Slow
The slow implementation uses a single generator shared by all threads within a worker.

The generator is initialized once, and every thread uses the same shared generator when creating Snowflake IDs.

Use this method when simpler initialization and a single generator per worker are preferred.

### Fast - Construction
| Timestamp | Worker ID | Generator ID | Increment |
|-----------|-----------|--------------|-----------|
| 42 bits   | 5 bits    | 5 bits       | 12 bits   |

### Slow - Construction
| Timestamp | Worker ID | Increment |
|-----------|-----------|-----------|
| 42 bits   | 10 bits   | 12 bits   |

## Examples
### Server – .env file
```dotenv
WORKER_ID=0
BINDING_ADDR=127.0.0.1
HTTP_PORT=8080
```
### Server – Endpoint
```http request
GET http://localhost:8080/snowflake HTTP/2 (Prior Knowledge)
```
### Lib – Slow snowflake generator
```rust
use snowflake::*;

fn main() {
    init_slow(10);

    let snowflake = create_slow_snowflake()
        .expect("failed to create snowflake");

    let decoded = decode_slow_snowflake(snowflake);

    println!("snowflake: {snowflake}");
    println!("decoded: {decoded:?}");
}
```
### Lib – Fast snowflake generator
```rust
use snowflake::*;
use std::thread;

const THREADS: usize = 4;

fn main() {
    thread::scope(|scope| {
        for thread_id in 0..THREADS {
            scope.spawn(move || {
                let generator = create_generator(10, thread_id as u64);
                init_thread_fast(&generator);

                let snowflake = create_fast_snowflake()
                    .expect("failed to create snowflake");

                let decoded = decode_fast_snowflake(snowflake);

                println!("thread={thread_id}: {snowflake} -> {decoded:?}");
            });
        }
    });
}
```
## How to get lib
```toml
[dependencies]
snowflake = { git = "https://github.com/im-olioli/snowflake" }
```
or use a local .rlib
```toml
[dependencies]
snowflake = { path = "PATH_TO_RLIB_FILE" }
```
## Build And Run
### Snowflake Server
```shell
cargo run --package snowflake --bin snowflake_server --release
```
### Snowflake Lib
```shell
cargo build --package snowflake --lib --release
cp ./target/release/libsnowflake.rlib ./libsnowflake.rlib
```
### pg_snowflake (Postgres extension)
```shell
cd pg_snowflake
cargo pgrx run pg19
```
## Benchmark
Command used to run the benchmark:
```shell
cargo bench --package snowflake --bench snowflake_bench --release
```
Hardware:
* CPU: AMD Ryzen 7 3800X
* RAM: 32 GB DDR4
* OS: Bazzite DX NVIDIA Stable — Linux 7.2.0-ogc6.1.fc44.x86_64
* Tokio worker threads: 16

Each multithreaded task performs **100,000 Snowflake generation and decode operations**.

### Results
#### Single-threaded
| Implementation | Time per operation |    Throughput |
| -------------- | -----------------: | ------------: |
| `fast`         |            ~244 ns | ~4.10 M ops/s |
| `slow`         |            ~244 ns | ~4.10 M ops/s |
Single-threaded performance is effectively identical for both implementations.

#### Multithreaded
| Tasks |        `fast` |       `slow` | `fast` vs `slow` |
| ----: | ------------: | -----------: | ---------------: |
|     1 |  4.10 M ops/s | 4.10 M ops/s |             1.0× |
|     2 |  8.19 M ops/s | 3.66 M ops/s |             2.2× |
|     4 | 16.36 M ops/s | 4.08 M ops/s |             4.0× |
|     8 | 32.64 M ops/s | 3.40 M ops/s |             9.6× |
|    16 | 59.29 M ops/s | 3.09 M ops/s |            19.2× |
|    32 | 60.77 M ops/s | 3.09 M ops/s |            19.6× |
|    64 | 59.83 M ops/s | 3.09 M ops/s |            19.3× |
The `fast` implementation scales nearly linearly up to the number of available Tokio worker threads.

On this system, throughput peaks at approximately **61 million operations per second**, compared with approximately **3.1 million operations per second** for the `slow` implementation under contention.

At high concurrency, the `fast` implementation is approximately **19× faster** than `slow`.

> Benchmark results depend on hardware, operating system, scheduler, compiler version, and system load.

## Use of AI
I used an LLM in this project (ChatGPT 5.6 Sol with reasoning set to high).

Code generated by AI and pasted directly into the project is enclosed in region comments:
```
// region AI
    code generated by AI
// endregion
```

## Contact
If you've encountered a problem or bug, or if you need help, message me on Discord: im.olioli