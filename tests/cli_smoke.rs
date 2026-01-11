#[test]
fn ralph_prints_version() {
    assert_cmd::cargo::cargo_bin_cmd!("ralph")
        .arg("--version")
        .assert()
        .success();
}
