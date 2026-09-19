use elastic::prelude::*;

elastic! {
    pub document inference_stack {
        resource worker_pool {
            class(shared);
            allow(parallelism);
            admit(reinterpret @ parallelism);
            capability(reinterpret @ parallelism);
        }
        resource session_kv {
            class(representational);
            allow(representation, residency);
            preserve(contents);
            optimize(latency, memory_footprint);
            observe(free_capacity);
        }
    }
}

fn main() -> Result<(), ElasticDocumentError> {
    let document = inference_stack::document()?;
    println!(
        "{} resources fingerprint={}",
        document.resources().len(),
        document.fingerprint()
    );
    Ok(())
}
