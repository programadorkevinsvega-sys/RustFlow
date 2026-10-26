use std::{io, net::SocketAddr, sync::Arc, time::Duration};

use rustls::ServerConfig;
use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{mpsc, watch, Semaphore},
    time::timeout,
};
use tokio_rustls::{TlsAcceptor, server::TlsStream};

use crate::ServerError;

const MAX_CONCURRENT_TLS_HANDSHAKES: usize = 64;
const TLS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const TLS_ACCEPT_QUEUE_CAPACITY: usize = MAX_CONCURRENT_TLS_HANDSHAKES;
const ACCEPT_ERROR_RETRY_DELAY: Duration = Duration::from_millis(100);

/// Build a rustls server configuration from PEM-encoded certificate and key material.
pub fn tls_server_config_from_pem(
    certificate_pem: &[u8],
    private_key_pem: &[u8],
) -> Result<Arc<ServerConfig>, ServerError> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let certificates = CertificateDer::pem_slice_iter(certificate_pem)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| ServerError::InvalidTlsMaterial(error.to_string()))?;
    if certificates.is_empty() {
        return Err(ServerError::InvalidTlsMaterial(
            "certificate PEM did not contain a certificate".to_owned(),
        ));
    }
    let key = PrivateKeyDer::from_pem_slice(private_key_pem)
        .map_err(|error| ServerError::InvalidTlsMaterial(error.to_string()))?;
    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certificates, key)
        .map_err(|error| ServerError::InvalidTlsMaterial(error.to_string()))?;
    Ok(Arc::new(config))
}

/// Accepts TCP connections independently from their TLS handshakes.
///
/// Handshakes run concurrently under a fixed cap and time out, so a peer that
/// opens TCP but never sends a complete ClientHello cannot block acceptance of
/// later connections indefinitely.
pub(crate) struct TlsListener {
    local_addr: SocketAddr,
    accepted: mpsc::Receiver<(TlsStream<TcpStream>, SocketAddr)>,
    shutdown: watch::Sender<bool>,
}

impl TlsListener {
    pub(crate) fn new(listener: TcpListener, config: Arc<ServerConfig>) -> io::Result<Self> {
        let local_addr = listener.local_addr()?;
        let acceptor = TlsAcceptor::from(config);
        let (accepted_tx, accepted) =
            mpsc::channel::<(TlsStream<TcpStream>, SocketAddr)>(TLS_ACCEPT_QUEUE_CAPACITY);
        let (shutdown, mut shutdown_rx) = watch::channel(false);

        tokio::spawn(async move {
            let handshake_slots = Arc::new(Semaphore::new(MAX_CONCURRENT_TLS_HANDSHAKES));

            loop {
                tokio::select! {
                    changed = shutdown_rx.changed() => {
                        if changed.is_err() || *shutdown_rx.borrow() {
                            break;
                        }
                    }
                    result = listener.accept() => {
                        match result {
                            Ok((stream, addr)) => {
                                let permit = match Arc::clone(&handshake_slots).try_acquire_owned() {
                                    Ok(permit) => permit,
                                    Err(_) => {
                                        tracing::warn!(
                                            peer = %addr,
                                            limit = MAX_CONCURRENT_TLS_HANDSHAKES,
                                            "TLS handshake concurrency limit reached; dropping connection"
                                        );
                                        drop(stream);
                                        continue;
                                    }
                                };

                                let acceptor = acceptor.clone();
                                let accepted_tx = accepted_tx.clone();
                                let mut connection_shutdown = shutdown_rx.clone();
                                tokio::spawn(async move {
                                    let _permit = permit;
                                    if *connection_shutdown.borrow() {
                                        return;
                                    }

                                    tokio::select! {
                                        _ = connection_shutdown.changed() => {}
                                        result = timeout(TLS_HANDSHAKE_TIMEOUT, acceptor.accept(stream)) => {
                                            match result {
                                                Ok(Ok(tls_stream)) => {
                                                    let _ = accepted_tx.send((tls_stream, addr)).await;
                                                }
                                                Ok(Err(error)) => {
                                                    tracing::debug!(
                                                        peer = %addr,
                                                        error = %error,
                                                        "TLS handshake failed"
                                                    );
                                                }
                                                Err(_) => {
                                                    tracing::warn!(
                                                        peer = %addr,
                                                        timeout_seconds = TLS_HANDSHAKE_TIMEOUT.as_secs(),
                                                        "TLS handshake timed out"
                                                    );
                                                }
                                            }
                                        }
                                    }
                                });
                            }
                            Err(error) => {
                                tracing::warn!(error = %error, "TLS TCP accept failed");
                                tokio::time::sleep(ACCEPT_ERROR_RETRY_DELAY).await;
                            }
                        }
                    }
                }
            }
        });

        Ok(Self {
            local_addr,
            accepted,
            shutdown,
        })
    }
}

impl Drop for TlsListener {
    fn drop(&mut self) {
        let _ = self.shutdown.send(true);
    }
}

impl axum::serve::Listener for TlsListener {
    type Io = TlsStream<TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            if let Some(connection) = self.accepted.recv().await {
                return connection;
            }

            tracing::error!("TLS accept task stopped unexpectedly");
            tokio::time::sleep(ACCEPT_ERROR_RETRY_DELAY).await;
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        Ok(self.local_addr)
    }
}
