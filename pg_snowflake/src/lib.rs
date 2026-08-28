use pgrx::prelude::*;
use pgrx::{composite_type, StringInfo};
use serde::{Deserialize, Serialize};

pg_module_magic!(name, version);

#[derive(PostgresType, Serialize, Deserialize, Debug, Clone, Copy,
    PartialEq, Eq, PartialOrd, Ord, PostgresEq, PostgresOrd)]
#[inoutfuncs]
pub struct FastSnowflake(u64);

#[derive(PostgresType, Serialize, Deserialize, Debug, Clone, Copy,
    PartialEq, Eq, PartialOrd, Ord, PostgresEq, PostgresOrd)]
#[inoutfuncs]
pub struct SlowSnowflake(u64);

impl InOutFuncs for FastSnowflake {
    fn input(input: &std::ffi::CStr) -> Self {
        let s = input
            .to_str()
            .expect("snowflake must be valid UTF-8");

        let value = s
            .parse::<u64>()
            .expect("invalid snowflake");

        FastSnowflake(value)
    }

    fn output(&self, buffer: &mut StringInfo) {
        buffer.push_str(&self.0.to_string());
    }
}

impl InOutFuncs for SlowSnowflake {
    fn input(input: &std::ffi::CStr) -> Self {
        let s = input
            .to_str()
            .expect("snowflake must be valid UTF-8");

        let value = s
            .parse::<u64>()
            .expect("invalid snowflake");

        SlowSnowflake(value)
    }

    fn output(&self, buffer: &mut StringInfo) {
        buffer.push_str(&self.0.to_string());
    }
}

extension_sql!(r#"
CREATE TYPE decoded_fast_snowflake AS (
    created_at bigint,
    worker_id  bigint,
    generator_id bigint,
    increment  bigint
);

CREATE TYPE decoded_slow_snowflake AS (
    created_at bigint,
    worker_id  bigint,
    increment  bigint
);
"#,
name="define_composite_type");

#[pg_extern]
fn decode_fast_snowflake(snowflake: FastSnowflake) -> composite_type!('static, "decoded_fast_snowflake") {
    let decode = snowflake::decode_fast_snowflake(snowflake.0);
    let mut result =
        PgHeapTuple::new_composite_type("decoded_fast_snowflake").unwrap();
    result.set_by_name("created_at", decode.timestamp_ms as i64).unwrap();
    result.set_by_name("worker_id", decode.worker_id as i64).unwrap();
    result.set_by_name("generator_id", decode.generator_id as i64).unwrap();
    result.set_by_name("increment", decode.increment as i64).unwrap();
    result
}

#[pg_extern]
fn decode_slow_snowflake(snowflake: SlowSnowflake) -> composite_type!('static, "decoded_slow_snowflake") {
    let decode = snowflake::decode_slow_snowflake(snowflake.0);
    let mut result =
        PgHeapTuple::new_composite_type("decoded_slow_snowflake").unwrap();
    result.set_by_name("created_at", decode.timestamp_ms as i64).unwrap();
    result.set_by_name("worker_id", decode.worker_id as i64).unwrap();
    result.set_by_name("increment", decode.increment as i64).unwrap();
    result
}

#[pg_extern]
fn hello_pg_snowflake() -> String {
    String::from("Hello, pg_snowflake")
}

// region AI
#[cfg(any(test, feature = "pg_test"))]
#[pg_schema]
mod tests {
    use pgrx::prelude::*;

    const FAST_SNOWFLAKE: u64 = 7_499_374_726_037_504;
    const SLOW_SNOWFLAKE: u64 = 7_499_374_724_685_824;

    fn get_i64(sql: &str) -> i64 {
        Spi::get_one::<i64>(sql)
            .expect("SPI query failed")
            .expect("query returned NULL")
    }

    fn get_string(sql: &str) -> String {
        Spi::get_one::<String>(sql)
            .expect("SPI query failed")
            .expect("query returned NULL")
    }

