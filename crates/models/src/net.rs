use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use reqwest::{
    StatusCode, Url,
    blocking::{Client, Response},
    header::{ACCEPT_ENCODING, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_RANGE, LOCATION, RANGE},
    redirect::Policy,
};

use crate::{Artifact, Error, Result};

pub(crate) fn validate_url(url: &str, allowed_hosts: &[String]) -> Result<()> {
    checked_url(url, allowed_hosts).map(|_| ())
}

fn checked_url(value: &str, allowed_hosts: &[String]) -> Result<Url> {
    let url = Url::parse(value).map_err(|_| network("Die Download-Adresse ist ungültig."))?;
    let host = url
        .host_str()
        .ok_or_else(|| network("Die Download-Adresse enthält keinen Host."))?;
    if !allowed_hosts.iter().any(|allowed| allowed == host) {
        return Err(Error::HostNotAllowed(host.into()));
    }
    let insecure_test_server = cfg!(test) && url.scheme() == "http" && host == "127.0.0.1";
    if ((!insecure_test_server)
        && (url.scheme() != "https" || url.port_or_known_default() != Some(443)))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(network("Die Download-Adresse ist nicht zulässig."));
    }
    Ok(url)
}

pub(crate) fn download(
    artifact: &Artifact,
    destination: &Path,
    allowed_hosts: &[String],
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64),
) -> Result<()> {
    check_cancelled(cancel)?;
    for url in &artifact.urls {
        validate_url(url, allowed_hosts)?;
    }
    let mut last_error = network("Es ist keine Download-Adresse vorhanden.");
    let existing = partial_size(destination)?;
    if existing == artifact.size {
        progress(existing);
        return check_cancelled(cancel);
    }
    let client = Client::builder()
        .redirect(Policy::none())
        .referer(false)
        .no_proxy()
        .connect_timeout(Duration::from_secs(15))
        // Blocking Response::read applies this timeout separately to each read.
        // Do not set RequestBuilder::timeout: that would also impose a total deadline.
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| network("Der Download-Client konnte nicht gestartet werden."))?;
    for url in &artifact.urls {
        check_cancelled(cancel)?;
        match download_one(
            &client,
            url,
            artifact.size,
            destination,
            allowed_hosts,
            cancel,
            progress,
        ) {
            Ok(()) => return Ok(()),
            Err(error @ Error::Network(_)) => last_error = error,
            Err(error) => return Err(error),
        }
    }
    Err(last_error)
}

fn download_one(
    client: &Client,
    url: &str,
    expected_size: u64,
    destination: &Path,
    allowed_hosts: &[String],
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64),
) -> Result<()> {
    let mut offset = partial_size(destination)?;
    if offset > expected_size {
        fs::remove_file(destination)?;
        offset = 0;
    }
    progress(offset);
    check_cancelled(cancel)?;
    let mut response = request(client, url, offset, allowed_hosts, cancel)?;
    let status = response.status();
    if status == StatusCode::PARTIAL_CONTENT {
        validate_range(&response, offset, expected_size)?;
    } else if status == StatusCode::OK {
        offset = 0;
    } else {
        return Err(network(&format!(
            "Der Download-Server meldet HTTP {}.",
            status.as_u16()
        )));
    }
    if response
        .headers()
        .get(CONTENT_ENCODING)
        .is_some_and(|value| value != "identity")
    {
        return Err(network(
            "Der Server hat die Download-Daten unerwartet komprimiert.",
        ));
    }
    if let Some(length) = response.headers().get(CONTENT_LENGTH) {
        let length = length
            .to_str()
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or_else(|| network("Die Download-Längenangabe ist ungültig."))?;
        if length != expected_size - offset {
            return Err(network(
                "Die Download-Größe stimmt nicht mit dem Manifest überein.",
            ));
        }
    }
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(offset == 0)
        .append(offset != 0)
        .open(destination)?;
    progress(offset);
    let mut received = offset;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        check_cancelled(cancel)?;
        let read = match response.read(&mut buffer) {
            Ok(read) => read,
            Err(_) => {
                check_cancelled(cancel)?;
                return Err(network(
                    "Der Download wurde unterbrochen; er kann fortgesetzt werden.",
                ));
            }
        };
        if read == 0 {
            break;
        }
        if read as u64 > expected_size - received {
            return Err(network(
                "Der Download enthält mehr Daten als im Manifest angegeben.",
            ));
        }
        file.write_all(&buffer[..read])?;
        received += read as u64;
        progress(received);
    }
    check_cancelled(cancel)?;
    if received != expected_size {
        return Err(network(
            "Der Download ist unvollständig; er kann fortgesetzt werden.",
        ));
    }
    file.sync_all()?;
    Ok(())
}

