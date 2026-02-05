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

        // Scope assertions to the `dylint:` recipe body so that similar lines in
        // `dylint-verbose` do not mask regressions.
        let dylint_body = {
            let start = makefile
                .find("\ndylint:")
                .expect("Makefile should contain a dylint: target")
                + 1;
            let rest = &makefile[start..];
            let end = rest.find("\ndylint-verbose:").unwrap_or(rest.len());
            &rest[..end]
        };

        // Ensure we compute the nightly cargo path via rustup.
        assert!(
            dylint_body.contains("rustup which cargo --toolchain nightly")
                || dylint_body.contains("\"$$RUSTUP_BIN\" which cargo --toolchain nightly"),
            "dylint target should resolve nightly cargo via rustup"
        );

        // Ensure we prepend a PATH entry so that cargo-dylint's internal `cargo`
        // subprocesses resolve to nightly even if `RUSTUP_TOOLCHAIN` is unset.
        assert!(
            dylint_body.contains("export PATH=\"$$WRAPPER_DIR:$$NIGHTLY_BIN_DIR:$$PATH\""),
            "dylint target should prepend wrapper + nightly bin dir to PATH"
        );

        // Ensure we use a wrapper `cargo` script which re-exports RUSTUP_TOOLCHAIN
        // to mitigate cargo-dylint unsetting it for driver rebuilds.
        assert!(
            dylint_body.contains("export RUSTUP_TOOLCHAIN=nightly"),
            "dylint target should export RUSTUP_TOOLCHAIN=nightly"
        );

        // We should not suppress rustup component installation failures.
        assert!(
            !dylint_body.contains(
                "rustup component add rustc-dev llvm-tools-preview --toolchain nightly || true"
            ),
            "dylint target must not suppress rustup component install failures"
        );

        // Offline/hermetic acceptance: do not unconditionally invoke a network-dependent
        // toolchain install when nightly is already installed.
        // (We allow toolchain install only when nightly is missing.)
        let has_guarded_nightly_install = dylint_body
            .contains("if ! rustup toolchain list | grep -qE \"^nightly\"; then")
            || dylint_body
                .contains("if ! \"$$RUSTUP_BIN\" toolchain list | grep -qE \"^nightly\"; then");
        assert!(
            has_guarded_nightly_install,
            "dylint target should only install nightly when missing"
        );

        // Unset HOME should yield an actionable message before bash -u fails.
        assert!(
            dylint_body.contains("HOME_DIR=\"$${HOME:-}\""),
            "dylint target should guard access to HOME under bash -u"
        );
    });
}
