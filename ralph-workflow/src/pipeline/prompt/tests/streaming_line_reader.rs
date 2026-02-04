use super::*;

#[test]
fn test_streaming_line_reader_rejects_single_line_larger_than_max_buffer_size() {
    // Regression test: BufRead::lines() must not accumulate unbounded memory
    // when the stream never emits a newline.
    let data = vec![b'a'; MAX_BUFFER_SIZE + 1];
    let reader = StreamingLineReader::new(Cursor::new(data));

    let mut lines = reader.lines();
    let first = lines.next().expect("expected one line or an error");
    assert!(
        first.is_err(),
        "expected an error when a single line exceeds MAX_BUFFER_SIZE"
    );
}
