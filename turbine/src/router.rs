#![allow(dead_code, unused)]
#[allow(unused_imports)] // TODO: remove me
use {
    bytes::Bytes,
    std::{collections::HashMap, net::SocketAddr, sync::Arc},
    std::{future::Future, pin::Pin},
    tokio::sync::{
        mpsc::{error::TrySendError, Receiver as AsyncReceiver, Sender as AsyncSender},
        RwLock as AsyncRwLock,
    },
};

pub trait Router {
    fn insert(
        &self,
        addr: SocketAddr,
        tx: AsyncSender<Bytes>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;

    fn remove(&self, addr: &SocketAddr) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;

    fn clear(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;

    fn get(
        &self,
        addr: &SocketAddr,
    ) -> Pin<Box<dyn Future<Output = Option<AsyncSender<Bytes>>> + Send + '_>>;
}

/// Use for non-debugging run
///
/// Normal router, doesn't have any intercepting logic.
#[derive(Clone, Default)]
pub struct NormalRouter {
    inner: Arc<AsyncRwLock<HashMap<SocketAddr, AsyncSender<Bytes>>>>,
}

impl NormalRouter {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Router for NormalRouter {
    #[inline]
    fn insert(
        &self,
        addr: SocketAddr,
        tx: AsyncSender<Bytes>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            self.inner.write().await.insert(addr, tx);
        })
    }

    #[inline]
    fn remove(&self, addr: &SocketAddr) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            self.inner.write().await.remove(addr);
        })
    }

    #[inline]
    fn clear(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            self.inner.write().await.clear();
        })
    }

    #[inline]
    fn get(
        &self,
        addr: &SocketAddr,
    ) -> Pin<Box<dyn Future<Output = Option<AsyncSender<Bytes>>> + Send + '_>> {
        Box::pin(async move { self.inner.read().await.get(addr).cloned() })
    }
}

/// Use for debugging / testing
///
/// Router that has ability to intercept and log down router traffic.
#[derive(Clone, Default)]
pub struct MiddlewareRouter {
    inner: Arc<AsyncRwLock<HashMap<SocketAddr, AsyncSender<Bytes>>>>,
}

impl MiddlewareRouter {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Router for MiddlewareRouter {
    #[inline]
    fn insert(
        &self,
        addr: SocketAddr,
        tx: AsyncSender<Bytes>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        println!(
            "MiddlewareRouter: INSERT: addr {:?}",
            addr.clone(), /* how to intercept whats sent to channel */
        );
        unimplemented!()
    }

    #[inline]
    fn remove(&self, addr: &SocketAddr) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        println!(
            "MiddlewareRouter: REMOVE: addr {:?}",
            addr.clone(), /* how to intercept whats sent to channel */
        );
        unimplemented!()
    }

    #[inline]
    fn clear(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        println!(
            "MiddlewareRouter: CLEAR: addr {:?}",
            addr.clone(), /* how to intercept whats sent to channel */
        );
        unimplemented!()
    }

    #[inline]
    fn get(
        &self,
        addr: &SocketAddr,
    ) -> Pin<Box<dyn Future<Output = Option<AsyncSender<Bytes>>> + Send + '_>> {
        println!(
            "MiddlewareRouter: GET: addr {:?}",
            addr.clone(), /* how to intercept whats sent to channel */
        );
        unimplemented!()
    }
}
