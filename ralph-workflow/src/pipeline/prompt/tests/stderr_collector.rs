use super::*;

#[derive(Debug)]
struct CountingReader {
    data: Vec<u8>,
    pos: usize,
    total_read: Arc<AtomicUsize>,
}

impl CountingReader {
    fn new(data: Vec<u8>, total_read: Arc<AtomicUsize>) -> Self {
        Self {
            data,
            pos: 0,
            total_read,
        }
    }
}

impl Read for CountingReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos >= self.data.len() {
            return Ok(0);
        }
        let remaining = self.data.len() - self.pos;
        let n = remaining.min(buf.len());
        buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
        self.pos += n;
        self.total_read.fetch_add(n, Ordering::SeqCst);
        Ok(n)
    }
}

#[test]
fn test_collect_stderr_with_cap_drains_to_eof() {
    let total_read = Arc::new(AtomicUsize::new(0));
    let data = (0..100u8).collect::<Vec<u8>>();
    let reader = CountingReader::new(data.clone(), Arc::clone(&total_read));

    let cancel = AtomicBool::new(false);
    let result = collect_stderr_with_cap_and_drain(reader, 10, &cancel).unwrap();
    assert!(result.contains("<stderr truncated>"));
    assert_eq!(total_read.load(Ordering::SeqCst), data.len());
}

#[test]
fn test_collect_stderr_with_cap_and_drain_retries_on_wouldblock() {
    // Non-blocking stderr reads can return WouldBlock when no data is available.
    // The collector must treat this as "no data yet" rather than a fatal error.
    #[derive(Debug)]
    struct WouldBlockThenEof {
        remaining_wouldblock: usize,
    }

    impl Read for WouldBlockThenEof {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            if self.remaining_wouldblock > 0 {
                self.remaining_wouldblock -= 1;
                return Err(io::Error::from(io::ErrorKind::WouldBlock));
            }
            Ok(0)
        }
    }

    let reader = WouldBlockThenEof {
        remaining_wouldblock: 3,
    };

    let cancel = AtomicBool::new(false);
    let out = collect_stderr_with_cap_and_drain(reader, 1024, &cancel)
        .expect("stderr collector should not fail on WouldBlock");
    assert!(out.is_empty());
}
