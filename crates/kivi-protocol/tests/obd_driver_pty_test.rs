//! Linux PTY loopback integration test for [`Obd2Driver<SerialObdTransport>`].
//!
//! Validates end-to-end integration of the read-only driver over real OS character-device
//! streams, testing live telemetry acquisition and graceful termination upon peer disconnect.
//!
//! Note on Emulation Boundaries:
//! Passing Linux PTY tests exercises character-device stream buffering, asynchronous driver
//! scheduling, and POSIX EOF detection. It does NOT prove physical USB bridge FIFO synchronization,
//! adapter microcontroller firmware behavior, baud rate clock accuracy, or real vehicle CAN bus safety.

#![cfg(unix)]

use kivi_protocol::driver::{EcuDriver, TelemetrySource};
use kivi_protocol::model::ConnectionState;
use kivi_protocol::obd::driver::{Obd2Driver, Obd2DriverConfig};
use kivi_protocol::obd::framing::SerialBaudRate;
use kivi_protocol::obd::serial::SerialObdTransport;
use std::time::Duration;

struct PtyFixture {
    master_fd: libc::c_int,
    slave_path: String,
}

impl PtyFixture {
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
                .expect("valid UTF-8 slave path")
                .to_string()
        };

        // Close slave descriptor immediately in fixture so SerialObdTransport owns the slave
        let close_res = unsafe { libc::close(slave) };
        assert_eq!(close_res, 0, "failed to close initial PTY slave descriptor");

        // Set master descriptor to non-blocking mode
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

    fn close_master(&mut self) {
        if self.master_fd >= 0 {
            unsafe {
                libc::close(self.master_fd);
            }
            self.master_fd = -1;
        }
    }

    fn write_all_timeout(&self, data: &[u8], timeout: Duration) {
        assert!(self.master_fd >= 0, "write to closed PTY master descriptor");
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
                    continue;
                } else if err.kind() == std::io::ErrorKind::WouldBlock {
                    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                    if remaining.is_zero() {
                        panic!("Timed out writing to PTY master descriptor");
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
                        panic!("poll for PTY master write failed: {}", poll_err);
                    } else if poll_res == 0 {
                        panic!("poll timed out waiting for PTY master write readiness");
                    }
                    if pfd.revents & libc::POLLNVAL != 0 || pfd.revents & libc::POLLERR != 0 {
                        panic!("PTY master descriptor reported error or invalid state");
                    }
                } else {
                    panic!("write to PTY master failed: {}", err);
                }
            }
        }
    }

    async fn read_until_terminator(&self, terminator: u8, timeout: Duration) -> Vec<u8> {
        let deadline = tokio::time::Instant::now() + timeout;
        let mut accumulated = Vec::new();
        let mut buf = [0u8; 128];

        loop {
            if accumulated.contains(&terminator) {
                return accumulated;
            }
            if tokio::time::Instant::now() >= deadline {
                panic!("Timed out waiting for terminator 0x{:02X}", terminator);
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
                } else if err.kind() == std::io::ErrorKind::WouldBlock {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                } else {
                    panic!("read from PTY master failed: {}", err);
                }
            } else {
                panic!("unexpected EOF on PTY master during test");
            }
        }
    }
}

impl Drop for PtyFixture {
    fn drop(&mut self) {
        self.close_master();
    }
}

#[tokio::test]
async fn test_pty_obd2_driver_streaming_and_disconnect() {
    let mut pty = PtyFixture::new();
    let slave_path = pty.slave_path().to_string();

    let transport = SerialObdTransport::new(slave_path, SerialBaudRate::Baud115200)
        .with_settling_delay(Duration::ZERO);

    let config = Obd2DriverConfig {
        poll_interval: Duration::from_millis(20),
        stoichiometric_ratio: None,
        polled_pids: vec![kivi_protocol::obd::command::Mode01Pid::EngineRpm],
        command_timeout: Duration::from_millis(500),
        query_vin: false,
        protocol: Default::default(),
        baud_rate: SerialBaudRate::Baud115200,
    };

    let driver = Obd2Driver::new(transport, config);
    let mut rx = driver.subscribe_telemetry();

    // Spawn adapter simulator task on master descriptor
    let master_task = tokio::spawn(async move {
        // 1. Handshake: ATZ
        pty.read_until_terminator(b'\r', Duration::from_secs(1))
            .await;
        pty.write_all_timeout(b"ELM327 v1.5\r\r>", Duration::from_millis(500));

        // 2. ATE0
        pty.read_until_terminator(b'\r', Duration::from_secs(1))
            .await;
        pty.write_all_timeout(b"OK\r\r>", Duration::from_millis(500));

        // 3. ATL0
        pty.read_until_terminator(b'\r', Duration::from_secs(1))
            .await;
        pty.write_all_timeout(b"OK\r\r>", Duration::from_millis(500));

        // 4. ATH0
        pty.read_until_terminator(b'\r', Duration::from_secs(1))
            .await;
        pty.write_all_timeout(b"OK\r\r>", Duration::from_millis(500));

        // 5. ATS1
        pty.read_until_terminator(b'\r', Duration::from_secs(1))
            .await;
        pty.write_all_timeout(b"OK\r\r>", Duration::from_millis(500));

        // 6. ATSP6
        pty.read_until_terminator(b'\r', Duration::from_secs(1))
            .await;
        pty.write_all_timeout(b"OK\r\r>", Duration::from_millis(500));

        // 7. ATI
        pty.read_until_terminator(b'\r', Duration::from_secs(1))
            .await;
        pty.write_all_timeout(b"ELM327 v1.5\r\r>", Duration::from_millis(500));

        // 8. STDI
        pty.read_until_terminator(b'\r', Duration::from_secs(1))
            .await;
        pty.write_all_timeout(b"?\r\r>", Duration::from_millis(500));

        // 9. Supported PIDs 01 00
        pty.read_until_terminator(b'\r', Duration::from_secs(1))
            .await;
        pty.write_all_timeout(b"41 00 00 10 00 00\r\r>", Duration::from_millis(500)); // RPM supported

        // 10. Live RPM query (01 0C)
        pty.read_until_terminator(b'\r', Duration::from_secs(1))
            .await;
        pty.write_all_timeout(b"41 0C 1A F8\r\r>", Duration::from_millis(500)); // 1726 RPM

        // Allow driver background loop to ingest and broadcast the frame
        tokio::time::sleep(Duration::from_millis(100)).await;

        // 11. Simulate physical peer disconnect / bridge unplug
        pty.close_master();
    });

    // Connect driver over PTY slave
    let identity = driver.connect().await.expect("PTY connect failed");
    assert_eq!(identity.ecu_id, "ELM327 v1.5");

    // Await live telemetry frame streamed across real PTY character device
    let frame = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("timeout waiting for PTY telemetry frame")
        .expect("channel error");

    assert!(!frame.is_simulated);
    assert!((frame.rpm - 1726.0).abs() < 1.0);

    // Wait for the simulator task to finish and severed PTY to propagate
    master_task.await.expect("master task error");

    // Allow time for the driver polling loop to observe the EOF/disconnect
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Driver must observe disconnect or fault and cleanly halt
    let state = driver.connection_state();
    assert!(
        matches!(
            state,
            ConnectionState::Faulted { .. } | ConnectionState::Disconnected
        ),
        "Expected Faulted or Disconnected state after PTY disconnect, got {:?}",
        state
    );

    // Teardown should not hang
    driver.disconnect().await.expect("disconnect failed");
    assert_eq!(driver.connection_state(), ConnectionState::Disconnected);
}