    fn get_bool(sql: &str) -> bool {
        Spi::get_one::<bool>(sql)
            .expect("SPI query failed")
            .expect("query returned NULL")
    }

    #[pg_test]
    fn test_hello_pg_snowflake() {
        assert_eq!("Hello, pg_snowflake", crate::hello_pg_snowflake());
    }

    #[pg_test]
    fn test_fast_snowflake_input_output_roundtrip() {
        let result = get_string(&format!(
            "SELECT '{}'::FastSnowflake::text",
            FAST_SNOWFLAKE
        ));

        assert_eq!(result, FAST_SNOWFLAKE.to_string());
    }

    #[pg_test]
    fn test_slow_snowflake_input_output_roundtrip() {
        let result = get_string(&format!(
            "SELECT '{}'::SlowSnowflake::text",
            SLOW_SNOWFLAKE
        ));

        assert_eq!(result, SLOW_SNOWFLAKE.to_string());
    }

    #[pg_test]
    fn test_fast_snowflake_supports_full_u64_range() {
        let result = get_string(&format!(
            "SELECT '{}'::FastSnowflake::text",
            u64::MAX
        ));

        assert_eq!(result, u64::MAX.to_string());
    }

    #[pg_test]
    fn test_slow_snowflake_supports_full_u64_range() {
        let result = get_string(&format!(
            "SELECT '{}'::SlowSnowflake::text",
            u64::MAX
        ));

        assert_eq!(result, u64::MAX.to_string());
    }

    #[pg_test]
    fn test_fast_snowflake_uses_custom_input_function() {
        // Rust's u64 parser accepts a leading `+`.
        //
        // This is also useful as a regression test for #[inoutfuncs]:
        // pgrx's default JSON parser would reject "+42".
        let result = get_string(
            "SELECT '+42'::FastSnowflake::text"
        );

        assert_eq!(result, "42");
    }

    #[pg_test]
    fn test_slow_snowflake_uses_custom_input_function() {
        let result = get_string(
            "SELECT '+42'::SlowSnowflake::text"
        );

        assert_eq!(result, "42");
    }

    #[pg_test]
    fn test_decode_fast_snowflake() {
        let expected = snowflake::decode_fast_snowflake(FAST_SNOWFLAKE);

        let created_at = get_i64(&format!(
            "SELECT (decode_fast_snowflake('{}')).created_at",
            FAST_SNOWFLAKE
        ));

        let worker_id = get_i64(&format!(
            "SELECT (decode_fast_snowflake('{}')).worker_id",
            FAST_SNOWFLAKE
        ));

        let generator_id = get_i64(&format!(
            "SELECT (decode_fast_snowflake('{}')).generator_id",
            FAST_SNOWFLAKE
        ));

        let increment = get_i64(&format!(
            "SELECT (decode_fast_snowflake('{}')).increment",
            FAST_SNOWFLAKE
        ));

        assert_eq!(created_at, expected.timestamp_ms as i64);
        assert_eq!(worker_id, expected.worker_id as i64);
        assert_eq!(generator_id, expected.generator_id as i64);
        assert_eq!(increment, expected.increment as i64);
    }

    #[pg_test]
    fn test_decode_slow_snowflake() {
        let expected = snowflake::decode_slow_snowflake(SLOW_SNOWFLAKE);

        let created_at = get_i64(&format!(
            "SELECT (decode_slow_snowflake('{}')).created_at",
            SLOW_SNOWFLAKE
        ));

        let worker_id = get_i64(&format!(
            "SELECT (decode_slow_snowflake('{}')).worker_id",
            SLOW_SNOWFLAKE
        ));

        let increment = get_i64(&format!(
            "SELECT (decode_slow_snowflake('{}')).increment",
            SLOW_SNOWFLAKE
        ));

        assert_eq!(created_at, expected.timestamp_ms as i64);
        assert_eq!(worker_id, expected.worker_id as i64);
        assert_eq!(increment, expected.increment as i64);
    }

