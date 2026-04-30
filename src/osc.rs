use rosc::{encoder, OscMessage, OscPacket, OscType};
use std::net::UdpSocket;

pub struct OscSender {
    socket: UdpSocket,
    target: String,
}

impl OscSender {
    pub fn new(ip: &str, port: u16) -> Result<Self, String> {
        let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
        Ok(Self {
            socket,
            target: format!("{ip}:{port}"),
        })
    }

    fn send(&self, addr: &str, args: Vec<OscType>) {
        let msg = OscPacket::Message(OscMessage {
            addr: addr.to_string(),
            args,
        });
        if let Ok(bytes) = encoder::encode(&msg) {
            let _ = self.socket.send_to(&bytes, &self.target);
        }
    }

    /// Chromatik: `/lx/tempo/bpm <float32>`
    pub fn send_bpm(&self, bpm: f64) {
        self.send("/lx/tempo/bpm", vec![OscType::Float(bpm as f32)]);
    }

    /// Chromatik: `/lx/tempo/beat` (no args — advance beat counter)
    pub fn send_beat(&self) {
        self.send("/lx/tempo/beat", vec![]);
    }

    /// Chromatik: `/lx/tempo/beat-within-bar <int32>` (1-indexed)
    pub fn send_beat_within_bar(&self, beat: i32) {
        self.send("/lx/tempo/beat-within-bar", vec![OscType::Int(beat)]);
    }

    /// Raw mode: send BPM to a custom address
    pub fn send_bpm_raw(&self, address: &str, bpm: f64) {
        self.send(address, vec![OscType::Float(bpm as f32)]);
    }
}
