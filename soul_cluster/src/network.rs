use soul_ipc::bus::AgentMessage;
use std::net::UdpSocket;

/// Packet binaire brut réseau de taille fixe (32 octets d'en-tête + charge utile)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct NetworkPacket {
    pub magic_bytes: u32, // Validation d'intégrité de l'OS (0x50554C)
    pub src_agent: u32,
    pub dst_agent: u32,
    pub signal: u32,
    pub payload_len: u32,
    pub data: [u8; 256],
}

pub struct ClusterNode {
    socket: UdpSocket,
}

impl ClusterNode {
    pub fn bind(local_address: &str) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(local_address)?;
        socket.set_nonblocking(true)?;
        Ok(Self { socket })
    }

    /// # Safety
    /// msg.payload_ptr must be valid for payload_size bytes if non-null.
    pub unsafe fn transmit_remote(
        &self,
        target_node_addr: &str,
        msg: &AgentMessage,
    ) -> std::io::Result<usize> {
        let mut pkt = NetworkPacket {
            magic_bytes: 0x50554C,
            src_agent: msg.source_agent_id,
            dst_agent: msg.target_agent_id,
            signal: msg.signal_code,
            payload_len: std::cmp::min(msg.payload_size as u32, 256),
            data: [0u8; 256],
        };

        if !msg.payload_ptr.is_null() && msg.payload_size > 0 {
            let slice = std::slice::from_raw_parts(msg.payload_ptr, pkt.payload_len as usize);
            pkt.data[0..slice.len()].copy_from_slice(slice);
        }

        let raw_ptr = &pkt as *const NetworkPacket as *const u8;
        let byte_slice = std::slice::from_raw_parts(raw_ptr, std::mem::size_of::<NetworkPacket>());

        self.socket.send_to(byte_slice, target_node_addr)
    }

    /// Reçoit un paquet du cluster et le convertit en message IPC exploitable localement
    pub fn listen_and_inject(&self, storage_buffer: &mut [u8; 256]) -> Option<AgentMessage> {
        let mut incoming = [0u8; std::mem::size_of::<NetworkPacket>()];

        if let Ok((bytes_received, _remote_src)) = self.socket.recv_from(&mut incoming) {
            if bytes_received < std::mem::size_of::<NetworkPacket>() {
                return None;
            }

            let pkt = unsafe { &*(incoming.as_ptr() as *const NetworkPacket) };
            if pkt.magic_bytes != 0x50554C {
                return None;
            } // Rejet des paquets corrompus

            // Copie de la charge utile réseau dans le tampon de stockage persistant
            storage_buffer[0..pkt.payload_len as usize]
                .copy_from_slice(&pkt.data[0..pkt.payload_len as usize]);

            Some(AgentMessage {
                source_agent_id: pkt.src_agent,
                target_agent_id: pkt.dst_agent,
                signal_code: pkt.signal,
                payload_ptr: storage_buffer.as_mut_ptr(),
                payload_size: pkt.payload_len as usize,
            })
        } else {
            None
        }
    }
}
