use {
    crate::{bind_common_in_range_with_config, bind_common_with_config, PortRange, SocketConfig},
    std::{
        net::{IpAddr, SocketAddr, TcpListener, UdpSocket},
        sync::atomic::{AtomicU16, Ordering},
    },
};
// base port for deconflicted allocations
const BASE_PORT: u16 = 5000;
// how much to allocate per individual process.
// we expect to have at most 64 concurrent tests in CI at any moment on a given host.
const SLICE_PER_PROCESS: u16 = (u16::MAX - BASE_PORT) / 64;
/// Retrieve a free 20-port slice for unit tests
///
/// When running under nextest, this will try to provide
/// a unique slice of port numbers (assuming no other nextest processes
/// are running on the same host) based on NEXTEST_TEST_GLOBAL_SLOT variable
/// The port ranges will be reused following nextest logic.
///
/// When running without nextest, this will only bump an atomic and eventually
/// panic when it runs out of port numbers to assign.
pub fn localhost_port_range_for_tests() -> (u16, u16) {
    static SLICE: AtomicU16 = AtomicU16::new(0);
    let offset = SLICE.fetch_add(20, Ordering::Relaxed);
    
    let start = match std::env::var("NEXTEST_TEST_GLOBAL_SLOT") {
        Ok(slot) => {
            let slot: u16 = slot.parse().unwrap();
            assert!(
                offset < SLICE_PER_PROCESS,
                "Overrunning into the port range of another test! Consider using fewer ports per test."
            );
            let slot_offset = slot.saturating_mul(SLICE_PER_PROCESS);
            BASE_PORT.saturating_add(slot_offset).saturating_add(offset)
        }
        Err(_) => BASE_PORT.saturating_add(offset),
    };
    
    assert!(start <= u16::MAX - 20, "ran out of port numbers!");
    (start, start.saturating_add(20))
}

pub fn bind_gossip_port_in_range(
    gossip_addr: &SocketAddr,
    port_range: PortRange,
    bind_ip_addr: IpAddr,
) -> (u16, (UdpSocket, TcpListener)) {
    let config = SocketConfig::default();
    if gossip_addr.port() != 0 {
        (
            gossip_addr.port(),
            bind_common_with_config(bind_ip_addr, gossip_addr.port(), config).unwrap_or_else(|e| {
                panic!("gossip_addr bind_to port {}: {}", gossip_addr.port(), e)
            }),
        )
    } else {
        bind_common_in_range_with_config(bind_ip_addr, port_range, config).expect("Failed to bind")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_localhost_port_range() {
        // Test without nextest
        env::remove_var("NEXTEST_TEST_GLOBAL_SLOT");
        
        let test_cases = vec![
            localhost_port_range_for_tests(),
            localhost_port_range_for_tests(),
        ];

        for window in test_cases.windows(2) {
            let (start, end) = window[0];
            assert_eq!(end - start, 20);
            assert!(start >= BASE_PORT);

            let (next_start, next_end) = window[1];
            assert_eq!(next_end - next_start, 20);
            assert!(next_start > start);
            assert_eq!(next_start, start + 20);
        }

        // Test with nextest
        env::set_var("NEXTEST_TEST_GLOBAL_SLOT", "2");
        
        let test_cases = vec![
            localhost_port_range_for_tests(),
            localhost_port_range_for_tests(),
        ];

        for window in test_cases.windows(2) {
            let (start, end) = window[0];
            assert_eq!(end - start, 20);
            // Don't check the exact value, as the SLICE counter has already been incremented
            assert!(start >= BASE_PORT + 2 * SLICE_PER_PROCESS);

            let (next_start, next_end) = window[1];
            assert_eq!(next_end - next_start, 20);
            assert!(next_start > start);
            assert_eq!(next_start, start + 20);
        }

        env::remove_var("NEXTEST_TEST_GLOBAL_SLOT");
    }
}