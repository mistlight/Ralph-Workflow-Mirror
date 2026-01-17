use crate::common::ralph_cmd;

#[test]
fn ralph_prints_version() {
    ralph_cmd().arg("--version").assert().success();
}