    #[pg_test]
    fn test_fast_snowflake_equality() {
        assert!(get_bool(&format!(
            "SELECT '{}'::FastSnowflake = '{}'::FastSnowflake",
            FAST_SNOWFLAKE,
            FAST_SNOWFLAKE
        )));

        assert!(!get_bool(&format!(
            "SELECT '{}'::FastSnowflake = '{}'::FastSnowflake",
            FAST_SNOWFLAKE,
            FAST_SNOWFLAKE + 1
        )));
    }

    #[pg_test]
    fn test_slow_snowflake_equality() {
        assert!(get_bool(&format!(
            "SELECT '{}'::SlowSnowflake = '{}'::SlowSnowflake",
            SLOW_SNOWFLAKE,
            SLOW_SNOWFLAKE
        )));

        assert!(!get_bool(&format!(
            "SELECT '{}'::SlowSnowflake = '{}'::SlowSnowflake",
            SLOW_SNOWFLAKE,
            SLOW_SNOWFLAKE + 1
        )));
    }

    #[pg_test]
    fn test_fast_snowflake_ordering() {
        assert!(get_bool(
            "SELECT '1'::FastSnowflake < '2'::FastSnowflake"
        ));

        assert!(get_bool(
            "SELECT '2'::FastSnowflake > '1'::FastSnowflake"
        ));

        assert!(get_bool(
            "SELECT '2'::FastSnowflake >= '2'::FastSnowflake"
        ));

        assert!(get_bool(
            "SELECT '1'::FastSnowflake <= '1'::FastSnowflake"
        ));
    }

    #[pg_test]
    fn test_slow_snowflake_ordering() {
        assert!(get_bool(
            "SELECT '1'::SlowSnowflake < '2'::SlowSnowflake"
        ));

        assert!(get_bool(
            "SELECT '2'::SlowSnowflake > '1'::SlowSnowflake"
        ));

        assert!(get_bool(
            "SELECT '2'::SlowSnowflake >= '2'::SlowSnowflake"
        ));

        assert!(get_bool(
            "SELECT '1'::SlowSnowflake <= '1'::SlowSnowflake"
        ));
    }

    #[pg_test]
    #[should_panic(expected = "invalid snowflake")]
    fn test_fast_snowflake_rejects_non_numeric_input() {
        let _ = Spi::get_one::<String>(
            "SELECT 'definitely-not-a-snowflake'::FastSnowflake::text"
        );
    }

    #[pg_test]
    #[should_panic(expected = "invalid snowflake")]
    fn test_slow_snowflake_rejects_non_numeric_input() {
        let _ = Spi::get_one::<String>(
            "SELECT 'definitely-not-a-snowflake'::SlowSnowflake::text"
        );
    }

    #[pg_test]
    #[should_panic(expected = "invalid snowflake")]
    fn test_fast_snowflake_rejects_overflow() {
        let _ = Spi::get_one::<String>(
            "SELECT '18446744073709551616'::FastSnowflake::text"
        );
    }

    #[pg_test]
    #[should_panic(expected = "invalid snowflake")]
    fn test_slow_snowflake_rejects_overflow() {
        let _ = Spi::get_one::<String>(
            "SELECT '18446744073709551616'::SlowSnowflake::text"
        );
    }
}
// endregion

/// This module is required by `cargo pgrx test` invocations.
/// It must be visible at the root of your extension crate.
#[cfg(test)]
pub mod pg_test {
    pub fn setup(_options: Vec<&str>) {
        // perform one-off initialization when the pg_test framework starts
    }

    #[must_use]
    pub fn postgresql_conf_options() -> Vec<&'static str> {
        // return any postgresql.conf settings that are required for your tests
        vec![]
    }
}
