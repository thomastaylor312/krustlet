use bindle::provider::Provider;
use bindle::Id;
use kubelet::container::PullPolicy;
use kubelet::secret::RegistryAuthResolver;
use kubelet::store::Store;
use oci_distribution::{secrets::RegistryAuth, Reference};
use tokio::stream::StreamExt;

pub struct BindleStore<P> {
    provider: P,
}

impl<P: Provider> BindleStore<P> {
    pub fn new(provider: P) -> Self {
        BindleStore { provider }
    }
}

#[async_trait::async_trait]
impl<P: Provider + Sync> Store for BindleStore<P> {
    async fn get(
        &self,
        image_ref: &Reference,
        _pull_policy: PullPolicy,
        _auth: &RegistryAuth, // not used right now, will be used later
    ) -> anyhow::Result<Vec<u8>> {
        // We don't care about the pull policy because bindles are immutable. The user can pass in
        // any type of Bindle provider that uses caching or doesn't use caching
        let (id, parcel_id) = into_id(image_ref);
        let mut stream = self.provider.get_parcel(id, &parcel_id).await?.fuse();

        let mut data = Vec::new();
        while let Some(res) = stream.next().await {
            let raw = res?;
            data.extend(raw.take(raw.remaining()));
        }
        Ok(data)
    }
}

fn into_id(image_ref: &Reference) -> (Id, String) {
    todo!()
}
