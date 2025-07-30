use std::net::UdpSocket;

pub struct Tvu {
    sockets: Vec<UdpSocket>,
    quic_socket: UdpSocket,
}

impl Tvu {
    /// returns new tvu sockets group
    pub fn new(sockets: Vec<UdpSocket>, quic_socket: UdpSocket) -> Self {
        Self {
            sockets,
            quic_socket,
        }
    }

    /// returns tvu sockets slice
    pub fn sockets(&self) -> &[UdpSocket] {
        &self.sockets
    }

    /// returns tvu quic socket ref
    pub fn quic_socket(&self) -> &UdpSocket {
        &self.quic_socket
    }
}

pub struct Tpu {
    sockets: Vec<UdpSocket>,
    forwards: Vec<UdpSocket>,
    votes: Vec<UdpSocket>,
}

impl Tpu {
    /// returns new tpu sockets group
    pub fn new(sockets: Vec<UdpSocket>, forwards: Vec<UdpSocket>, votes: Vec<UdpSocket>) -> Self {
        Self {
            sockets,
            forwards,
            votes,
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

    /// returns tpu votes sockets slice
    pub fn votes(&self) -> &[UdpSocket] {
        &self.votes
    }
}

#[cfg(test)]
mod tests {
    use {
        crate::sockets::{Tpu, Tvu},
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
        unique_port_range_for_tests(size)
            .clone()
            .into_iter()
            .for_each(|port| {
                sockets.push(
                    bind_to(ip_addr, port).expect(&format!("should bind - sockets: {:?}", port)),
                );
            });
        sockets
    }

    #[test]
    fn test_new_tvu_verify_outcome() {
        const NUM_PORTS: u16 = 3;
        const IP_ADDR: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

        let quic_socket = bind_to_localhost_unique().expect("should bind - quic port");
        let sockets = vec_sockets_from_size_and_addr(NUM_PORTS, IP_ADDR);
        let tvu_group = Tvu::new(sockets, quic_socket);

        assert_eq!(IP_ADDR, tvu_group.quic_socket().local_addr().unwrap().ip());
        assert_sockets_range(NUM_PORTS, IP_ADDR, tvu_group.sockets());
    }

    #[test]
    fn test_new_tpu_verify_outcome() {
        const NUM_PORTS: usize = 3;
        const IP_ADDR: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

        let mut sockets = vec![];
        let mut forwards = vec![];
        let mut votes = vec![];

        let port_range_sockets = unique_port_range_for_tests(NUM_PORTS as u16);
        let port_range_forwards = unique_port_range_for_tests(NUM_PORTS as u16);
        let port_range_votes = unique_port_range_for_tests(NUM_PORTS as u16);

        port_range_sockets.clone().into_iter().for_each(|port| {
            sockets
                .push(bind_to(IP_ADDR, port).expect(&format!("should bind - sockets: {:?}", port)));
        });
        port_range_forwards.clone().into_iter().for_each(|port| {
            forwards.push(
                bind_to(IP_ADDR, port).expect(&format!("should bind - forwards: {:?}", port)),
            );
        });
        port_range_votes.clone().into_iter().for_each(|port| {
            votes.push(bind_to(IP_ADDR, port).expect(&format!("should bind - votes: {:?}", port)));
        });

        let tpu_group = Tpu::new(sockets, forwards, votes);
        assert_eq!(NUM_PORTS, tpu_group.sockets.len());
        assert_eq!(NUM_PORTS, tpu_group.forwards.len());
        assert_eq!(NUM_PORTS, tpu_group.votes.len());

        for s in tpu_group.sockets() {
            let addr = s.local_addr().unwrap();
            assert_eq!(IP_ADDR, addr.ip());
            assert!(
                port_range_sockets.clone().contains(&addr.port()),
                "socket port {} not in reserved range {:?}",
                addr.port(),
                port_range_sockets
            );
        }
        for s in tpu_group.forwards() {
            let addr = s.local_addr().unwrap();
            assert_eq!(IP_ADDR, addr.ip());
            assert!(
                port_range_forwards.clone().contains(&addr.port()),
                "socket port {} not in reserved range {:?}",
                addr.port(),
                port_range_forwards
            );
        }
        for s in tpu_group.votes() {
            let addr = s.local_addr().unwrap();
            assert_eq!(IP_ADDR, addr.ip());
            assert!(
                port_range_votes.clone().contains(&addr.port()),
                "socket port {} not in reserved range {:?}",
                addr.port(),
                port_range_votes
            );
        }
    }
}
