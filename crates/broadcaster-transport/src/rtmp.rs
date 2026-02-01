//! RTMP client implementation.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use crossbeam_channel::{Receiver, Sender};
use parking_lot::RwLock;
use rml_rtmp::handshake::{Handshake, HandshakeProcessResult, PeerType};
use rml_rtmp::sessions::{
    ClientSession, ClientSessionConfig, ClientSessionEvent, ClientSessionResult,
};
use rml_rtmp::time::RtmpTimestamp;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::runtime::Runtime;
use tracing::{debug, error, info, instrument, trace, warn};
use url::Url;

use crate::connection::{ConnectionState, ReconnectPolicy};
use crate::error::TransportError;
use crate::{TransportResult, PACKET_CHANNEL_CAPACITY};

/// A packet to send over RTMP.
#[derive(Debug, Clone)]
pub struct RtmpPacket {
    /// Packet data.
    pub data: Bytes,

    /// Presentation timestamp in milliseconds.
    pub timestamp_ms: u32,

    /// Whether this is a video packet.
    pub is_video: bool,

    /// Whether this is a keyframe (for video).
    pub is_keyframe: bool,
}

/// RTMP client for streaming.
pub struct RtmpClient {
    rtmp_url: String,
    stream_key: String,
    state: Arc<RwLock<ConnectionState>>,
    runtime: Option<Runtime>,
    should_stop: Arc<AtomicBool>,
    packet_sender: Option<Sender<RtmpPacket>>,
    reconnect_policy: ReconnectPolicy,
    bytes_sent: AtomicU64,
    packets_sent: AtomicU64,
    packets_dropped: AtomicU64,
}

impl RtmpClient {
    /// Create a new RTMP client.
    pub fn new(rtmp_url: String, stream_key: String) -> TransportResult<Self> {
        // Validate URL
        if !rtmp_url.starts_with("rtmp://") && !rtmp_url.starts_with("rtmps://") {
            return Err(TransportError::InvalidUrl(
                "URL must start with rtmp:// or rtmps://".to_string(),
            ));
        }

        Ok(Self {
            rtmp_url,
            stream_key,
            state: Arc::new(RwLock::new(ConnectionState::Disconnected)),
            runtime: None,
            should_stop: Arc::new(AtomicBool::new(false)),
            packet_sender: None,
            reconnect_policy: ReconnectPolicy::default(),
            bytes_sent: AtomicU64::new(0),
            packets_sent: AtomicU64::new(0),
            packets_dropped: AtomicU64::new(0),
        })
    }

    /// Connect to the RTMP server.
    #[instrument(name = "rtmp_connect", skip(self))]
    pub fn connect(&mut self) -> TransportResult<Sender<RtmpPacket>> {
        if self.state.read().is_connected() {
            return Err(TransportError::AlreadyConnected);
        }

        info!(url = %self.rtmp_url, "Connecting to RTMP server");
        *self.state.write() = ConnectionState::Connecting;

        // Create tokio runtime for async network operations
        let runtime = Runtime::new().map_err(TransportError::Io)?;

        // Create packet channel
        let (sender, receiver): (Sender<RtmpPacket>, Receiver<RtmpPacket>) =
            crossbeam_channel::bounded(PACKET_CHANNEL_CAPACITY);

        let state = Arc::clone(&self.state);
        let should_stop = Arc::clone(&self.should_stop);
        should_stop.store(false, Ordering::SeqCst);

        let url = self.rtmp_url.clone();
        let key = self.stream_key.clone();
        let policy = self.reconnect_policy.clone();
        let bytes_sent = Arc::new(AtomicU64::new(0));
        let packets_sent = Arc::new(AtomicU64::new(0));
        let packets_dropped = Arc::new(AtomicU64::new(0));

        let bytes_sent_clone = Arc::clone(&bytes_sent);
        let packets_sent_clone = Arc::clone(&packets_sent);
        let packets_dropped_clone = Arc::clone(&packets_dropped);

        // Spawn connection task
        runtime.spawn(async move {
            if let Err(e) = run_rtmp_connection(
                url,
                key,
                receiver,
                state,
                should_stop,
                policy,
                bytes_sent_clone,
                packets_sent_clone,
                packets_dropped_clone,
            )
            .await
            {
                error!("RTMP connection error: {}", e);
            }
        });

        self.runtime = Some(runtime);
        self.packet_sender = Some(sender.clone());

        Ok(sender)
    }

