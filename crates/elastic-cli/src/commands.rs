pub fn inspect(id: &str) {
    println!("Inspecting resource: {id}");
}

pub fn observe(id: &str) {
    println!("Observing resource: {id}");
}

pub fn plan(id: &str) {
    println!("Planning for resource: {id}");
}

pub fn validate(id: &str) {
    println!("Validating resource: {id}");
}

pub fn apply(id: &str) {
    println!("Applying changes to resource: {id}");
}

pub fn run(id: &str) {
    println!("Running runtime for resource: {id}");
}

pub fn watch(id: &str, interval_ms: Option<u64>) {
    println!("Watching resource: {id} interval: {:?}", interval_ms);
}

pub fn explain(id: &str) {
    println!("Explaining resource: {id}");
}
