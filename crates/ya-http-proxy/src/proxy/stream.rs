//! Adaptation of `hyper::server::tcp::addr_stream::AddrStream` for use with TLS

use std::io;
use std::net::SocketAddr;
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, RawFd};
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;

pin_project_lite::pin_project! {
    #[derive(Debug)]
    pub struct TlsAddrStream {
        #[pin]
        inner: TlsStream<TcpStream>,
        remote_addr: SocketAddr,
    }
}

impl TlsAddrStream {
    pub fn new(tcp: TlsStream<TcpStream>, addr: SocketAddr) -> TlsAddrStream {
        TlsAddrStream {
            inner: tcp,
            remote_addr: addr,
        }
    }

    #[inline]
    pub fn remote_addr(&self) -> SocketAddr {
        self.remote_addr
    }

    #[allow(unused)]
    #[inline]
    pub fn into_inner(self) -> TlsStream<TcpStream> {
        self.inner
    }

    #[allow(unused)]
    pub fn poll_peek(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<usize>> {
        self.inner.get_mut().0.poll_peek(cx, buf)
    }
}

impl AsyncRead for TlsAddrStream {
    #[inline]
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.project().inner.poll_read(cx, buf)
    }
}

impl AsyncWrite for TlsAddrStream {
    #[inline]
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.project().inner.poll_write(cx, buf)
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    #[inline]
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.project().inner.poll_shutdown(cx)
    }

    #[inline]
    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        self.project().inner.poll_write_vectored(cx, bufs)
    }

    #[inline]
    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }
}

#[cfg(unix)]
impl AsRawFd for TlsAddrStream {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}
