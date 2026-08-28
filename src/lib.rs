use chrono::Utc;
use std::sync::{Arc, Mutex, OnceLock};

const MY_OWN_EPOCH_TIME: i64 = 1_785_542_400_000;

pub const TIME_BITS: u8 = 42;
const TIME_MASK: u64 = (1u64 << TIME_BITS) - 1;
const TIME_SHIFT: u8 = 64 - TIME_BITS;

pub const FAST_WORKER_BITS: u8 = 5;
pub const FAST_GENERATOR_BITS: u8 = 5;
pub const FAST_INCREMENT_BITS: u8 = 12;
const FAST_WORKER_MASK: u64 = (1u64 << FAST_WORKER_BITS) - 1;
const FAST_GENERATOR_MASK: u64 = (1u64 << FAST_GENERATOR_BITS) - 1;
const FAST_INCREMENT_MASK: u64 = (1u64 << FAST_INCREMENT_BITS) - 1;
const FAST_WORKER_SHIFT: u8 = FAST_GENERATOR_BITS + FAST_INCREMENT_BITS;
const FAST_GENERATOR_SHIFT: u8 = FAST_INCREMENT_BITS;

pub const SLOW_WORKER_BITS: u8 = 10;
pub const SLOW_INCREMENT_BITS: u8 = 12;
const SLOW_WORKER_MASK: u64 = (1u64 << SLOW_WORKER_BITS) - 1;
const SLOW_INCREMENT_MASK: u64 = (1u64 << SLOW_INCREMENT_BITS) - 1;
const SLOW_WORKER_SHIFT: u8 = SLOW_INCREMENT_BITS;

const _: () = {
    assert!(
        (TIME_BITS as u64 +
        FAST_WORKER_BITS as u64 +
        FAST_GENERATOR_BITS as u64 +
        FAST_INCREMENT_BITS as u64) <= 64,
        "Fast bytes bigger then 64 bits");
    assert!(
        (TIME_BITS as u64 +
            SLOW_WORKER_BITS as u64 +
            SLOW_INCREMENT_BITS as u64) <= 64,
        "Slow bytes bigger then 64 bits");
};

pub type FastSnowflake = u64;
pub type SlowSnowflake = u64;

#[derive(Debug)]
pub enum SnowflakeError {
    ThreadNotInitProperly,
    NotInitProperly
}

pub struct GeneratorState {
    worker_id: u64,
    generator_id: u64,
    last_increment: u64,
    increment: u64,
}
thread_local! {
    static FAST_STATE: OnceLock<Arc<Mutex<GeneratorState>>> = OnceLock::new();
}

static SLOW_STATE: OnceLock<Mutex<GeneratorState>> = OnceLock::new();

#[allow(dead_code)]
pub fn init_slow(worker_id: u64) {
    if worker_id > SLOW_WORKER_MASK {
        panic!("WORKER_ID out of range")
    }
    let state = SLOW_STATE.get_or_init( || Mutex::new(
        GeneratorState {
            worker_id,
            generator_id: 0,
            last_increment: 0,
            increment: 0,
        }
    ));
    state.lock().expect("Problem with mutex at init").worker_id = worker_id;
}

#[allow(dead_code)]
pub fn create_generator(worker_id: u64, generator_id: u64) -> Arc<Mutex<GeneratorState>> {
    if worker_id > FAST_WORKER_MASK {
        panic!("WORKER_ID out of range")
    }
    if generator_id > FAST_GENERATOR_MASK {
        panic!("generator_id out of range")
    }
    Arc::new(Mutex::new(GeneratorState {
        worker_id,
        generator_id,
        last_increment: 0,
        increment: 0,
    }))
}

#[allow(dead_code)]
pub fn init_thread_fast(generator: &Arc<Mutex<GeneratorState>>) {
    FAST_STATE.with(|v| {
        v.set(Arc::clone(generator))
    }).map_err(|_| { panic!("Problem with set once lock at init thread") }).unwrap()
}

fn get_timestamp() -> u64 {
    let timestamp_ms: i64 = Utc::now().timestamp_millis() - MY_OWN_EPOCH_TIME;
    if timestamp_ms <= 0 {
        panic!("System time is before my epoch")
    }
    timestamp_ms as u64
}

fn incrementation_logic(state: &mut GeneratorState, timestamp_ms: &mut u64, max_increment: u64) {
    if *timestamp_ms > state.last_increment {
        state.last_increment = *timestamp_ms;
        state.increment = 0;
    } else {
        *timestamp_ms = state.last_increment;
        state.increment = state.increment.saturating_add(1);
    }

    if state.increment > max_increment {
        while get_timestamp() <= state.last_increment {
            std::thread::yield_now();
        }

        *timestamp_ms = get_timestamp();
        state.last_increment = *timestamp_ms;
        state.increment = 0;
    }
}

