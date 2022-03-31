use std::io::{Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;
use std::{fs, io};

use futures::SinkExt;
use hyper::server::accept::Accept;
use hyper::server::{accept, Builder, Server};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

use crate::conf::ServerConf;
use crate::conf_builder_server;
use crate::error::{Error, TlsError};
use crate::proxy::stream::HttpStream;

pub async fn listen_http(
    conf: &ServerConf,
) -> Result<Option<Builder<impl Accept<Conn = HttpStream, Error = std::io::Error>>>, Error> {
    let addrs = match conf.bind_http.as_ref() {
        Some(addrs) => addrs.to_vec(),
        None => return Ok(None),
    };

    let tcp_listener = TcpListener::bind(addrs.as_slice()).await?;
    let (tx, rx) = futures::channel::mpsc::channel(64);

    tokio::task::spawn(async move {
        loop {
            match tcp_listener.accept().await {
                Ok((stream, addr)) => {
                    let mut tx = tx.clone();
                    tokio::task::spawn(async move {
                        let stream = HttpStream::plain(stream, addr);
                        let _ = tx.send(Ok(stream)).await;
                    });
                }
                // FIXME: handle network errors
                Err(err) => match tcp_listener.local_addr() {
                    Ok(_) => log::debug!("Client error: {}", err),
                    Err(_) => {
                        log::error!("Network error: {}", err);
                        break;
                    }
                },
            }
        }
    });

    let acceptor = accept::from_stream(rx);
    let mut builder = Server::builder(acceptor);
    conf_builder_server!(builder, conf);

    Ok(Some(builder))
}

pub async fn listen_https(
    conf: &ServerConf,
) -> Result<Option<Builder<impl Accept<Conn = HttpStream, Error = std::io::Error>>>, Error> {
    let addrs = match conf.bind_https.as_ref() {
        Some(addrs) => addrs.to_vec(),
        None => return Ok(None),
    };

    let tls_conf = read_tls_conf(conf)?;
    let tcp_listener = TcpListener::bind(addrs.as_slice()).await?;
    let tls_acceptor = TlsAcceptor::from(tls_conf);
    let (tx, rx) = futures::channel::mpsc::channel(64);

    tokio::task::spawn(async move {
        loop {
            match tcp_listener.accept().await {
                Ok((socket, addr)) => {
                    let tls_acceptor = tls_acceptor.clone();
                    let mut tx = tx.clone();

                    // perform TLS handshakes in background
                    tokio::task::spawn(async move {
                        match tls_acceptor.accept(socket).await {
                            Ok(stream) => {
                                let stream = HttpStream::tls(stream, addr);
                                let _ = tx.send(Ok(stream)).await;
                            }
                            Err(error) => log::warn!("[{}] TLS error: {}", addr, error),
                        }
                    });
                }
                // FIXME: handle network errors
                Err(err) => match tcp_listener.local_addr() {
                    Ok(_) => log::debug!("Client error: {}", err),
                    Err(_) => {
                        log::error!("Network error: {}", err);
                        break;
                    }
                },
            }
        }
    });

    let acceptor = accept::from_stream(rx);
    let mut builder = Server::builder(acceptor);
    conf_builder_server!(builder, conf);

    Ok(Some(builder))
}

fn read_tls_conf(conf: &ServerConf) -> Result<Arc<rustls::ServerConfig>, Error> {
    let store = match conf.server_cert.server_cert_store_path.clone() {
        Some(path) => read_cert_store(path)?,
        None => return Err(TlsError::ServerCertStore("path not set".to_string()).into()),
    };
    let key = match conf.server_cert.server_key_path.clone() {
        Some(path) => read_cert_key(path)?,
        None => return Err(TlsError::ServerCertKey("path not set".to_string()).into()),
    };

    let mut cfg = rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(store, key)
        .map_err(|e| TlsError::Other(e.to_string()))?;

    if conf.http1_only.unwrap_or(false) {
        cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
    } else if conf.server_common.http2_only.unwrap_or(false) {
        cfg.alpn_protocols = vec![b"h2".to_vec()];
    } else {
        cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    }

    Ok(Arc::new(cfg))
}

fn read_cert_store(path: impl AsRef<Path>) -> Result<Vec<rustls::Certificate>, Error> {
    let path = path.as_ref();
    let file = fs::File::open(&path).map_err(|e| {
        TlsError::ServerCertStore(format!("cannot open '{}': {}", path.display(), e))
    })?;
    let mut reader = io::BufReader::new(file);

    let store = rustls_pemfile::certs(&mut reader).map_err(|e| {
        TlsError::ServerCertStore(format!("error reading '{}': {}", path.display(), e))
    })?;
    Ok(store.into_iter().map(rustls::Certificate).collect())
}

fn read_cert_key(path: impl AsRef<Path>) -> Result<rustls::PrivateKey, Error> {
    let path = path.as_ref();
    let file = fs::File::open(&path)
        .map_err(|e| TlsError::ServerCertKey(format!("cannot open '{}': {}", path.display(), e)))?;
    let mut reader = io::BufReader::new(file);

    let mut keys = rustls_pemfile::rsa_private_keys(&mut reader).map_err(|e| {
        TlsError::ServerCertKey(format!("error reading '{}': {}", path.display(), e))
    })?;

    if keys.is_empty() {
        reader.seek(SeekFrom::Start(0))?;
        keys = rustls_pemfile::pkcs8_private_keys(&mut reader).map_err(|e| {
            TlsError::ServerCertKey(format!("error reading '{}': {}", path.display(), e))
        })?;
    }

    if keys.is_empty() {
        return Err(TlsError::ServerCertKey("missing server private key".to_string()).into());
    } else if keys.len() > 1 {
        return Err(TlsError::ServerCertKey("expected a single private key".to_string()).into());
    }

    Ok(rustls::PrivateKey(keys.remove(0)))
}
