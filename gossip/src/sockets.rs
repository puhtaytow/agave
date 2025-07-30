use std::net::UdpSocket;

#[derive(Debug)]
pub struct Tvu {
    sockets: Vec<UdpSocket>,
    quic: UdpSocket,
}

impl Tvu {
    /// returns new tvu sockets group
    pub fn new(sockets: Vec<UdpSocket>, quic: UdpSocket) -> Self {
        Self { sockets, quic }
    }

    /// returns tvu sockets slice
    pub fn sockets(&self) -> &[UdpSocket] {
        &self.sockets
    }

    /// returns tvu quic socket ref
    pub fn quic_socket(&self) -> &UdpSocket {
        &self.quic
    }
}

#[derive(Debug)]
pub struct Tpu {
    sockets: Vec<UdpSocket>,
    forwards: Vec<UdpSocket>,
    vote: Vec<UdpSocket>,
    quic: Vec<UdpSocket>,
    forwards_quic: Vec<UdpSocket>,
    vote_quic: Vec<UdpSocket>,
    // Client-side socket for ForwardingStage vote transactions
    vote_forwarding_client: UdpSocket,
    // Client-side socket for ForwardingStage non-vote transactions
    transaction_forwarding_client: UdpSocket,
}

impl Tpu {
    /// returns new tpu sockets group
    pub fn new(
        sockets: Vec<UdpSocket>,
        forwards: Vec<UdpSocket>,
        vote: Vec<UdpSocket>,
        quic: Vec<UdpSocket>,
        forwards_quic: Vec<UdpSocket>,
        vote_quic: Vec<UdpSocket>,
        vote_forwarding_client: UdpSocket,
        transaction_forwarding_client: UdpSocket,
    ) -> Self {
        Self {
            sockets,
            forwards,
            vote,
            quic,
            forwards_quic,
            vote_quic,
            vote_forwarding_client,
            transaction_forwarding_client,
        }
    }

    /// returns tpu sockets slice
    pub fn sockets(&self) -> &[UdpSocket] {
        &self.sockets
    }

    /// returns tpu forwards sockets slice
    pub fn forwards(&self) -> &[UdpSocket] {
        &self.forwards
    }

    /// returns tpu vote sockets slice
    pub fn vote(&self) -> &[UdpSocket] {
        &self.vote
    }

    /// returns tpu quic sockets slice
    pub fn quic(&self) -> &[UdpSocket] {
        &self.quic
    }

    /// returns tpu forwards quic sockets slice
    pub fn forwards_quic(&self) -> &[UdpSocket] {
        &self.forwards_quic
    }

    /// returns tpu votes sockets slice
    pub fn vote_quic(&self) -> &[UdpSocket] {
        &self.vote_quic
    }

    /// returns tpu vote forwarding client socket ref
    pub fn vote_forwarding_client(&self) -> &UdpSocket {
        &self.vote_forwarding_client
    }

    /// returns tpu transaction forwarding client socket ref
    pub fn transaction_forwarding_client(&self) -> &UdpSocket {
        &self.transaction_forwarding_client
    }
}

#[derive(Debug)]
pub struct Repair {
    // Socket sending out local repair requests,
    // and receiving repair responses from the cluster.
    socket: UdpSocket,
    quic: UdpSocket,
}

impl Repair {
    /// returns repair sockets group
    pub fn new(socket: UdpSocket, quic: UdpSocket) -> Self {
        Self { socket, quic }
    }

    /// returns repair socket ref
    pub fn socket(&self) -> &UdpSocket {
        &self.socket
    }

    /// returns repair quic socket ref
    pub fn quic(&self) -> &UdpSocket {
        &self.quic
    }
}

#[cfg(test)]
mod tests {
    use {
        crate::sockets::{Repair, Tpu, Tvu},
        solana_net_utils::sockets::{
            bind_to, bind_to_localhost_unique, unique_port_range_for_tests,
        },
        std::net::{IpAddr, Ipv4Addr, UdpSocket},
    };

