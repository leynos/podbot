//! Unit tests for credential upload planning and archive generation.

mod tar_archive;
mod upload_flow;

use std::io;
use std::sync::{Arc, Mutex};

use bollard::query_parameters::UploadToContainerOptions;
use camino::Utf8Path;
use cap_std::ambient_authority;
use cap_std::fs_utf8::Dir;
use mockall::mock;
use tempfile::TempDir;

use super::*;

mock! {
    #[derive(Debug)]
    pub Uploader {}

    impl ContainerUploader for Uploader {
        fn upload_to_container(
            &self,
            container_id: &str,
            options: Option<UploadToContainerOptions>,
            archive_bytes: Vec<u8>,
        ) -> UploadToContainerFuture<'_>;
    }
}

#[derive(Debug, Clone, Default)]
struct CapturedUploadCall {
    call_count: usize,
    container_id: Option<String>,
    options: Option<UploadToContainerOptions>,
    archive_bytes: Vec<u8>,
}

fn uploader_with_result(
    result: Result<(), bollard::errors::Error>,
) -> (MockUploader, Arc<Mutex<CapturedUploadCall>>) {
    let mut uploader = MockUploader::new();
    let captured = Arc::new(Mutex::new(CapturedUploadCall::default()));
    let captured_for_closure = Arc::clone(&captured);

    let response_state = Arc::new(Mutex::new(Some(result)));
    let response_state_for_closure = Arc::clone(&response_state);

    uploader
        .expect_upload_to_container()
        .returning(move |container_id, options, archive_bytes| {
            {
                let mut captured_lock = captured_for_closure
                    .lock()
                    .expect("capture lock should succeed");
                captured_lock.call_count += 1;
                captured_lock.container_id = Some(String::from(container_id));
                captured_lock.options = options;
                captured_lock.archive_bytes = archive_bytes;
            }

            let response = response_state_for_closure
                .lock()
                .expect("response lock should succeed")
                .take()
                .expect("mock response should be configured");

            Box::pin(async move { response })
        });

    (uploader, captured)
}

fn successful_uploader() -> (MockUploader, Arc<Mutex<CapturedUploadCall>>) {
    uploader_with_result(Ok(()))
}

fn failing_uploader(
    error: bollard::errors::Error,
) -> (MockUploader, Arc<Mutex<CapturedUploadCall>>) {
    uploader_with_result(Err(error))
}

fn captured_call(captured: &Arc<Mutex<CapturedUploadCall>>) -> CapturedUploadCall {
    captured
        .lock()
        .expect("capture lock should succeed")
        .clone()
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Runtime::new().expect("runtime creation should succeed")
}

fn host_home_dir() -> (TempDir, Utf8PathBuf) {
    let temp_dir = tempfile::tempdir().expect("tempdir creation should succeed");
    let utf8_path = Utf8PathBuf::from_path_buf(temp_dir.path().to_path_buf())
        .expect("tempdir path should be valid UTF-8");

    (temp_dir, utf8_path)
}

fn create_dir(path: &Utf8Path) {
    Dir::create_ambient_dir_all(path, ambient_authority())
        .expect("directory creation should succeed");
}

fn write_file(path: &Utf8Path, contents: &str) {
    let parent = path
        .parent()
        .expect("file path should include a parent directory");
    let file_name = path
        .file_name()
        .expect("file path should include a file name");
    let parent_dir = Dir::open_ambient_dir(parent, ambient_authority())
        .expect("parent directory should be openable");
    parent_dir
        .write(file_name, contents)
        .expect("file write should succeed");
}

#[cfg(unix)]
fn set_mode(path: &Utf8Path, mode: u32) {
    use cap_std::fs::PermissionsExt;

    let parent = path
        .parent()
        .expect("path should include a parent directory");
    let file_name = path
        .file_name()
        .expect("path should include a final component");
    let parent_dir = Dir::open_ambient_dir(parent, ambient_authority())
        .expect("parent directory should be openable");
    let permissions = cap_std::fs::Permissions::from_mode(mode);
    parent_dir
        .set_permissions(file_name, permissions)
        .expect("setting permissions should succeed");
}

#[cfg(not(unix))]
fn set_mode(_path: &Utf8Path, _mode: u32) {}

fn io_error(message: impl Into<String>) -> io::Error {
    io::Error::other(message.into())
}
