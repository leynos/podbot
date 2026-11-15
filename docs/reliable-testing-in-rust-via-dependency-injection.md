# 🛡️ Reliable testing in Rust via dependency injection

Writing robust, reliable, and parallelizable tests requires an intentional
approach to handling external dependencies such as environment variables, the
filesystem, or the system clock. Functions that directly call `std::env::var` or
`SystemTime::now()` are difficult to test because they depend on global,
non-deterministic state.

This leads to several problems:

- **Flaky Tests:** A test might pass or fail depending on the environment it
  runs in.
- **Parallel Execution Conflicts:** Tests that modify the same global
  environment variable (`std::env::set_var`) will interfere with each other
  when run with `cargo test`.
- **State Corruption:** A test that panics can fail to clean up its changes to
  the environment, poisoning subsequent tests.

The solution is a classic software design pattern: **Dependency Injection
(DI)**. Instead of a function reaching out to the global state, its
dependencies are provided as arguments. The `mockable` crate offers a
convenient set of traits (`Env`, `Clock`, etc.) to implement this pattern for
common system interactions in Rust.

______________________________________________________________________

## ✨ Mocking environment variables

### 1. Add `mockable`

First, add the crate to development dependencies in `Cargo.toml`.

```toml
[dev-dependencies]
mockable = { version = "3.0.0", default-features = false, features = ["clock", "mock"] }
```

### 2. The untestable code (before)

Directly calling `std::env` makes it difficult to test all logic paths
exhaustively.

```rust,no_run
pub fn get_api_key() -> Option<String> {
    match std::env::var("API_KEY") {
        Ok(key) if !key.is_empty() => Some(key),
        _ => None,
    }
}
```

### 3. Refactoring for testability (after)

The function is refactored to accept a generic type that implements the
`mockable::Env` trait.

```rust,no_run
use mockable::Env;

pub fn get_api_key(env: &impl Env) -> Option<String> {
    env.string("API_KEY").filter(|key| !key.is_empty())
}
```

The function's core logic remains unchanged, but its dependency on the
environment is now explicit and injectable.

### 4. Writing isolated unit tests

Tests can use `MockEnv`, a `mockall` mock of the `Env` trait, to simulate any
environmental condition without touching the actual process environment. The
mock is programmed with `expect_*` methods that mirror the trait methods
(`expect_string` for `Env::string`, and so on).

```rust,no_run
#[cfg(test)]
mod tests {
    use super::*;
    use mockable::MockEnv;

    /// Build a mock whose `API_KEY` is `value`, or unset when `None`.
    fn env_with_api_key(value: Option<&'static str>) -> MockEnv {
        let mut env = MockEnv::new();
        env.expect_string()
            .returning(move |key| value.filter(|_| key == "API_KEY").map(String::from));
        env
    }

    #[test]
    fn test_get_api_key_present() {
        let env = env_with_api_key(Some("secret123"));
        assert_eq!(get_api_key(&env), Some("secret123".to_string()));
    }

    #[test]
    fn test_get_api_key_missing() {
        let env = env_with_api_key(None);
        assert_eq!(get_api_key(&env), None);
    }

    #[test]
    fn test_get_api_key_present_but_empty() {
        let env = env_with_api_key(Some(""));
        assert_eq!(get_api_key(&env), None);
    }
}
```

These tests are fast, completely isolated from each other, and will never fail
due to external state.

### 5. Usage in production code

In production code, inject the default implementation, `DefaultEnv`, a unit
struct that reads the actual process environment.

```rust,no_run
use mockable::DefaultEnv;

fn main() {
    let env = DefaultEnv;
    if let Some(api_key) = get_api_key(&env) {
        println!("API Key found!");
    } else {
        println!("API Key not configured.");
    }
}
```

______________________________________________________________________

## 🔩 Handling other non-deterministic dependencies

This dependency injection pattern also applies to other non-deterministic
dependencies, such as the system clock. The `mockable` crate provides a `Clock`
trait for this purpose, with `local()` and `utc()` methods (enable the `clock`
feature).

### Untestable code

```rust,no_run
use std::time::{SystemTime, Duration};

fn is_cache_entry_stale(creation_time: SystemTime) -> bool {
    let timeout = Duration::from_secs(300);
    match SystemTime::now().duration_since(creation_time) {
        Ok(age) => age > timeout,
        Err(_) => false,
    }
}
```

### Testable refactor

```rust,no_run
use chrono::{DateTime, Duration, Utc};
use mockable::Clock;

fn is_cache_entry_stale(creation_time: DateTime<Utc>, clock: &impl Clock) -> bool {
    clock.utc() - creation_time > Duration::seconds(300)
}
```

### Testing with `MockClock`

`MockClock` is also a `mockall` mock: program the instants it returns with
`expect_utc` (or `expect_local`).

```rust,no_run
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use mockable::MockClock;

    fn clock_at(now: DateTime<Utc>) -> MockClock {
        let mut clock = MockClock::new();
        clock.expect_utc().return_const(now);
        clock
    }

    #[test]
    fn test_cache_is_not_stale() {
        let creation_time = Utc::now();
        let clock = clock_at(creation_time + Duration::seconds(100));
        assert!(!is_cache_entry_stale(creation_time, &clock));
    }

    #[test]
    fn test_cache_is_stale() {
        let creation_time = Utc::now();
        let clock = clock_at(creation_time + Duration::seconds(301));
        assert!(is_cache_entry_stale(creation_time, &clock));
    }
}
```

In production, `DefaultClock` (also a unit struct) would be used.

______________________________________________________________________

## 📌 Key takeaways

- **The Problem is Non-Determinism:** Directly accessing global state like
  `std::env` or `SystemTime::now` makes code difficult to test exhaustively.
- **The Solution is Dependency Injection:** Pass dependencies into functions as
  arguments.
- **Use** `mockable` **Traits:** Abstract dependencies behind traits such as
  `impl Env` or `impl Clock`.
- **`Mock*` for Tests:** Use `MockEnv` and `MockClock` in unit tests for
  isolated, deterministic control.
- **`Default*` for Production:** Use `DefaultEnv` and `DefaultClock` in the
  application to interact with the actual system.
- **`Env` is read-only:** The trait exposes reads (`string`, `all` and the typed
  getters) and no setters. Tests that need a particular environment inject a
  `MockEnv`; tests that must exercise the real process environment run the code
  in a subprocess and set variables with `Command::env`, never by mutating the
  test process's own environment.
