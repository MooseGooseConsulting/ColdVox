//! Test modules for coldvox-text-injection

#[cfg(all(feature = "real-injection-tests", target_os = "linux"))]
pub mod real_injection;
#[cfg(all(feature = "real-injection-tests", target_os = "linux"))]
pub mod test_harness;
#[cfg(all(feature = "real-injection-tests", not(target_os = "linux")))]
mod real_injection {
    #[test]
    fn real_injection_tests_are_linux_only() {
        eprintln!("Skipping real-injection-tests: real desktop injection tests are Linux-only.");
    }
}
pub mod test_utils;
#[cfg(all(unix, feature = "ydotool"))]
pub mod test_ydotool_injector;
pub mod wl_copy_basic_test;
pub mod wl_copy_simple_test;
pub mod wl_copy_stdin_test;