    /// Disconnect from the RTMP server.
    #[instrument(name = "rtmp_disconnect", skip(self))]
    pub fn disconnect(&mut self) -> TransportResult<()> {
        info!("Disconnecting from RTMP server");

        self.should_stop.store(true, Ordering::SeqCst);

        // Drop the packet sender to signal shutdown
        self.packet_sender = None;

        // Shutdown runtime
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_timeout(Duration::from_secs(5));
        }

        *self.state.write() = ConnectionState::Disconnected;

        info!("Disconnected from RTMP server");
        Ok(())
    }

    /// Get the current connection state.
    pub fn state(&self) -> ConnectionState {
        self.state.read().clone()
    }

    /// Check if connected.
    pub fn is_connected(&self) -> bool {
        self.state.read().is_connected()
    }

    /// Get transport statistics.
    pub fn statistics(&self) -> TransportStatistics {
        TransportStatistics {
            bytes_sent: self.bytes_sent.load(Ordering::Relaxed),
            packets_sent: self.packets_sent.load(Ordering::Relaxed),
            packets_dropped: self.packets_dropped.load(Ordering::Relaxed),
        }
    }
}

impl Drop for RtmpClient {
    fn drop(&mut self) {
        let _ = self.disconnect();
    }
}

/// Transport statistics.
#[derive(Debug, Clone, Default)]
pub struct TransportStatistics {
    pub bytes_sent: u64,
    pub packets_sent: u64,
    pub packets_dropped: u64,
}