fn request(
    client: &Client,
    value: &str,
    offset: u64,
    allowed_hosts: &[String],
    cancel: &AtomicBool,
) -> Result<Response> {
    let mut url = checked_url(value, allowed_hosts)?;
    for hop in 0..=10 {
        check_cancelled(cancel)?;
        let mut request = client.get(url.clone()).header(ACCEPT_ENCODING, "identity");
        if offset != 0 {
            request = request.header(RANGE, format!("bytes={offset}-"));
        }
        let response = request.send().map_err(|_| {
            if cancel.load(Ordering::Relaxed) {
                Error::Cancelled
            } else {
                network("Der Download-Server ist nicht erreichbar.")
            }
        })?;
        check_cancelled(cancel)?;
        if !matches!(response.status().as_u16(), 301 | 302 | 303 | 307 | 308) {
            return Ok(response);
        }
        if hop == 10 {
            return Err(network("Der Download enthält zu viele Weiterleitungen."));
        }
        let location = response
            .headers()
            .get(LOCATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| network("Die Download-Weiterleitung ist ungültig."))?;
        let target = url
            .join(location)
            .map_err(|_| network("Die Download-Weiterleitung ist ungültig."))?;
        url = checked_url(target.as_str(), allowed_hosts)?;
    }
    unreachable!("the redirect limit always returns")
}

fn validate_range(response: &Response, offset: u64, expected_size: u64) -> Result<()> {
    let range = response
        .headers()
        .get(CONTENT_RANGE)
        .and_then(|value| value.to_str().ok());
    let expected = format!(
        "bytes {offset}-{}/{}",
        expected_size.saturating_sub(1),
        expected_size
    );
    if expected_size == 0 || range != Some(expected.as_str()) {
        return Err(network(
            "Der Server hat einen unpassenden Download-Bereich geliefert.",
        ));
    }
    Ok(())
}

fn partial_size(destination: &Path) -> Result<u64> {
    match fs::metadata(destination) {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error.into()),
    }
}

fn check_cancelled(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}

