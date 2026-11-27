#![cfg(target_os = "linux")]

//! Linux Pseudo-Terminal (PTY) integration tests for `SerialObdTransport`.
//!
//! Exercises real OS character-device stream behavior, chunked response framing,
//! lifecycle state guards, disconnect EOF/error handling, and timeout fault containment
//! without requiring physical OBD-II adapter hardware.

use kivi_protocol::obd::adapter::parse_adapter_response;
use kivi_protocol::obd::command::{AdapterCommand, Mode01Pid, ObdCommand};
use kivi_protocol::obd::errors::ObdCodecError;
use kivi_protocol::obd::framing::SerialBaudRate;
use kivi_protocol::obd::serial::SerialObdTransport;
use kivi_protocol::obd::session::{ObdSession, ObdSessionConfig, ObdSessionState};
use kivi_protocol::obd::transport::ObdTransport;
use std::time::Duration;

/// Deterministic Linux PTY test fixture managing master and slave descriptors.
struct PtyFixture {
    master_fd: libc::c_int,
    slave_path: String,
}

impl PtyFixture {
    /// Creates a new POSIX pseudo-terminal pair, extracts the slave character device path,
    /// configures the master descriptor for non-blocking I/O, and immediately closes the
    /// initial slave file descriptor so that the transport under test is the sole owner
    /// of opened slave descriptors.
    fn new() -> Self {
        let mut master: libc::c_int = -1;
        let mut slave: libc::c_int = -1;
        let mut name_buf = [0 as libc::c_char; 4096];

        let ret = unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                name_buf.as_mut_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        assert_eq!(ret, 0, "libc::openpty failed to create pseudo-terminal");

        let slave_path = unsafe {
            std::ffi::CStr::from_ptr(name_buf.as_ptr())
                .to_str()
                .expect("valid UTF-8 slave path from openpty")
                .to_string()
        };

        // Close slave descriptor immediately in fixture so that only the SerialObdTransport
        // will hold an open slave descriptor upon open().
        let close_res = unsafe { libc::close(slave) };
        assert_eq!(close_res, 0, "failed to close initial PTY slave descriptor");

        // Set master descriptor to non-blocking mode to prevent deadlocking test runners
        unsafe {
            let flags = libc::fcntl(master, libc::F_GETFL, 0);
            assert!(flags >= 0, "fcntl F_GETFL failed on PTY master");
            let set_res = libc::fcntl(master, libc::F_SETFL, flags | libc::O_NONBLOCK);
            assert_eq!(set_res, 0, "fcntl F_SETFL failed on PTY master");
        }

        Self {
            master_fd: master,
            slave_path,
        }
    }

    fn slave_path(&self) -> &str {
        &self.slave_path
    }

    /// Closes the master descriptor, severing the PTY link to simulate peer disconnection.
    fn close_master(&mut self) {
        if self.master_fd >= 0 {
            unsafe {
                libc::close(self.master_fd);
            }
            self.master_fd = -1;
        }
    }

    /// Default finite deadline for fixture writes to the PTY master descriptor (500 ms).
    const DEFAULT_WRITE_TIMEOUT: Duration = Duration::from_millis(500);