#[allow(dead_code)]
pub fn create_fast_snowflake() -> Result<FastSnowflake, SnowflakeError> {
    FAST_STATE.with(|state| {
        let mut state = state.get().ok_or(SnowflakeError::ThreadNotInitProperly)?
            .lock().expect("Mutex poisoned");

        let mut timestamp_ms = get_timestamp();

        if state.worker_id > FAST_WORKER_MASK { panic!("WORKER_ID out of range") }

        let generator_id = state.generator_id;
        if generator_id > FAST_GENERATOR_MASK { panic!("generator_id out of range") }

        incrementation_logic(&mut *state, &mut timestamp_ms, FAST_INCREMENT_MASK);

        let timestamp_ms = timestamp_ms & TIME_MASK;
        let worker_id = state.worker_id & FAST_WORKER_MASK;
        let generator_id = generator_id & FAST_GENERATOR_MASK;
        let increment = state.increment & FAST_INCREMENT_MASK;

        Ok(
            (timestamp_ms << TIME_SHIFT)
                | (worker_id << FAST_WORKER_SHIFT)
                | (generator_id << FAST_GENERATOR_SHIFT)
                | increment,
        )
    })
}

#[derive(Debug)]
pub struct FastSnowflakeDecode {
    pub timestamp_ms: u64,
    pub worker_id: u64,
    pub generator_id: u64,
    pub increment: u64
}

#[allow(dead_code)]
pub fn decode_fast_snowflake(mut snowflake: FastSnowflake) -> FastSnowflakeDecode {
    let increment = snowflake & FAST_INCREMENT_MASK;
    snowflake >>= FAST_INCREMENT_BITS;
    let generator_id = snowflake & FAST_GENERATOR_MASK;
    snowflake >>= FAST_GENERATOR_BITS;
    let worker_id = snowflake & FAST_WORKER_MASK;
    snowflake >>= FAST_WORKER_BITS;
    let timestamp_ms = (snowflake & TIME_MASK) + MY_OWN_EPOCH_TIME as u64;


    FastSnowflakeDecode {
        timestamp_ms,
        worker_id,
        generator_id,
        increment,
    }
}

#[allow(dead_code)]
pub fn create_slow_snowflake() -> Result<SlowSnowflake, SnowflakeError> {
    let mut state = SLOW_STATE.get().ok_or(SnowflakeError::NotInitProperly)?
        .lock().expect("Mutex poisoned");

    let mut timestamp_ms = get_timestamp();

    if state.worker_id > SLOW_WORKER_MASK { panic!("WORKER_ID out of range") }

    incrementation_logic(&mut *state, &mut timestamp_ms, SLOW_INCREMENT_MASK);

    let timestamp_ms = timestamp_ms & TIME_MASK;
    let worker_id = state.worker_id & SLOW_WORKER_MASK;
    let increment = state.increment & SLOW_INCREMENT_MASK;

    Ok(
        (timestamp_ms << TIME_SHIFT)
            | (worker_id << SLOW_WORKER_SHIFT)
            | increment,
    )
}

#[derive(Debug)]
pub struct SlowSnowflakeDecode {
    pub timestamp_ms: u64,
    pub worker_id: u64,
    pub increment: u64
}

#[allow(dead_code)]
pub fn decode_slow_snowflake(mut snowflake: SlowSnowflake) -> SlowSnowflakeDecode {
    let increment = snowflake & SLOW_INCREMENT_MASK;
    snowflake >>= SLOW_INCREMENT_BITS;
    let worker_id = snowflake & SLOW_WORKER_MASK;
    snowflake >>= SLOW_WORKER_BITS;
    let timestamp_ms = (snowflake & TIME_MASK) + MY_OWN_EPOCH_TIME as u64;


    SlowSnowflakeDecode {
        timestamp_ms,
        worker_id,
        increment,
    }
}

