//! Recording writers report a poisoned capture lock rather than reading through it.

use std::sync::{Arc, Mutex};
use std::thread;

use rstest::rstest;
use tokio::io::AsyncWriteExt;

use super::*;

/// Poisons a capture lock the only way it can be poisoned: a holder panics.
fn poison(bytes: &Arc<Mutex<Vec<u8>>>) {
    let held = Arc::clone(bytes);
    let holder = thread::spawn(move || {
        let _guard = held.lock();
        panic!("a holder panics while the capture lock is held");
    });
    drop(holder.join());
}

#[rstest]
fn captured_bytes_reports_a_poisoned_lock() {
    let writer = RecordingWriter::new();
    poison(&writer.bytes);

    assert!(
        captured_bytes(&writer.bytes).is_err(),
        "a poisoned capture must be an error, not a read of half-written bytes"
    );
}

#[rstest]
fn a_write_to_a_poisoned_capture_fails(runtime: RuntimeFixture) {
    let runtime_handle = runtime.expect("the runtime fixture initialises");
    let mut writer = RecordingWriter::new();
    poison(&writer.bytes);

    let written = runtime_handle.block_on(writer.write(b"bytes"));

    assert!(
        written.is_err(),
        "a write into a poisoned capture must fail rather than report success"
    );
}