    /// Writes all bytes to the master descriptor within the specified monotonic timeout deadline.
    ///
    /// - Uses a single absolute monotonic deadline (`Instant::now() + timeout`) covering the complete operation.
    /// - Accurately tracks partial writes across iterations.
    /// - Handles `Interrupted` (`EINTR`) safely without advancing write progress or resetting the deadline.
    /// - If `write` returns `WouldBlock` (`EAGAIN`), uses `libc::poll` on `POLLOUT` up to the remaining
    ///   deadline to avoid busy-spinning or arbitrary fixed sleeps.
    /// - Panics with an actionable error message if the deadline expires before all bytes are written.
    fn write_all_timeout(&self, data: &[u8], timeout: Duration) {
        assert!(
            self.master_fd >= 0,
            "attempted write to closed PTY master descriptor"
        );
        let deadline = std::time::Instant::now() + timeout;
        let mut written = 0;

        while written < data.len() {
            let now = std::time::Instant::now();
            if now >= deadline {
                panic!(
                    "Timed out writing to PTY master descriptor after {:?}: wrote {} of {} bytes",
                    timeout,
                    written,
                    data.len()
                );
            }

            let n = unsafe {
                libc::write(
                    self.master_fd,
                    data[written..].as_ptr() as *const libc::c_void,
                    data.len() - written,
                )
            };

            if n > 0 {
                written += n as usize;
            } else if n < 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() == std::io::ErrorKind::Interrupted {
                    // Safe retry on signal interruption without advancing index or resetting deadline
                    continue;
                } else if err.kind() == std::io::ErrorKind::WouldBlock {
                    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                    if remaining.is_zero() {
                        panic!(
                            "Timed out writing to PTY master descriptor after {:?}: wrote {} of {} bytes (buffer saturated)",
                            timeout,
                            written,
                            data.len()
                        );
                    }

                    let mut pfd = libc::pollfd {
                        fd: self.master_fd,
                        events: libc::POLLOUT,
                        revents: 0,
                    };
                    let poll_timeout_ms =
                        remaining.as_millis().min(i32::MAX as u128).max(1) as libc::c_int;

                    let poll_res = unsafe { libc::poll(&mut pfd, 1, poll_timeout_ms) };
                    if poll_res < 0 {
                        let poll_err = std::io::Error::last_os_error();
                        if poll_err.kind() == std::io::ErrorKind::Interrupted {
                            continue;
                        }
                        panic!("poll for PTY master write readiness failed: {}", poll_err);
                    } else if poll_res == 0 {
                        panic!(
                            "Timed out waiting for PTY master write readiness after {:?}: wrote {} of {} bytes",
                            timeout,
                            written,
                            data.len()
                        );
                    }

                    // Check for exceptional error, hangup, or invalid descriptor states
                    if pfd.revents & libc::POLLNVAL != 0 {
                        panic!("PTY master descriptor is invalid (POLLNVAL)");
                    }
                    if pfd.revents & libc::POLLERR != 0 {
                        panic!("PTY master descriptor reported device error (POLLERR)");
                    }
                    if pfd.revents & libc::POLLHUP != 0 {
                        panic!("PTY master connection hung up (POLLHUP)");
                    }
                    if pfd.revents & libc::POLLOUT == 0 {
                        // revents did not signal write readiness; continue polling until deadline
                        continue;
                    }
                } else {
                    panic!("write to PTY master failed: {}", err);
                }
            } else {
                panic!("unexpected 0-byte write to PTY master");
            }
        }
    }

    /// Writes all bytes to the master descriptor within the default 500 ms deadline.
    fn write_all(&self, data: &[u8]) {
        self.write_all_timeout(data, Self::DEFAULT_WRITE_TIMEOUT);
    }

    /// Asynchronously polls and reads from the master descriptor until the specified terminator
    /// is observed or the bounded deadline expires.
    async fn read_until_terminator(&self, terminator: u8, timeout: Duration) -> Vec<u8> {
        let deadline = tokio::time::Instant::now() + timeout;
        let mut accumulated = Vec::new();
        let mut buf = [0u8; 128];

        loop {
            if accumulated.contains(&terminator) {
                return accumulated;
            }

            let now = tokio::time::Instant::now();
            if now >= deadline {
                panic!(
                    "Timed out waiting for terminator 0x{:02X} after {:?}. Received: {:?}",
                    terminator,
                    timeout,
                    String::from_utf8_lossy(&accumulated)
                );
            }

            let n = unsafe {
                libc::read(
                    self.master_fd,
                    buf.as_mut_ptr() as *mut libc::c_void,
                    buf.len(),
                )
            };

            if n > 0 {
                accumulated.extend_from_slice(&buf[..n as usize]);
            } else if n < 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                if err.kind() == std::io::ErrorKind::WouldBlock {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                    continue;
                }
                panic!("read from PTY master failed: {}", err);
            } else {
                return accumulated;
            }
        }
    }
}

impl Drop for PtyFixture {
    fn drop(&mut self) {
        if self.master_fd >= 0 {
            unsafe {
                libc::close(self.master_fd);
            }
            self.master_fd = -1;
        }
    }
}

/// T-2B-01: Open and basic byte-stream communication.
///
/// Verifies:
/// - `open()` connects successfully and reports `is_connected() == true`.
/// - Encoded typed commands (`ATZ\r`) are received on the PTY master with `\r` terminator.
/// - Framed responses ending in `>` prompt are correctly read and returned by the transport.
/// - `close()` tears down the stream and restores `is_connected() == false`.
#[tokio::test]
async fn test_pty_t2b01_open_and_basic_byte_stream_communication() {
    let fixture = PtyFixture::new();
    let mut transport = SerialObdTransport::new(fixture.slave_path(), SerialBaudRate::Baud115200)
        .with_settling_delay(Duration::ZERO);

    assert!(!transport.is_connected());
    transport
        .open()
        .await
        .expect("transport open failed on PTY");
    assert!(transport.is_connected());

    transport
        .write_command(ObdCommand::Adapter(AdapterCommand::Reset))
        .await
        .expect("write_command failed");

    let cmd = fixture
        .read_until_terminator(b'\r', Duration::from_millis(500))
        .await;
    assert_eq!(cmd, b"ATZ\r");

    fixture.write_all(b"ELM327 v1.5\r\r>");

    let response = transport
        .read_response(Duration::from_millis(500), 1024)
        .await
        .expect("read_response failed");
    assert_eq!(response, "ELM327 v1.5\r\r");

    transport.close().await.expect("transport close failed");
    assert!(!transport.is_connected());
}

