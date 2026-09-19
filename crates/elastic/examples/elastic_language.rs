use elastic::prelude::*;

elastic! {
    pub resource inference_budget {
        class(configurational);
        id("inference-budget");
        allow(concurrency, energy);
        preserve(identity);
        optimize(latency, energy);
        observe(utilization, thermal_margin, energy_rate);
        admit(reinterpret @ concurrency);
        capability(reinterpret @ concurrency);
    }
}

fn main() -> Result<(), ResourceSpecError> {
    let spec = inference_budget::resource_spec()?;
    println!("{spec}");
    Ok(())
}
