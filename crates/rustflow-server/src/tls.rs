use std::{io, net::SocketAddr, sync::Arc};

use rustls::ServerConfig;
use rustls_pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{TlsAcceptor, server::TlsStream};

use crate::ServerError;

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

pub(crate) struct TlsListener {
    listener: TcpListener,
    acceptor: TlsAcceptor,
}

impl TlsListener {
    pub(crate) fn new(listener: TcpListener, config: Arc<ServerConfig>) -> Self {
        Self {
            listener,
            acceptor: TlsAcceptor::from(config),
        }
    }
}

impl axum::serve::Listener for TlsListener {
    type Io = TlsStream<TcpStream>;
    type Addr = SocketAddr;

    fn accept(&mut self) -> impl std::future::Future<Output = (Self::Io, Self::Addr)> + Send {
        async move {
            loop {
                match self.listener.accept().await {
                    Ok((stream, addr)) => match self.acceptor.accept(stream).await {
                        Ok(tls) => return (tls, addr),
                        Err(error) => {
                            tracing::warn!(error = %error, "TLS handshake failed");
                        }
                    },
                    Err(error) => {
                        tracing::warn!(error = %error, "TLS TCP accept failed");
                    }
                }
            }
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.listener.local_addr()
    }
}
