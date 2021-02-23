use std::time::Instant;
use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures::Future;

pub struct WrappedFuture<F> {
    name: String,
    inner: F,
}

impl<F> WrappedFuture<F> {
    pub fn new(name: impl AsRef<str>, fut: F) -> Self {
        WrappedFuture {
            name: name.as_ref().to_owned(),
            inner: fut,
        }
    }
}

impl<F: Future + Unpin> Future for WrappedFuture<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let now = Instant::now();
        let res = Pin::new(&mut self.inner).poll(cx);
        println!("Poll for future {}, took {:?}", self.name, now.elapsed());
        res
    }
}