    fn assert_sockets_range(num_ports: u16, ip_addr: IpAddr, sockets: &[UdpSocket]) {
        assert_eq!(
            num_ports as usize,
            sockets.len(),
            "number of ports and sockets must match"
        );
        for socket in sockets {
            let addr = socket.local_addr().unwrap();
            assert_eq!(ip_addr, addr.ip());
        }
    }

    fn vec_sockets_from_size_and_addr(size: u16, ip_addr: IpAddr) -> Vec<UdpSocket> {
        let mut sockets = vec![];
        unique_port_range_for_tests(size).for_each(|port| {
            sockets.push(
                bind_to(ip_addr, port)
                    .unwrap_or_else(|_| panic!("{}", &format!("should bind - port: {:?}", port))),
            );
        });
        sockets
    }

    #[test]
    fn test_new_tvu_verify_outcome() {
        const NUM_PORTS: u16 = 3;
        const IP_ADDR: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

        let quic = bind_to_localhost_unique().expect("should bind - quic port");
        let sockets = vec_sockets_from_size_and_addr(NUM_PORTS, IP_ADDR);
        let tvu_group = Tvu::new(sockets, quic);

        assert_eq!(IP_ADDR, tvu_group.quic_socket().local_addr().unwrap().ip());
        assert_sockets_range(NUM_PORTS, IP_ADDR, tvu_group.sockets());
    }

    #[test]
    fn test_new_tpu_verify_outcome() {
        const NUM_PORTS: u16 = 3;
        const IP_ADDR: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

        let sockets = vec_sockets_from_size_and_addr(NUM_PORTS, IP_ADDR);
        let forwards = vec_sockets_from_size_and_addr(NUM_PORTS, IP_ADDR);
        let votes = vec_sockets_from_size_and_addr(NUM_PORTS, IP_ADDR);
        let quic = vec_sockets_from_size_and_addr(NUM_PORTS, IP_ADDR);
        let forwards_quic = vec_sockets_from_size_and_addr(NUM_PORTS, IP_ADDR);
        let votes_quic = vec_sockets_from_size_and_addr(NUM_PORTS, IP_ADDR);

        let vote_forwarding_client =
            bind_to_localhost_unique().expect("should bind - vote_forwarding_client port");
        let transaction_forwarding_client =
            bind_to_localhost_unique().expect("should bind - transaction_forwarding_client port");

        let tpu_group = Tpu::new(
            sockets,
            forwards,
            votes,
            quic,
            forwards_quic,
            votes_quic,
            vote_forwarding_client,
            transaction_forwarding_client,
        );

        assert_sockets_range(NUM_PORTS, IP_ADDR, tpu_group.sockets());
        assert_sockets_range(NUM_PORTS, IP_ADDR, tpu_group.forwards());
        assert_sockets_range(NUM_PORTS, IP_ADDR, tpu_group.vote());
        assert_sockets_range(NUM_PORTS, IP_ADDR, tpu_group.quic());
        assert_sockets_range(NUM_PORTS, IP_ADDR, tpu_group.forwards_quic());
        assert_sockets_range(NUM_PORTS, IP_ADDR, tpu_group.vote_quic());

        assert_eq!(
            IP_ADDR,
            tpu_group
                .vote_forwarding_client()
                .local_addr()
                .unwrap()
                .ip()
        );
        assert_eq!(
            IP_ADDR,
            tpu_group
                .transaction_forwarding_client()
                .local_addr()
                .unwrap()
                .ip()
        );
    }

    #[test]
    fn test_new_repair_verify_outcome() {
        const IP_ADDR: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

        let socket = bind_to_localhost_unique().expect("should bind - repair port");
        let quic = bind_to_localhost_unique().expect("should bind - quic port");
        let repair_group = Repair::new(socket, quic);

        assert_eq!(IP_ADDR, repair_group.socket().local_addr().unwrap().ip());
        assert_eq!(IP_ADDR, repair_group.quic().local_addr().unwrap().ip());
    }
}
