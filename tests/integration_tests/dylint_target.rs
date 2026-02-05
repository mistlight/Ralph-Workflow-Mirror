//! Integration test for the `make dylint` target.
//!
//! This validates the Makefile's behavior in the mixed-install scenario where:
//! - a stable `cargo` exists on PATH (e.g., Homebrew/apt)
//! - `rustup` provides the nightly toolchain
//!
//! The regression we care about is that cargo-dylint may spawn a subprocess that
//! unsets `RUSTUP_TOOLCHAIN` and then invokes plain `cargo`, which would resolve
//! to the stable cargo on PATH unless the Makefile prepends the nightly toolchain
//! bin directory (or otherwise forces resolution).
//!
//! Per integration test rules, we do not spawn external processes (no `make`,
//! no `cargo`, no `rustup`). We assert the observable, deterministic behavior
//! of the Makefile content itself.

use crate::test_timeout::with_default_timeout;

#[test]
fn make_dylint_target_forces_nightly_cargo_resolution() {
    with_default_timeout(|| {
        let makefile = include_str!("../../Makefile");

        // Ensure we compute the nightly cargo path via rustup.
        assert!(
            makefile.contains("rustup which cargo --toolchain nightly"),
            "dylint target should resolve nightly cargo via rustup"
        );

        // Ensure we prepend a PATH entry so that cargo-dylint's internal `cargo`
        // subprocesses resolve to nightly even if `RUSTUP_TOOLCHAIN` is unset.
        assert!(
            makefile.contains("export PATH=\"$$WRAPPER_DIR:$$NIGHTLY_BIN_DIR:$$PATH\""),
            "dylint target should prepend wrapper + nightly bin dir to PATH"
        );

        // Ensure we use a wrapper `cargo` script which re-exports RUSTUP_TOOLCHAIN
        // to mitigate cargo-dylint unsetting it for driver rebuilds.
        assert!(
            makefile.contains("export RUSTUP_TOOLCHAIN=nightly"),
            "dylint target should export RUSTUP_TOOLCHAIN=nightly"
        );

        // We should not suppress rustup component installation failures.
        assert!(
            !makefile.contains(
                "rustup component add rustc-dev llvm-tools-preview --toolchain nightly || true"
            ),
            "dylint target must not suppress rustup component install failures"
        );

        // Offline/hermetic acceptance: do not unconditionally invoke a network-dependent
        // toolchain install when nightly is already installed.
        // (We allow toolchain install only when nightly is missing.)
        let has_guarded_nightly_install =
            makefile.contains("if ! rustup toolchain list | grep -qE \"^nightly\"; then");
        assert!(
            has_guarded_nightly_install,
            "dylint target should only install nightly when missing"
        );

        // Unset HOME should yield an actionable message before bash -u fails.
        assert!(
            makefile.contains("HOME_DIR=\"$${HOME:-}\""),
            "dylint target should guard access to HOME under bash -u"
        );
    });
}
