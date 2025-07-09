#![cfg(target_os = "linux")]

use {
    solana_net_utils::sockets::{bind_to, localhost_port_range_for_tests},
    solana_streamer::{
        packet::{Meta, Packet, PACKET_DATA_SIZE},
        recvmmsg::*,
    },
    std::{net::Ipv4Addr, time::Instant},
};

#[test]
pub fn test_recv_mmsg_batch_size() {
    let port_range = localhost_port_range_for_tests();
    let mut port_range = port_range.0..port_range.1;
    let reader_socket = bind_to(
        std::net::IpAddr::V4(Ipv4Addr::LOCALHOST),
        port_range.next().unwrap(),
    )
    .expect("should bind reader");
    let addr = reader_socket.local_addr().unwrap();
    let sender_socket = bind_to(
        std::net::IpAddr::V4(Ipv4Addr::LOCALHOST),
        port_range.next().unwrap(),
    )
    .expect("should bind sender");

    const TEST_BATCH_SIZE: usize = 64;
    let sent = TEST_BATCH_SIZE;

    let mut elapsed_in_max_batch = 0;
    let mut num_max_batches = 0;
    (0..1000).for_each(|_| {
        for _ in 0..sent {
            let data = [0; PACKET_DATA_SIZE];
            sender_socket.send_to(&data[..], addr).unwrap();
        }
        let mut packets = vec![Packet::default(); TEST_BATCH_SIZE];
        let now = Instant::now();
        let recv = recv_mmsg(&reader_socket, &mut packets[..]).unwrap();
        elapsed_in_max_batch += now.elapsed().as_nanos();
        if recv == TEST_BATCH_SIZE {
            num_max_batches += 1;
        }
    });
    assert!(num_max_batches > 990);

    let mut elapsed_in_small_batch = 0;
    (0..1000).for_each(|_| {
        for _ in 0..sent {
            let data = [0; PACKET_DATA_SIZE];
            sender_socket.send_to(&data[..], addr).unwrap();
        }
        let mut packets = vec![Packet::default(); 4];
        let mut recv = 0;
        let now = Instant::now();
        while let Ok(num) = recv_mmsg(&reader_socket, &mut packets[..]) {
            recv += num;
            if recv >= TEST_BATCH_SIZE {
                break;
            }
            packets
                .iter_mut()
                .for_each(|pkt| *pkt.meta_mut() = Meta::default());
        }
        elapsed_in_small_batch += now.elapsed().as_nanos();
        assert_eq!(TEST_BATCH_SIZE, recv);
    });

    assert!(elapsed_in_max_batch <= elapsed_in_small_batch);
}