fn network(message: &str) -> Error {
    Error::Network(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ArchiveFormat, Error, Manager, Manifest, Model, ModelFile, ModelKind, ProgressPhase,
    };
    use sha2::{Digest, Sha256};
    use std::{
        fs,
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        sync::atomic::Ordering,
        thread::{self, JoinHandle},
        time::{Duration, Instant},
    };

    struct Server {
        url: String,
        worker: JoinHandle<Vec<String>>,
    }

    impl Server {
        fn new(responses: Vec<Vec<u8>>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let url = format!("http://{}/model", listener.local_addr().unwrap());
            let worker = thread::spawn(move || {
                let mut requests = Vec::new();
                for response in responses {
                    let deadline = Instant::now() + Duration::from_secs(10);
                    let mut connection = loop {
                        match listener.accept() {
                            Ok((connection, _)) => break connection,
                            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                                assert!(Instant::now() < deadline, "test server timed out");
                                thread::sleep(Duration::from_millis(5));
                            }
                            Err(error) => panic!("test server: {error}"),
                        }
                    };
                    // Accepted sockets inherit nonblocking mode on Windows.
                    connection.set_nonblocking(false).unwrap();
                    connection
                        .set_write_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    requests.push(read_request(&mut connection));
                    let _ = connection.write_all(&response);
                }
                requests
            });
            Self { url, worker }
        }

        fn join(self) -> Vec<String> {
            self.worker.join().unwrap()
        }
    }

    fn read_request(connection: &mut TcpStream) -> String {
        connection
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            connection.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
            assert!(request.len() < 16384);
        }
        String::from_utf8(request).unwrap()
    }

    fn response(status: &str, headers: &str, body: &[u8]) -> Vec<u8> {
        let mut value =
            format!("HTTP/1.1 {status}\r\nConnection: close\r\n{headers}\r\n").into_bytes();
        value.extend_from_slice(body);
        value
    }

    fn artifact(url: String, size: u64) -> Artifact {
        Artifact {
            urls: vec![url],
            archive: ArchiveFormat::None,
            sha256: "0".repeat(64),
            size,
            files: vec![],
        }
    }

    fn hosts() -> Vec<String> {
        vec!["127.0.0.1".into()]
    }

    fn manager(root: &Path, url: String, body: &[u8]) -> Manager {
        let mut artifact = artifact(url, body.len() as u64);
        artifact.sha256 = format!("{:x}", Sha256::digest(body));
        artifact.files = vec![ModelFile {
            path: "model.onnx".into(),
            archive_path: "model.onnx".into(),
            size: body.len() as u64,
        }];
        Manager::from_manifest(
            root.to_owned(),
            Manifest {
                version: 1,
                allowed_hosts: hosts(),
                models: vec![Model {
                    id: "test-model".into(),
                    display_name: "Testmodell".into(),
                    languages: vec!["de".into()],
                    kind: ModelKind::Asr,
                    license: "MIT".into(),
                    attribution: "Synthetische Testdaten".into(),
                    artifacts: vec![artifact],
                }],
            },
        )
        .unwrap()
    }

    #[test]
    fn manager_resumes_interrupted_install_and_publishes_verified_model() {
        let server = Server::new(vec![
            response("200 OK", "Content-Length: 10\r\n", b"abcd"),
            response(
                "206 Partial Content",
                "Content-Length: 6\r\nContent-Range: bytes 4-9/10\r\n",
                b"efghij",
            ),
        ]);
        let directory = tempfile::tempdir().unwrap();
        let manager = manager(directory.path(), server.url.clone(), b"abcdefghij");
        let cancel = AtomicBool::new(false);
        assert!(manager.install("test-model", &cancel, |_| {}).is_err());
        assert!(!manager.is_installed("test-model").unwrap());
        assert!(manager.path_of("test-model").unwrap().is_none());
        assert_eq!(
            fs::read(directory.path().join(".downloads/test-model/0.part")).unwrap(),
            b"abcd"
        );
        let mut phases = Vec::new();
        let installed = manager
            .install("test-model", &cancel, |progress| {
                phases.push(progress.phase)
            })
            .unwrap();
        assert_eq!(
            fs::read(installed.join("model.onnx")).unwrap(),
            b"abcdefghij"
        );
        assert_eq!(manager.path_of("test-model").unwrap(), Some(installed));
        assert!(manager.is_installed("test-model").unwrap());
        assert!(!directory.path().join(".downloads/test-model").exists());
        assert!(!directory.path().join(".staging/test-model").exists());
        assert!(phases.contains(&ProgressPhase::Verifying));
        assert_eq!(phases.last(), Some(&ProgressPhase::Complete));
        assert!(
            server.join()[1]
                .to_ascii_lowercase()
                .contains("range: bytes=4-")
        );
        manager.remove("test-model").unwrap();
        assert!(!manager.is_installed("test-model").unwrap());
    }

    #[test]
    fn manager_deletes_all_installation_data_after_bad_download_checksum() {
        let server = Server::new(vec![response(
            "200 OK",
            "Content-Length: 10\r\n",
            b"WRONG DATA",
        )]);
        let directory = tempfile::tempdir().unwrap();
        let manager = manager(directory.path(), server.url.clone(), b"abcdefghij");
        assert!(matches!(
            manager.install("test-model", &AtomicBool::new(false), |_| {}),
            Err(Error::ChecksumMismatch)
        ));
        for suffix in ["test-model", ".downloads/test-model", ".staging/test-model"] {
            assert!(!directory.path().join(suffix).exists());
        }
        assert!(!manager.is_installed("test-model").unwrap());
        server.join();
    }

    #[test]
    fn refuses_unlisted_hosts_and_insecure_or_credentialed_urls() {
        let allowed = vec!["example.com".into()];
        for url in [
            "https://example.com.evil.invalid/file",
            "https://evil.invalid/example.com",
            "http://example.com/file",
            "https://user:password@example.com/file",
            "https://example.com:8443/file",
        ] {
            assert!(validate_url(url, &allowed).is_err(), "{url}");
        }
        assert!(validate_url("https://example.com/file", &allowed).is_ok());
    }

    #[test]
    fn cancellation_happens_before_any_network_or_file_creation() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("download.part");
        let result = download(
            &artifact("https://unlisted.invalid/file".into(), 4),
            &destination,
            &[],
            &AtomicBool::new(true),
            &mut |_| {},
        );
        assert!(matches!(result, Err(Error::Cancelled)));
        assert!(!destination.exists());
    }

    #[test]
    fn interrupted_transfer_resumes_from_existing_bytes() {
        let server = Server::new(vec![
            response("200 OK", "Content-Length: 10\r\n", b"abcd"),
            response(
                "206 Partial Content",
                "Content-Length: 6\r\nContent-Range: bytes 4-9/10\r\n",
                b"efghij",
            ),
        ]);
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("download.part");
        let artifact = artifact(server.url.clone(), 10);
        let cancel = AtomicBool::new(false);
        assert!(download(&artifact, &destination, &hosts(), &cancel, &mut |_| {}).is_err());
        assert_eq!(fs::read(&destination).unwrap(), b"abcd");
        let mut progress = Vec::new();
        download(&artifact, &destination, &hosts(), &cancel, &mut |bytes| {
            progress.push(bytes);
        })
        .unwrap();
        assert_eq!(fs::read(destination).unwrap(), b"abcdefghij");
        assert_eq!(progress.first(), Some(&4));
        assert_eq!(progress.last(), Some(&10));
        let requests = server.join();
        assert!(!requests[0].to_ascii_lowercase().contains("range:"));
        assert!(requests[1].to_ascii_lowercase().contains("range: bytes=4-"));
    }

    #[test]
    fn cancellation_preserves_bytes_and_resumes_on_next_call() {
        let body = vec![b'x'; 256 * 1024];
        let first = Server::new(vec![response(
            "200 OK",
            &format!("Content-Length: {}\r\n", body.len()),
            &body,
        )]);
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("download.part");
        let cancel = AtomicBool::new(false);
        let result = download(
            &artifact(first.url.clone(), body.len() as u64),
            &destination,
            &hosts(),
            &cancel,
            &mut |bytes| {
                if bytes > 0 {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
        );
        assert!(matches!(result, Err(Error::Cancelled)));
        let offset = fs::metadata(&destination).unwrap().len();
        assert!(offset > 0 && offset < body.len() as u64);
        first.join();
        let second = Server::new(vec![response(
            "206 Partial Content",
            &format!(
                "Content-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\n",
                body.len() as u64 - offset,
                offset,
                body.len() - 1,
                body.len(),
            ),
            &body[offset as usize..],
        )]);
        cancel.store(false, Ordering::Relaxed);
        download(
            &artifact(second.url.clone(), body.len() as u64),
            &destination,
            &hosts(),
            &cancel,
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(fs::read(destination).unwrap(), body);
        let requests = second.join();
        assert!(requests[0].contains(&format!("bytes={offset}-")));
    }

    #[test]
    fn ignored_range_restarts_without_appending() {
        let server = Server::new(vec![response(
            "200 OK",
            "Content-Length: 10\r\n",
            b"abcdefghij",
        )]);
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("download.part");
        fs::write(&destination, b"abcd").unwrap();
        download(
            &artifact(server.url.clone(), 10),
            &destination,
            &hosts(),
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(fs::read(destination).unwrap(), b"abcdefghij");
        server.join();
    }

    #[test]
    fn malformed_range_never_appends_to_partial_file() {
        for range in ["bytes 3-8/10", "bytes 4-9/11", "bytes 4-8/10", "invalid"] {
            let server = Server::new(vec![response(
                "206 Partial Content",
                &format!("Content-Length: 6\r\nContent-Range: {range}\r\n"),
                b"efghij",
            )]);
            let directory = tempfile::tempdir().unwrap();
            let destination = directory.path().join("download.part");
            fs::write(&destination, b"abcd").unwrap();
            assert!(
                download(
                    &artifact(server.url.clone(), 10),
                    &destination,
                    &hosts(),
                    &AtomicBool::new(false),
                    &mut |_| {},
                )
                .is_err()
            );
            assert_eq!(fs::read(destination).unwrap(), b"abcd");
            server.join();
        }
    }

    #[test]
    fn redirects_are_checked_before_contacting_another_host() {
        let server = Server::new(vec![response(
            "302 Found",
            "Location: https://unlisted.invalid/model\r\nContent-Length: 0\r\n",
            b"",
        )]);
        let directory = tempfile::tempdir().unwrap();
        let result = download(
            &artifact(server.url.clone(), 4),
            &directory.path().join("download.part"),
            &hosts(),
            &AtomicBool::new(false),
            &mut |_| {},
        );
        assert!(matches!(result, Err(Error::HostNotAllowed(_))));
        server.join();
    }

    #[test]
    fn wrong_content_length_is_rejected_before_writing() {
        let server = Server::new(vec![response("200 OK", "Content-Length: 4\r\n", b"abcd")]);
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("download.part");
        assert!(
            download(
                &artifact(server.url.clone(), 10),
                &destination,
                &hosts(),
                &AtomicBool::new(false),
                &mut |_| {},
            )
            .is_err()
        );
        assert!(!destination.exists());
        server.join();
    }
}
