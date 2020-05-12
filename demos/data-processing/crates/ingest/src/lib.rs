extern crate wascc_actor as actor;
extern crate wascc_codec;

use actor::prelude::*;

use wascc_codec::blobstore::Blob;

actor_handlers! {
    codec::http::OP_HANDLE_REQUEST => ingest,
    codec::core::OP_HEALTH_REQUEST => health
}
const FILE_NAME: &str = "work";

fn ingest(r: codec::http::Request) -> HandlerResult<codec::http::Response> {
    // k8s volumes are mounted into the waSCC runtime using the same volume mount name
    let first_partition = objectstore::host("partition-1");
    let second_partition = objectstore::host("partition-2");

    match r.method.as_str() {
        "POST" => {
            let data: Vec<isize> = serde_json::from_slice(&r.body)?;
            let mut chunks = data.chunks(data.len() / 2 + 1);
            let chunk1 = serde_json::to_vec(chunks.next().unwrap()).unwrap();
            let chunk2 = serde_json::to_vec(chunks.next().unwrap()).unwrap();
            let blob = Blob {
                id: FILE_NAME.to_owned(),
                container: "".to_owned(),
                byte_size: chunk1.len() as u64,
            };
            // TODO: check if this is the start of an upload or another chunk. Right now we accept the request as the only chunk.
            let transfer =
                first_partition.start_upload(&blob, chunk1.len() as u64, chunk1.len() as u64)?;
            first_partition.upload_chunk(&transfer, 0, &chunk1)?;

            let blob = Blob {
                id: FILE_NAME.to_owned(),
                container: "".to_owned(),
                byte_size: chunk2.len() as u64,
            };
            let transfer =
                second_partition.start_upload(&blob, chunk2.len() as u64, chunk2.len() as u64)?;
            second_partition.upload_chunk(&transfer, 0, &chunk2)?;
            Ok(codec::http::Response::ok())
        }
        _ => Ok(codec::http::Response::bad_request()),
    }
}

fn health(_req: codec::core::HealthRequest) -> HandlerResult<()> {
    Ok(())
}
