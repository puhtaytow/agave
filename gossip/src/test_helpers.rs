use {
    crate::cluster_info::{BindIpAddrs, Node, NodeConfig},
    solana_net_utils::{find_available_ports_in_range, sockets::localhost_port_range_for_tests},
    solana_pubkey::Pubkey,
    solana_streamer::quic::DEFAULT_QUIC_ENDPOINTS,
    std::{
        net::{IpAddr, Ipv4Addr, SocketAddr},
        num::NonZero,
    },
};

/// create localhost node for tests
pub fn new_localhost() -> Node {
    let pubkey = solana_pubkey::new_rand();
    new_localhost_with_pubkey(&pubkey)
}

/// create localhost node for tests with provided pubkey
/// unlike the [new_with_external_ip], this will also bind RPC sockets.
pub fn new_localhost_with_pubkey(pubkey: &Pubkey) -> Node {
    let port_range = localhost_port_range_for_tests();
    let bind_ip_addr = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let config = NodeConfig {
        bind_ip_addrs: BindIpAddrs::new(vec![bind_ip_addr]).expect("should bind"),
        gossip_port: port_range.0,
        port_range,
        advertised_ip: bind_ip_addr,
        public_tpu_addr: None,
        public_tpu_forwards_addr: None,
        num_tvu_receive_sockets: NonZero::new(1).unwrap(),
        num_tvu_retransmit_sockets: NonZero::new(1).unwrap(),
        num_quic_endpoints: NonZero::new(DEFAULT_QUIC_ENDPOINTS)
            .expect("Number of QUIC endpoints can not be zero"),
        vortexor_receiver_addr: None,
    };
    let mut node = Node::new_with_external_ip(pubkey, config);
    let rpc_ports: [u16; 2] = find_available_ports_in_range(bind_ip_addr, port_range).unwrap();
    let rpc_addr = SocketAddr::new(bind_ip_addr, rpc_ports[0]);
    let rpc_pubsub_addr = SocketAddr::new(bind_ip_addr, rpc_ports[1]);
    node.info.set_rpc(rpc_addr).unwrap();
    node.info.set_rpc_pubsub(rpc_pubsub_addr).unwrap();
    node
}
