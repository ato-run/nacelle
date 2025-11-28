use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{info, warn, error, debug};

#[derive(Clone)]
pub struct EgressProxy {
    port: u16,
    whitelist: Arc<RwLock<Vec<String>>>,
}

impl EgressProxy {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            whitelist: Arc::new(RwLock::new(vec![
                "huggingface.co".to_string(),
                "github.com".to_string(),
                "api.openai.com".to_string(),
                "tailscale.com".to_string(),
                // Add more default allowed domains
            ])),
        }
    }

    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let addr = SocketAddr::from(([127, 0, 0, 1], self.port));
        let listener = TcpListener::bind(addr).await?;
        info!("Egress Proxy listening on {}", addr);

        let whitelist = self.whitelist.clone();

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((socket, _)) => {
                        let wl = whitelist.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(socket, wl).await {
                                debug!("Proxy connection error: {}", e);
                            }
                        });
                    }
                    Err(e) => error!("Proxy accept error: {}", e),
                }
            }
        });

        Ok(())
    }

    pub fn add_to_whitelist(&self, domain: String) {
        if let Ok(mut wl) = self.whitelist.write() {
            wl.push(domain);
        }
    }
}

async fn handle_connection(mut client_socket: TcpStream, whitelist: Arc<RwLock<Vec<String>>>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Simple HTTP/CONNECT parsing
    let mut buf = [0u8; 4096];
    let n = client_socket.peek(&mut buf).await?;
    if n == 0 {
        return Ok(());
    }

    let request_str = String::from_utf8_lossy(&buf[..n]);
    
    // Check for CONNECT (HTTPS)
    let target_host = if request_str.starts_with("CONNECT") {
        // Parse "CONNECT host:port HTTP/1.1"
        request_str.split_whitespace().nth(1).map(|s| s.split(':').next().unwrap_or(s))
    } else {
        // Assume HTTP, look for Host header
        request_str.lines()
            .find(|l| l.to_lowercase().starts_with("host:"))
            .map(|l| l.split(':').nth(1).unwrap_or("").trim())
    };

    if let Some(host) = target_host {
        let allowed = {
            let wl = whitelist.read().unwrap();
            wl.iter().any(|domain| host.ends_with(domain))
        };

        if !allowed {
            warn!("Blocked outbound connection to: {}", host);
            // Send 403 Forbidden
            let response = "HTTP/1.1 403 Forbidden\r\n\r\nAccess Denied by Gumball Egress Filter";
            client_socket.write_all(response.as_bytes()).await?;
            return Ok(());
        }

        info!("Allowed connection to: {}", host);

        // If CONNECT, we need to establish tunnel
        if request_str.starts_with("CONNECT") {
            // Read the actual request to consume it from the buffer? 
            // No, peek didn't consume. We need to read it out.
            let mut read_buf = vec![0u8; n];
            client_socket.read_exact(&mut read_buf).await?; // Consume the CONNECT request header
            
            // Find the end of the header (\r\n\r\n) if it wasn't fully in the first peek?
            // Simplified: Assuming standard CONNECT request fits in 4k and we read it all.
            // In reality, we should read until double CRLF.
            
            // Connect to target
            // We need the port.
            let target = request_str.split_whitespace().nth(1).ok_or("Invalid CONNECT")?;
            let mut target_socket = TcpStream::connect(target).await?;

            // Send 200 OK to client
            client_socket.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await?;

            // Tunnel
            let (mut client_reader, mut client_writer) = client_socket.into_split();
            let (mut target_reader, mut target_writer) = target_socket.into_split();

            let client_to_target = tokio::io::copy(&mut client_reader, &mut target_writer);
            let target_to_client = tokio::io::copy(&mut target_reader, &mut client_writer);

            tokio::try_join!(client_to_target, target_to_client)?;
        } else {
            // Normal HTTP Proxying (Forwarding)
            // We need to parse the full URL or just forward to the Host?
            // Simple approach: Connect to Host:80 and forward everything.
            let host_port = if host.contains(':') { host.to_string() } else { format!("{}:80", host) };
            let mut target_socket = TcpStream::connect(host_port).await?;
            
            // We didn't consume the buffer yet (peek).
            // But we can't easily "un-peek".
            // We need to read from client_socket and write to target_socket.
            // But 'copy' consumes.
            
            // Wait, we need to ensure we don't lose the initial bytes.
            // Since we only peeked, the data is still in the socket buffer.
            // So we can just copy!
            
            let (mut client_reader, mut client_writer) = client_socket.into_split();
            let (mut target_reader, mut target_writer) = target_socket.into_split();
            
            let client_to_target = tokio::io::copy(&mut client_reader, &mut target_writer);
            let target_to_client = tokio::io::copy(&mut target_reader, &mut client_writer);
            
            tokio::try_join!(client_to_target, target_to_client)?;
        }
    } else {
        warn!("Could not determine target host, blocking.");
        client_socket.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await?;
    }

    Ok(())
}
