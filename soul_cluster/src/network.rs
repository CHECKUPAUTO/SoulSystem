use soul_ipc::bus::AgentMessage;
use std::net::UdpSocket;

/// Magic bytes du protocole réseau SoulSystem (ASCII "PUL")
const MAGIC_BYTES: u32 = 0x50554C;

/// Taille maximale de la charge utile d'un paquet réseau
const MAX_PAYLOAD_LEN: usize = 256;

/// Packet binaire brut réseau de taille fixe (32 octets d'en-tête + charge utile)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct NetworkPacket {
    pub magic_bytes: u32,
    pub src_agent: u32,
    pub dst_agent: u32,
    pub signal: u32,
    pub payload_len: u32,
    pub data: [u8; MAX_PAYLOAD_LEN],
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
    /// msg.payload_ptr doit être valide pour msg.payload_size octets si non-null.
    /// Le paquet est envoyé en binaire brut (`repr(C, packed)`) — l'alignement est
    /// garanti par le layout C du struct.
    pub unsafe fn transmit_remote(
        &self,
        target_node_addr: &str,
        msg: &AgentMessage,
    ) -> std::io::Result<usize> {
        let payload_len = std::cmp::min(msg.payload_size, MAX_PAYLOAD_LEN) as u32;
        let mut pkt = NetworkPacket {
            magic_bytes: MAGIC_BYTES,
            src_agent: msg.source_agent_id,
            dst_agent: msg.target_agent_id,
            signal: msg.signal_code,
            payload_len,
            data: [0u8; MAX_PAYLOAD_LEN],
        };

        if !msg.payload_ptr.is_null() && msg.payload_size > 0 {
            let len = payload_len as usize;
            let slice = std::slice::from_raw_parts(msg.payload_ptr, len);
            pkt.data[0..len].copy_from_slice(slice);
        }

        // SAFETY: NetworkPacket est repr(C, packed), donc la conversion en byte slice
        // est sûre et produit une représentation binaire déterministe.
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

            // SAFETY: `incoming` est un buffer de taille exacte `size_of::<NetworkPacket>()`.
            // `NetworkPacket` est `#[repr(C, packed)]` donc pas de padding et la taille est
            // prévisible. La validation magic_bytes juste après garantit que le paquet est
            // conforme au protocole. Le buffer est local à cette fonction (stack) donc
            // l'alignement est géré par l'allocateur.
            let pkt = unsafe { &*(incoming.as_ptr() as *const NetworkPacket) };
            if pkt.magic_bytes != 0x50554C {
                return None;
            } // Rejet des paquets corrompus

            // Copie de la charge utile réseau dans le tampon de stockage persistant
            let payload_len = pkt.payload_len as usize;
            if payload_len > 256 {
                return None;
            }
            storage_buffer[0..payload_len]
                .copy_from_slice(&pkt.data[0..payload_len]);

            Some(AgentMessage {
                source_agent_id: pkt.src_agent,
                target_agent_id: pkt.dst_agent,
                signal_code: pkt.signal,
                payload_ptr: storage_buffer.as_mut_ptr(),
                payload_size: payload_len,
            })
        } else {
            None
        }
    }
}