#[allow(clippy::too_many_arguments)]
async fn run_rtmp_connection(
    url: String,
    stream_key: String,
    receiver: Receiver<RtmpPacket>,
    state: Arc<RwLock<ConnectionState>>,
    should_stop: Arc<AtomicBool>,
    policy: ReconnectPolicy,
    bytes_sent: Arc<AtomicU64>,
    packets_sent: Arc<AtomicU64>,
    packets_dropped: Arc<AtomicU64>,
) -> TransportResult<()> {
    let mut attempt = 0u32;

    loop {
        if should_stop.load(Ordering::SeqCst) {
            break;
        }

        // Try to connect
        match connect_rtmp(&url, &stream_key).await {
            Ok(mut connection) => {
                *state.write() = ConnectionState::Connected;
                attempt = 0;

                info!("RTMP connection established");

                // Send packets until error or stop
                loop {
                    if should_stop.load(Ordering::SeqCst) {
                        break;
                    }

                    match receiver.recv_timeout(Duration::from_millis(100)) {
                        Ok(packet) => {
                            if let Err(e) = send_packet(&mut connection, &packet).await {
                                warn!("Send error: {}", e);
                                packets_dropped.fetch_add(1, Ordering::Relaxed);
                                break; // Reconnect
                            }
                            bytes_sent.fetch_add(packet.data.len() as u64, Ordering::Relaxed);
                            packets_sent.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                        Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                            debug!("Packet channel disconnected");
                            return Ok(());
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Connection attempt {} failed: {}", attempt + 1, e);
                attempt += 1;

                if !policy.should_retry(attempt) {
                    *state.write() = ConnectionState::Failed {
                        reason: format!("Failed after {} attempts: {}", attempt, e),
                    };
                    return Err(TransportError::ReconnectExhausted(attempt));
                }

                *state.write() = ConnectionState::Reconnecting { attempt };
                let delay = policy.delay_for_attempt(attempt);
                info!("Reconnecting in {:?}...", delay);
                tokio::time::sleep(delay).await;
            }
        }
    }

    Ok(())
}

/// RTMP connection with session state.
#[allow(dead_code)]
struct RtmpConnection {
    /// TCP stream to the RTMP server.
    stream: TcpStream,
    /// RTMP client session for protocol handling.
    session: ClientSession,
    /// Application name from URL.
    app_name: String,
    /// Stream key for publishing.
    stream_key: String,
    /// Whether we've successfully started publishing.
    publishing: bool,
}

async fn connect_rtmp(url: &str, stream_key: &str) -> TransportResult<RtmpConnection> {
    debug!(url = %url, "Connecting to RTMP server");

    // Parse URL to extract host, port, app name
    let parsed = Url::parse(url).map_err(|e| TransportError::InvalidUrl(e.to_string()))?;

    let host = parsed
        .host_str()
        .ok_or_else(|| TransportError::InvalidUrl("Missing host".to_string()))?;
    let port = parsed.port().unwrap_or(1935);
    let app_name = parsed.path().trim_start_matches('/').to_string();

    if app_name.is_empty() {
        return Err(TransportError::InvalidUrl(
            "Missing application name in URL path".to_string(),
        ));
    }

    info!(host = %host, port = port, app = %app_name, "Connecting to RTMP server");

    // Establish TCP connection
    let addr = format!("{}:{}", host, port);
    let mut stream = TcpStream::connect(&addr)
        .await
        .map_err(|e| TransportError::Connection(format!("TCP connect failed: {}", e)))?;

    debug!("TCP connection established, starting handshake");

    // Perform RTMP handshake
    let mut handshake = Handshake::new(PeerType::Client);

    // Generate and send C0+C1
    let p0_p1 = handshake
        .generate_outbound_p0_and_p1()
        .map_err(|e| TransportError::Connection(format!("Handshake generation failed: {:?}", e)))?;
    stream
        .write_all(&p0_p1)
        .await
        .map_err(|e| TransportError::Connection(format!("Handshake write failed: {}", e)))?;

    // Read handshake response (S0+S1+S2 = 1 + 1536 + 1536 = 3073 bytes)
    let mut handshake_buf = vec![0u8; 4096];
    let mut handshake_complete = false;
    let mut leftover_bytes = Vec::new();

    while !handshake_complete {
        let n = stream
            .read(&mut handshake_buf)
            .await
            .map_err(|e| TransportError::Connection(format!("Handshake read failed: {}", e)))?;

        if n == 0 {
            return Err(TransportError::Connection(
                "Connection closed during handshake".to_string(),
            ));
        }

        match handshake.process_bytes(&handshake_buf[..n]) {
            Ok(HandshakeProcessResult::InProgress { response_bytes }) => {
                if !response_bytes.is_empty() {
                    stream.write_all(&response_bytes).await.map_err(|e| {
                        TransportError::Connection(format!("Handshake write failed: {}", e))
                    })?;
                }
            }
            Ok(HandshakeProcessResult::Completed {
                response_bytes,
                remaining_bytes,
            }) => {
                if !response_bytes.is_empty() {
                    stream.write_all(&response_bytes).await.map_err(|e| {
                        TransportError::Connection(format!("Handshake write failed: {}", e))
                    })?;
                }
                leftover_bytes = remaining_bytes;
                handshake_complete = true;
            }
            Err(e) => {
                return Err(TransportError::Connection(format!(
                    "Handshake failed: {:?}",
                    e
                )));
            }
        }
    }

    debug!("Handshake complete, creating RTMP session");

    // Create RTMP client session
    let config = ClientSessionConfig::new();
    let (mut session, initial_results) = ClientSession::new(config)
        .map_err(|e| TransportError::Connection(format!("Session creation failed: {:?}", e)))?;

    // Send initial session packets (chunk size, etc.)
    for result in initial_results {
        if let ClientSessionResult::OutboundResponse(packet) = result {
            stream
                .write_all(&packet.bytes)
                .await
                .map_err(TransportError::Io)?;
        }
    }

    // Process any leftover bytes from handshake
    if !leftover_bytes.is_empty() {
        let _ = session.handle_input(&leftover_bytes);
    }

    // Request connection to the application
    debug!(app = %app_name, "Requesting RTMP connection");
    let connect_results = session
        .request_connection(app_name.clone())
        .map_err(|e| TransportError::Connection(format!("Connection request failed: {:?}", e)))?;

    // Send connection request
    if let ClientSessionResult::OutboundResponse(packet) = connect_results {
        stream
            .write_all(&packet.bytes)
            .await
            .map_err(TransportError::Io)?;
    }

    // Wait for connection acceptance
    let mut connected = false;
    let mut read_buf = vec![0u8; 4096];

    for _ in 0..50 {
        // Timeout after ~5 seconds
        tokio::select! {
            result = stream.read(&mut read_buf) => {
                let n = result.map_err(TransportError::Io)?;
                if n == 0 {
                    return Err(TransportError::Connection("Connection closed".to_string()));
                }

                let results = session
                    .handle_input(&read_buf[..n])
                    .map_err(|e| TransportError::Connection(format!("Session input error: {:?}", e)))?;

                for result in results {
                    match result {
                        ClientSessionResult::OutboundResponse(packet) => {
                            stream.write_all(&packet.bytes).await.map_err(TransportError::Io)?;
                        }
                        ClientSessionResult::RaisedEvent(event) => {
                            match event {
                                ClientSessionEvent::ConnectionRequestAccepted => {
                                    debug!("Connection accepted by server");
                                    connected = true;
                                }
                                ClientSessionEvent::ConnectionRequestRejected { description } => {
                                    return Err(TransportError::Connection(
                                        format!("Connection rejected: {}", description),
                                    ));
                                }
                                _ => {
                                    trace!("Received event: {:?}", event);
                                }
                            }
                        }
                        _ => {}
                    }
                }

                if connected {
                    break;
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                continue;
            }
        }
    }

    if !connected {
        return Err(TransportError::Connection(
            "Timeout waiting for connection acceptance".to_string(),
        ));
    }

    // Request publishing
    debug!(stream_key = %stream_key, "Requesting publish");
    let publish_results = session
        .request_publishing(
            stream_key.to_string(),
            rml_rtmp::sessions::PublishRequestType::Live,
        )
        .map_err(|e| TransportError::Connection(format!("Publish request failed: {:?}", e)))?;

    if let ClientSessionResult::OutboundResponse(packet) = publish_results {
        stream
            .write_all(&packet.bytes)
            .await
            .map_err(TransportError::Io)?;
    }

    // Wait for publish acceptance
    let mut publishing = false;
    for _ in 0..30 {
        tokio::select! {
            result = stream.read(&mut read_buf) => {
                let n = result.map_err(TransportError::Io)?;
                if n == 0 {
                    return Err(TransportError::Connection("Connection closed".to_string()));
                }

                let results = session
                    .handle_input(&read_buf[..n])
                    .map_err(|e| TransportError::Connection(format!("Session input error: {:?}", e)))?;

                for result in results {
                    match result {
                        ClientSessionResult::OutboundResponse(packet) => {
                            stream.write_all(&packet.bytes).await.map_err(TransportError::Io)?;
                        }
                        ClientSessionResult::RaisedEvent(
                            ClientSessionEvent::PublishRequestAccepted,
                        ) => {
                            debug!("Publish request accepted");
                            publishing = true;
                        }
                        ClientSessionResult::RaisedEvent(_) => {}
                        _ => {}
                    }
                }

                if publishing {
                    break;
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                continue;
            }
        }
    }

    if !publishing {
        return Err(TransportError::Connection(
            "Timeout waiting for publish acceptance".to_string(),
        ));
    }

    info!("RTMP connection established and publishing started");

    Ok(RtmpConnection {
        stream,
        session,
        app_name,
        stream_key: stream_key.to_string(),
        publishing: true,
    })
}

async fn send_packet(connection: &mut RtmpConnection, packet: &RtmpPacket) -> TransportResult<()> {
    let timestamp = RtmpTimestamp::new(packet.timestamp_ms);

    // Publish the packet through the session
    let result = if packet.is_video {
        connection.session.publish_video_data(
            packet.data.clone(),
            timestamp,
            !packet.is_keyframe, // can_be_dropped: true for non-keyframes
        )
    } else {
        connection.session.publish_audio_data(
            packet.data.clone(),
            timestamp,
            false, // can_be_dropped: audio is important
        )
    };

    let session_result =
        result.map_err(|e| TransportError::Send(format!("Failed to publish data: {:?}", e)))?;

    // Send the outbound packet
    if let ClientSessionResult::OutboundResponse(rtmp_packet) = session_result {
        connection
            .stream
            .write_all(&rtmp_packet.bytes)
            .await
            .map_err(TransportError::Io)?;
    }

    Ok(())
}
