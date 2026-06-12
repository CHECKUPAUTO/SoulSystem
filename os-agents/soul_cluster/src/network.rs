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

            // SAFETY: On utilise read_unaligned pour éviter les undefined behaviors
            // liés aux accès désalignés sur un struct `#[repr(C, packed)]`.
            // Chaque champ est lu depuis sa position exacte dans le buffer.
            let magic = unsafe { std::ptr::read_unaligned(incoming.as_ptr() as *const u32) };
            if magic != MAGIC_BYTES {
                return None;
            }

            let src_agent = unsafe { std::ptr::read_unaligned(incoming.as_ptr().add(4) as *const u32) };
            let dst_agent = unsafe { std::ptr::read_unaligned(incoming.as_ptr().add(8) as *const u32) };
            let signal = unsafe { std::ptr::read_unaligned(incoming.as_ptr().add(12) as *const u32) };
            let payload_len = unsafe { std::ptr::read_unaligned(incoming.as_ptr().add(16) as *const u32) } as usize;

            if payload_len > MAX_PAYLOAD_LEN {
                return None;
            }

            // Copie de la charge utile réseau dans le tampon de stockage persistant
            let data_offset = 20; // taille de l'en-tête (5 * u32)
            storage_buffer[0..payload_len]
                .copy_from_slice(&incoming[data_offset..data_offset + payload_len]);

            Some(AgentMessage {
                source_agent_id: src_agent,
                target_agent_id: dst_agent,
                signal_code: signal,
                payload_ptr: storage_buffer.as_mut_ptr(),
                payload_size: payload_len,
            })
        } else {
            None
        }
    }
}