/// T-2B-02: Response framing across multiple writes.
///
/// Verifies that responses delivered in multiple fragmented chunks across the OS PTY stream
/// are accumulated until the prompt delimiter `>` is encountered, and that the resulting
/// frame is parsed into valid diagnostic bytes.
#[tokio::test]
async fn test_pty_t2b02_chunked_response_framing() {
    let fixture = PtyFixture::new();
    let mut transport = SerialObdTransport::new(fixture.slave_path(), SerialBaudRate::Baud115200)
        .with_settling_delay(Duration::ZERO);

    transport
        .open()
        .await
        .expect("transport open failed on PTY");

    transport
        .write_command(ObdCommand::Mode01(Mode01Pid::EngineRpm))
        .await
        .expect("write_command failed");

    let cmd = fixture
        .read_until_terminator(b'\r', Duration::from_millis(500))
        .await;
    assert_eq!(cmd, b"01 0C\r");

    // Deliver response across three separate writes to test incremental buffer accumulation
    fixture.write_all(b"41 ");
    tokio::time::sleep(Duration::from_millis(10)).await;
    fixture.write_all(b"0C 1A ");
    tokio::time::sleep(Duration::from_millis(10)).await;
    fixture.write_all(b"F8\r\r>");

    let response = transport
        .read_response(Duration::from_millis(500), 1024)
        .await
        .expect("read_response failed");
    assert_eq!(response, "41 0C 1A F8\r\r");

    let payload = parse_adapter_response(&response).expect("adapter response parsing failed");
    assert_eq!(payload, vec![0x41, 0x0C, 0x1A, 0xF8]);

    transport.close().await.expect("transport close failed");
}

/// T-2B-03: Double-open rejection.
///
/// Verifies that calling `open()` on an already-open transport returns `InvalidSessionState`,
/// preserves the active underlying connection, and remains fully functional for subsequent I/O.
#[tokio::test]
async fn test_pty_t2b03_double_open_rejection() {
    let fixture = PtyFixture::new();
    let mut transport = SerialObdTransport::new(fixture.slave_path(), SerialBaudRate::Baud115200)
        .with_settling_delay(Duration::ZERO);

    transport
        .open()
        .await
        .expect("initial transport open failed");
    assert!(transport.is_connected());

    let second_open = transport.open().await;
    match second_open {
        Err(ObdCodecError::InvalidSessionState { current, expected }) => {
            assert_eq!(current, "Open");
            assert_eq!(expected, "Closed");
        }
        other => panic!("expected InvalidSessionState, got: {:?}", other),
    }

    // Existing connection must remain active and functional
    assert!(transport.is_connected());
    transport
        .write_command(ObdCommand::Adapter(AdapterCommand::EchoOff))
        .await
        .expect("write_command after rejected double-open failed");

    let cmd = fixture
        .read_until_terminator(b'\r', Duration::from_millis(500))
        .await;
    assert_eq!(cmd, b"ATE0\r");

    transport.close().await.expect("transport close failed");
    assert!(!transport.is_connected());
}

/// T-2B-04: Slave disconnect and EOF behavior.
///
/// Verifies that closing the master descriptor while reading from the PTY slave
/// cleanly yields either an unexpected EOF or an OS I/O error without hanging or crashing.
#[tokio::test]
async fn test_pty_t2b04_peer_disconnect_and_eof_behavior() {
    let mut fixture = PtyFixture::new();
    let mut transport = SerialObdTransport::new(fixture.slave_path(), SerialBaudRate::Baud115200)
        .with_settling_delay(Duration::ZERO);

    transport.open().await.expect("transport open failed");
    assert!(transport.is_connected());

    // Sever PTY connection by closing master
    fixture.close_master();

    let res = transport
        .read_response(Duration::from_millis(200), 1024)
        .await;
    match res {
        Err(ObdCodecError::UnexpectedEof) | Err(ObdCodecError::IoError(_)) => {}
        other => panic!(
            "expected UnexpectedEof or IoError on severed PTY connection, got: {:?}",
            other
        ),
    }

    transport
        .close()
        .await
        .expect("close after peer disconnect failed");
    assert!(!transport.is_connected());
}

