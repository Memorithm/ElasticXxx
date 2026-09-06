use elastic_kv::{run_fixed_baseline, SyntheticCorpus};

const FROZEN_BASELINE: &str = include_str!("data/elastic-bit-allocation-baseline-v1.txt");

#[test]
fn frozen_stage_a_baseline_matches_the_executable_protocol() {
    let records = run_fixed_baseline(&SyntheticCorpus::deterministic())
        .expect("frozen Stage A baseline must remain executable")
        .into_iter()
        .map(|result| result.canonical_record())
        .collect::<Vec<_>>()
        .join("\n");
    let actual = format!("{records}\n");

    assert_eq!(actual, FROZEN_BASELINE);
}