// region AI
#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashSet;
    use std::thread;

    // #[test]
    // fn create_sample() {
    //     init_slow(10);
    //     init_thread_fast(10,20);
    //     let fast = create_fast_snowflake()
    //         .expect("err creating sample fast snowflake");
    //     let slow = create_slow_snowflake()
    //         .expect("err creating sample slow snowflake");
    //     println!("Sample snowflakes: fast: {}; slow: {}", fast, slow);
    // }

    #[test]
    fn decode_fast_extracts_correct_fields() {
        let timestamp = 0x123_4567_89AB & TIME_MASK;
        let worker_id = 0b10101;
        let generator_id = 0b11010;
        let increment = 0xABC;

        let snowflake =
            (timestamp << TIME_SHIFT)
                | (worker_id << FAST_WORKER_SHIFT)
                | (generator_id << FAST_GENERATOR_SHIFT)
                | increment;

        let decoded = decode_fast_snowflake(snowflake);

        assert_eq!(decoded.timestamp_ms - MY_OWN_EPOCH_TIME as u64, timestamp);
        assert_eq!(decoded.worker_id, worker_id);
        assert_eq!(decoded.generator_id, generator_id);
        assert_eq!(decoded.increment, increment);
    }

    #[test]
    fn decode_slow_extracts_correct_fields() {
        let timestamp = 0x123_4567_89AB & TIME_MASK;
        let worker_id = 0b10_1010_1010;
        let increment = 0xABC;

        let snowflake =
            (timestamp << TIME_SHIFT)
                | (worker_id << SLOW_WORKER_SHIFT)
                | increment;

        let decoded = decode_slow_snowflake(snowflake);

        assert_eq!(decoded.timestamp_ms - MY_OWN_EPOCH_TIME as u64, timestamp);
        assert_eq!(decoded.worker_id, worker_id);
        assert_eq!(decoded.increment, increment);
    }

    #[test]
    fn fast_returns_error_when_thread_is_not_initialized() {
        let result = thread::spawn(|| {
            matches!(
                create_fast_snowflake(),
                Err(SnowflakeError::ThreadNotInitProperly)
            )
        })
            .join()
            .unwrap();

        assert!(result);
    }

    #[test]
    fn fast_rejects_worker_id_out_of_range() {
        let result = std::panic::catch_unwind(|| {
            let generator = create_generator(FAST_WORKER_MASK + 1, 0);
            init_thread_fast(&generator);
        });

        assert!(result.is_err());
    }

    #[test]
    fn fast_rejects_generator_id_out_of_range() {
        let result = std::panic::catch_unwind(|| {
            let generator = create_generator(0, FAST_GENERATOR_MASK + 1);
            init_thread_fast(&generator);
        });

        assert!(result.is_err());
    }

    #[test]
    fn slow_rejects_worker_id_out_of_range() {
        let result = std::panic::catch_unwind(|| {
            init_slow(SLOW_WORKER_MASK + 1);
        });

        assert!(result.is_err());
    }

    #[test]
    fn increment_resets_when_timestamp_moves_forward() {
        let mut state = GeneratorState {
            worker_id: 1,
            generator_id: 1,
            last_increment: 100,
            increment: 123,
        };

        let mut timestamp = 101;

        incrementation_logic(
            &mut state,
            &mut timestamp,
            FAST_INCREMENT_MASK,
        );

        assert_eq!(timestamp, 101);
        assert_eq!(state.last_increment, 101);
        assert_eq!(state.increment, 0);
    }

    #[test]
    fn increment_uses_last_timestamp_when_clock_moves_backwards() {
        let mut state = GeneratorState {
            worker_id: 1,
            generator_id: 1,
            last_increment: 100,
            increment: 7,
        };
        
        let mut timestamp = 99;

        incrementation_logic(
            &mut state,
            &mut timestamp,
            FAST_INCREMENT_MASK,
        );
        
        assert_eq!(timestamp, 100);
        assert_eq!(state.last_increment, 100);
        assert_eq!(state.increment, 8);
    }

    #[test]
    fn increment_increases_within_same_millisecond() {
        let mut state = GeneratorState {
            worker_id: 1,
            generator_id: 1,
            last_increment: 100,
            increment: 15,
        };

        let mut timestamp = 100;

        incrementation_logic(
            &mut state,
            &mut timestamp,
            FAST_INCREMENT_MASK,
        );

        assert_eq!(timestamp, 100);
        assert_eq!(state.last_increment, 100);
        assert_eq!(state.increment, 16);
    }

    #[test]
    fn increment_overflow_waits_for_next_millisecond() {
        let timestamp = get_timestamp();

        let mut state = GeneratorState {
            worker_id: 1,
            generator_id: 1,
            last_increment: timestamp,
            increment: FAST_INCREMENT_MASK,
        };

        let mut new_timestamp = timestamp;
        
        incrementation_logic(
            &mut state,
            &mut new_timestamp,
            FAST_INCREMENT_MASK,
        );

        assert!(
            new_timestamp > timestamp,
            "timestamp should move to the next millisecond"
        );

        assert_eq!(state.last_increment, new_timestamp);
        assert_eq!(state.increment, 0);
    }

    #[test]
    fn fast_snowflakes_are_strictly_increasing_on_single_thread() {
        thread::spawn(|| {
            let worker_id = 17;
            let generator_id = 9;

            let generator = create_generator(worker_id, generator_id);
            init_thread_fast(&generator);

            let mut previous_id = None;
            let mut previous_decoded: Option<FastSnowflakeDecode> = None;

            for _ in 0..10_000 {
                let snowflake = create_fast_snowflake().unwrap();
                let decoded = decode_fast_snowflake(snowflake);

                assert_eq!(decoded.worker_id, worker_id);
                assert_eq!(decoded.generator_id, generator_id);
                assert!(decoded.timestamp_ms > 0);
                assert!(decoded.increment <= FAST_INCREMENT_MASK);
                
                let reconstructed =
                    ((decoded.timestamp_ms - MY_OWN_EPOCH_TIME as u64) << TIME_SHIFT)
                        | (decoded.worker_id << FAST_WORKER_SHIFT)
                        | (decoded.generator_id << FAST_GENERATOR_SHIFT)
                        | decoded.increment;

                assert_eq!(reconstructed, snowflake);

                if let Some(previous_id) = previous_id {
                    assert!(
                        snowflake > previous_id,
                        "snowflakes should be strictly increasing"
                    );
                }

                if let Some(previous) = previous_decoded {
                    assert!(
                        decoded.timestamp_ms >= previous.timestamp_ms,
                        "timestamp must never move backwards"
                    );

                    if decoded.timestamp_ms == previous.timestamp_ms {
                        assert_eq!(
                            decoded.increment,
                            previous.increment + 1,
                            "increment should increase inside the same millisecond"
                        );
                    } else {
                        assert_eq!(
                            decoded.increment, 0,
                            "increment should reset after timestamp changes"
                        );
                    }
                }

                previous_id = Some(snowflake);
                previous_decoded = Some(decoded);
            }
        })
            .join()
            .unwrap();
    }

    #[test]
    fn fast_snowflakes_are_unique_across_threads_with_unique_generator_ids() {
        const THREADS: usize = 8;
        const IDS_PER_THREAD: usize = 2_000;

        let worker_id = 3;

        let handles: Vec<_> = (0..THREADS)
            .map(|generator_id| {
                thread::spawn(move || {
                    let generator = create_generator(worker_id, generator_id as u64);
                    init_thread_fast(&generator);

                    let mut ids = Vec::with_capacity(IDS_PER_THREAD);

                    for _ in 0..IDS_PER_THREAD {
                        let snowflake = create_fast_snowflake().unwrap();

                        let decoded = decode_fast_snowflake(snowflake);

                        assert_eq!(decoded.worker_id, worker_id);
                        assert_eq!(decoded.generator_id, generator_id as u64);

                        ids.push(snowflake);
                    }

                    ids
                })
            })
            .collect();

        let mut all_ids = Vec::with_capacity(THREADS * IDS_PER_THREAD);

        for handle in handles {
            all_ids.extend(handle.join().unwrap());
        }

        let unique: HashSet<_> = all_ids.iter().copied().collect();

        assert_eq!(
            unique.len(),
            all_ids.len(),
            "duplicate FAST snowflakes detected"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn slow_snowflakes_are_unique_under_concurrency() {
        const TASKS: usize = 8;
        const IDS_PER_TASK: usize = 2_000;

        let worker_id = 777;

        init_slow(worker_id);

        let mut handles = Vec::with_capacity(TASKS);

        for _ in 0..TASKS {
            handles.push(tokio::task::spawn_blocking(|| {
                let mut ids = Vec::with_capacity(IDS_PER_TASK);

                for _ in 0..IDS_PER_TASK {
                    ids.push(create_slow_snowflake().unwrap());
                }

                ids
            }));
        }

        let mut all_ids = Vec::with_capacity(TASKS * IDS_PER_TASK);

        for handle in handles {
            all_ids.extend(
                handle
                    .await
                    .expect("spawn_blocking task panicked"),
            );
        }

        assert_eq!(
            all_ids.len(),
            TASKS * IDS_PER_TASK
        );

        let unique: HashSet<_> = all_ids.iter().copied().collect();

        assert_eq!(
            unique.len(),
            all_ids.len(),
            "duplicate SLOW snowflakes detected"
        );

        for snowflake in all_ids {
            let decoded = decode_slow_snowflake(snowflake);

            assert_eq!(decoded.worker_id, worker_id);
            assert!(decoded.increment <= SLOW_INCREMENT_MASK);
            assert!(decoded.timestamp_ms > 0);
        }
    }
}
// endregion