/// T-2B-05: Failed open leaves transport disconnected.
///
/// Verifies that attempting to open a destroyed or inaccessible PTY slave path fails closed,
/// leaves `is_connected() == false`, and enforces closed-state guards on subsequent read/write calls.
#[tokio::test]
async fn test_pty_t2b05_failed_open_leaves_transport_disconnected() {
    let mut fixture = PtyFixture::new();
    let slave_path = fixture.slave_path().to_string();

    // Closing the master destroys the devpts node, guaranteeing open() fails
    fixture.close_master();

    let mut transport = SerialObdTransport::new(&slave_path, SerialBaudRate::Baud115200)
        .with_settling_delay(Duration::ZERO);

    let open_res = transport.open().await;
    match open_res {
        Err(ObdCodecError::IoError(err_msg)) => {
            assert!(
                err_msg.contains("failed to open serial port"),
                "expected open failure, got: {}",
                err_msg
            );
        }
        other => panic!("expected IoError on destroyed PTY path, got: {:?}", other),
    }

    assert!(!transport.is_connected());

    let write_res = transport
        .write_command(ObdCommand::Adapter(AdapterCommand::Reset))
        .await;
    assert!(matches!(
        write_res,
        Err(ObdCodecError::InvalidSessionState { .. })
    ));

    let read_res = transport
        .read_response(Duration::from_millis(50), 1024)
        .await;
    assert!(matches!(
        read_res,
        Err(ObdCodecError::InvalidSessionState { .. })
    ));
}

/// T-2B-06: Timeout and session fault containment.
///
/// Verifies:
/// - Unanswered commands time out within the configured transaction timeout deadline.
/// - Transaction timeout transitions `ObdSession` into `Faulted` state.
/// - Subsequent commands on a faulted session fail immediately without issuing I/O.
/// - Standalone `read_response` on `SerialObdTransport` enforces bounded timeout deadlines.
#[tokio::test]
async fn test_pty_t2b06_timeout_and_session_fault_containment() {
    let fixture = PtyFixture::new();
    let transport = SerialObdTransport::new(fixture.slave_path(), SerialBaudRate::Baud115200)
        .with_settling_delay(Duration::ZERO);

    let config = ObdSessionConfig {
        command_timeout: Duration::from_millis(60),
        ..Default::default()
    };
    let mut session = ObdSession::new(transport, config);

    session.open().await.expect("session open failed");
    assert_eq!(*session.state(), ObdSessionState::Initializing);

    // initialize() sends ATZ\r; fixture emits no response, causing transaction timeout
    let init_res = session.initialize().await;
    match init_res {
        Err(ObdCodecError::Timeout(ms)) => {
            assert_eq!(ms, 60);
        }
        other => panic!("expected Timeout(60), got: {:?}", other),
    }

    // Session must be in Faulted state
    match session.state() {
        ObdSessionState::Faulted { reason } => {
            assert!(
                reason.contains("timed out") || reason.contains("timeout"),
                "unexpected fault reason: {}",
                reason
            );
        }
        other => panic!("expected Faulted state, got: {:?}", other),
    }

    // Subsequent commands on faulted session must fail immediately
    let subsequent_res = session
        .execute_command(ObdCommand::Mode01(Mode01Pid::EngineRpm))
        .await;
    assert!(matches!(
        subsequent_res,
        Err(ObdCodecError::SessionFaulted(_))
    ));

    // Master should only have received the initial ATZ\r command
    let received_cmd = fixture
        .read_until_terminator(b'\r', Duration::from_millis(100))
        .await;
    assert_eq!(received_cmd, b"ATZ\r");

    session.close().await.expect("session close failed");

    // Standalone direct transport timeout check
    let direct_fixture = PtyFixture::new();
    let mut direct_transport =
        SerialObdTransport::new(direct_fixture.slave_path(), SerialBaudRate::Baud115200)
            .with_settling_delay(Duration::ZERO);
    direct_transport.open().await.expect("direct open failed");
    direct_transport
        .write_command(ObdCommand::Adapter(AdapterCommand::Reset))
        .await
        .expect("direct write failed");

    let timeout_res = direct_transport
        .read_response(Duration::from_millis(50), 1024)
        .await;
    match timeout_res {
        Err(ObdCodecError::Timeout(ms)) => {
            assert_eq!(ms, 50);
        }
        other => panic!("expected direct Timeout(50), got: {:?}", other),
    }

    direct_transport.close().await.expect("direct close failed");
}
