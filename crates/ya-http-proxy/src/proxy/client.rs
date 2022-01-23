use std::{fs, io};

use hyper::client::{Builder, Client, HttpConnector};
use hyper_rustls::{ConfigBuilderExt, HttpsConnector, HttpsConnectorBuilder};

use crate::conf::ClientConf;
use crate::conf_builder_client;
use crate::error::{Error, TlsError};

pub fn build(conf: &ClientConf) -> Client<HttpConnector> {
    builder(conf).build_http()
}

#[allow(unused)]
pub fn build_tls(conf: &ClientConf) -> Result<Client<HttpsConnector<HttpConnector>>, Error> {
    let tls_conf = match conf.client_cert.client_ca_cert_store_path {
        Some(ref path) => {
            let file = fs::File::open(path).map_err(|e| {
                TlsError::ClientCertStore(format!("cannot open '{}': {}", path.display(), e))
            })?;

            let mut reader = io::BufReader::new(file);
            let certs = rustls_pemfile::certs(&mut reader).map_err(|e| {
                TlsError::ClientCertStore(format!("error reading '{}': {}", path.display(), e))
            })?;

            let mut store = rustls::RootCertStore::empty();
            store.add_parsable_certificates(&certs);

            rustls::ClientConfig::builder()
                .with_safe_defaults()
                .with_root_certificates(store)
                .with_no_client_auth()
        }
        None => rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_native_roots()
            .with_no_client_auth(),
    };

    let https = HttpsConnectorBuilder::new()
        .with_tls_config(tls_conf)
        .https_or_http()
        .enable_http1()
        .enable_http2()
        .build();

    Ok(builder(conf).build(https))
}

fn builder(conf: &ClientConf) -> Builder {
    let mut builder = Client::builder();
    let mut target = &mut builder;
    conf_builder_client!(target, conf);
    builder
}
