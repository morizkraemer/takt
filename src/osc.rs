use rosc::{encoder, OscMessage, OscPacket, OscType};
use std::net::UdpSocket;
use std::sync::Arc;

#[derive(Clone)]
pub struct OscSender {
    socket: Arc<UdpSocket>,
    target: String,
}

impl OscSender {
    pub fn new(ip: &str, port: u16) -> Result<Self, String> {
        let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
        Ok(Self {
            socket: Arc::new(socket),
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

    /// Send a string to an address
    pub fn send_string(&self, address: &str, value: &str) {
        self.send(address, vec![OscType::String(value.to_string())]);
    }
}

/// OSC input listener — receives incoming messages and signals resets.
pub struct OscReceiver {
    _thread: std::thread::JoinHandle<()>,
}

/// Commands that can come in via OSC.
#[derive(Debug, Clone)]
pub enum OscCommand {
    Reset,
}

fn handle_packet(packet: &OscPacket, tx: &std::sync::mpsc::Sender<OscCommand>) {
    match packet {
        OscPacket::Message(msg) => {
            match msg.addr.as_str() {
                "/takt/reset" | "/bpm/reset" => {
                    let _ = tx.send(OscCommand::Reset);
                }
                other => {
                    eprintln!("[osc-in] unhandled: {other}");
                }
            }
        }
        OscPacket::Bundle(bundle) => {
            for p in &bundle.content {
                handle_packet(p, tx);
            }
        }
    }
}

impl OscReceiver {
    /// Start listening on `port`. Returns a receiver channel for incoming commands.
    pub fn new(port: u16) -> Result<(Self, std::sync::mpsc::Receiver<OscCommand>), String> {
        let socket = UdpSocket::bind(("0.0.0.0", port)).map_err(|e| format!("OSC listen bind: {e}"))?;
        let (tx, rx) = std::sync::mpsc::channel();

        let thread = std::thread::Builder::new()
            .name("osc-listen".into())
            .spawn(move || {
                let mut buf = [0u8; 65535];
                loop {
                    match socket.recv_from(&mut buf) {
                        Ok((size, _addr)) => {
                            match rosc::decoder::decode_udp(&buf[..size]) {
                                Ok((_rest, packet)) => {
                                    handle_packet(&packet, &tx);
                                }
                                Err(e) => {
                                    eprintln!("[osc-in] decode error: {e}");
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("[osc-in] recv error: {e}");
                            break;
                        }
                    }
                }
            })
            .map_err(|e| format!("OSC listen thread: {e}"))?;

        Ok((Self { _thread: thread }, rx))
    }
}