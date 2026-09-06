#!/usr/bin/env python3
"""Validate the frozen ElasticBitAllocation Stage-B preregistration.

This validator deliberately checks identities and research gates only. It does
not download the model/dataset, execute NNIS, derive acceptance thresholds, or
authorize final-test access.
"""

from __future__ import annotations

import argparse
import copy
import json
import re
from pathlib import Path
from typing import Any

SCHEMA = "elastic-bit-allocation-stage-b-preregistration-v1"
MODEL_REVISION = "93efa2f097d58c2a74874c7e644dbc9b0cee75a2"
MODEL_SHA256 = "80521b40281d6ce74e35c9282c22539e75aa0ac8578892b2a59955ef78d55da1"
TOKENIZER_SHA256 = "9ca9acddb6525a194ec8ac7a87f24fbba7232a9a15ffa1af0c1224fcd888e47c"
DATASET_REVISION = "b08601e04326c79dfdd32d625aee71d232d685c3"
NNIS_REVISION = "b9d2f1e74bb68dfa90ca499a24ce857d15e7fb02"

DATASET_FILES = {
    "train": (
        "wikitext-2-raw-v1/train-00000-of-00001.parquet",
        "e83889baabc497075506f91975be5fac0d45c5290b6b20582c8cd1e853d0c9f7",
        36718,
    ),
    "validation": (
        "wikitext-2-raw-v1/validation-00000-of-00001.parquet",
        "204929b7ff9d6184953f867dedb860e40aa69c078fc1e54b3baaa8fb28511c4c",
        3760,
    ),
    "test": (
        "wikitext-2-raw-v1/test-00000-of-00001.parquet",
        "5f1bea067869d04849c0f975a2b29c4ff47d867f484f5010ea5e861eab246d91",
        4358,
    ),
}

THRESHOLD_FIELDS = {
    "max_quality_degradation",
    "min_physical_storage_improvement",
    "max_steady_state_latency_regression",
    "max_conversion_or_setup_cost_or_amortization_horizon",
    "reproducibility_tolerance_and_run_policy",
}


class ValidationError(ValueError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValidationError(message)


def exact_keys(value: dict[str, Any], expected: set[str], where: str) -> None:
    actual = set(value)
    require(actual == expected, f"{where} keys differ: expected {sorted(expected)}, got {sorted(actual)}")


def is_sha256(value: Any) -> bool:
    return isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value) is not None


def no_duplicate_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ValidationError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def validate_manifest(data: dict[str, Any]) -> None:
    exact_keys(
        data,
        {
            "schema",
            "status",
            "issue",
            "purpose",
            "model",
            "dataset",
            "tokenization",
            "data_partitions",
            "quality",
            "runtime",
            "hardware",
            "performance_protocol",
            "representation_accounting",
            "required_fixed_baseline_families_before_allocator_go",
            "candidate_rules",
            "acceptance_thresholds",
            "authorization",
        },
        "root",
    )
    require(data["schema"] == SCHEMA, "unexpected schema")
    require(data["status"] == "preregistered_not_executed", "Stage B must remain unexecuted")
    require(data["issue"] == 29, "preregistration must remain bound to issue #29")
    require(
        data["purpose"] == "fixed_real_model_baselines_before_any_elastic_allocator_or_search",
        "unexpected preregistration purpose",
    )

    model = data["model"]
    exact_keys(
        model,
        {
            "repository",
            "revision",
            "source_model_sha256",
            "source_weight_dtype",
            "tokenizer_sha256",
            "declared_license",
        },
        "model",
    )
    require(model["repository"] == "HuggingFaceTB/SmolLM2-135M", "unexpected model repository")
    require(model["revision"] == MODEL_REVISION, "unexpected model revision")
    require(model["source_model_sha256"] == MODEL_SHA256, "unexpected model hash")
    require(is_sha256(model["source_model_sha256"]), "model hash must be lowercase SHA-256")
    require(model["source_weight_dtype"] == "bfloat16", "unexpected source weight dtype")
    require(model["tokenizer_sha256"] == TOKENIZER_SHA256, "unexpected tokenizer hash")
    require(is_sha256(model["tokenizer_sha256"]), "tokenizer hash must be lowercase SHA-256")
    require(model["declared_license"] == "apache-2.0", "unexpected declared model license")

    dataset = data["dataset"]
    exact_keys(
        dataset,
        {
            "repository",
            "revision",
            "config",
            "declared_license_metadata",
            "license_metadata_is_provenance_not_legal_interpretation",
            "files",
        },
        "dataset",
    )
    require(dataset["repository"] == "Salesforce/wikitext", "unexpected dataset repository")
    require(dataset["revision"] == DATASET_REVISION, "unexpected dataset revision")
    require(dataset["config"] == "wikitext-2-raw-v1", "unexpected dataset config")
    require(
        dataset["declared_license_metadata"] == ["cc-by-sa-3.0", "gfdl"],
        "dataset license metadata drifted",
    )
    require(
        dataset["license_metadata_is_provenance_not_legal_interpretation"] is True,
        "dataset license metadata boundary must remain explicit",
    )
    exact_keys(dataset["files"], set(DATASET_FILES), "dataset.files")
    for split, (path, sha256, rows) in DATASET_FILES.items():
        entry = dataset["files"][split]
        exact_keys(entry, {"path", "sha256", "rows"}, f"dataset.files.{split}")
        require(entry["path"] == path, f"{split} path drifted")
        require(entry["sha256"] == sha256, f"{split} hash drifted")
        require(is_sha256(entry["sha256"]), f"{split} hash must be lowercase SHA-256")
        require(entry["rows"] == rows, f"{split} row count drifted")

    tokenization = data["tokenization"]
    exact_keys(
        tokenization,
        {
            "tokenizer_identity",
            "text_field",
            "row_order",
            "normalization",
            "row_join",
            "add_special_tokens",
            "sequence_length",
        },
        "tokenization",
    )
    require(tokenization["tokenizer_identity"] == "model.tokenizer_sha256", "tokenizer identity must be model-bound")
    require(tokenization["text_field"] == "text", "unexpected text field")
    require(tokenization["row_order"] == "parquet_row_order", "row order must remain deterministic")
    require(tokenization["normalization"] == "none", "text normalization is forbidden")
    require(
        tokenization["row_join"] == "concatenate_exact_utf8_text_fields_without_inserted_separator",
        "row concatenation rule drifted",
    )
    require(tokenization["add_special_tokens"] is False, "special-token insertion must remain disabled")
    require(tokenization["sequence_length"] == 2048, "sequence length drifted")

    partitions = data["data_partitions"]
    exact_keys(partitions, {"calibration", "development", "final_test"}, "data_partitions")
    calibration = partitions["calibration"]
    development = partitions["development"]
    final_test = partitions["final_test"]
    exact_keys(
        calibration,
        {"split", "rule", "sequences", "sequence_length", "candidate_fitting_allowed", "acceptance_threshold_selection_allowed"},
        "data_partitions.calibration",
    )
    require(calibration["split"] == "train", "calibration must use train")
    require(calibration["rule"] == "first_262144_token_ids_after_full_split_concatenate_and_tokenize", "calibration rule drifted")
    require(calibration["sequences"] == 128 and calibration["sequence_length"] == 2048, "calibration geometry drifted")
    require(calibration["candidate_fitting_allowed"] is True, "calibration must remain the only fitting partition")
    require(calibration["acceptance_threshold_selection_allowed"] is False, "calibration cannot select thresholds")

    exact_keys(
        development,
        {"split", "rule", "candidate_fitting_allowed", "acceptance_threshold_selection_allowed"},
        "data_partitions.development",
    )
    require(development["split"] == "validation", "development must use validation")
    require(
        development["rule"] == "all_non_overlapping_complete_2048_token_blocks_drop_final_partial_block",
        "development block rule drifted",
    )
    require(development["candidate_fitting_allowed"] is False, "development fitting is forbidden")
    require(development["acceptance_threshold_selection_allowed"] is True, "development must be the threshold-selection partition")

    exact_keys(
        final_test,
        {"split", "rule", "candidate_fitting_allowed", "acceptance_threshold_selection_allowed", "access"},
        "data_partitions.final_test",
    )
    require(final_test["split"] == "test", "final test must use test")
    require(
        final_test["rule"] == "all_non_overlapping_complete_2048_token_blocks_drop_final_partial_block",
        "final-test block rule drifted",
    )
    require(final_test["candidate_fitting_allowed"] is False, "final-test fitting is forbidden")
    require(final_test["acceptance_threshold_selection_allowed"] is False, "final test cannot select thresholds")
    require(final_test["access"] == "locked_until_followup_threshold_record_is_frozen", "final test must remain locked")
    require(
        {calibration["split"], development["split"], final_test["split"]} == {"train", "validation", "test"},
        "data partitions must remain split-disjoint",
    )

    quality = data["quality"]
    require(quality["primary_metric"] == "mean_next_token_negative_log_likelihood", "primary quality metric drifted")
    require(quality["derived_metric"] == "perplexity_exp_mean_nll", "derived quality metric drifted")
    require(quality["dense_reference_required"] is True, "dense reference is mandatory")
    require(quality["final_test_is_confirmatory_only"] is True, "final test must remain confirmatory")

    runtime = data["runtime"]
    require(runtime["owner_repository"] == "Memorithm/NNIS", "runtime ownership drifted")
    require(runtime["revision"] == NNIS_REVISION, "NNIS revision drifted")
    require(runtime["rust_msrv"] == "1.77", "NNIS MSRV drifted")
    require(runtime["reference_execution_weight_dtype"] == "f32", "reference execution dtype drifted")
    require(runtime["low_bit_full_model_runtime_qualified_at_preregistration"] is False, "low-bit runtime qualification must not be fabricated")
    require(runtime["structural_full_model_runtime_qualified_at_preregistration"] is False, "structural runtime qualification must not be fabricated")

    hardware = data["hardware"]
    require(hardware["device"] == "NVIDIA Jetson AGX Thor", "hardware target drifted")
    require(hardware["device_ordinal"] == 0, "device ordinal drifted")
    require(hardware["power_mode"] == "MAXN", "power mode drifted")
    for key in (
        "same_physical_device_required_for_all_compared_candidates",
        "same_power_clock_thermal_policy_required",
        "capture_nvpmodel",
        "capture_jetson_clocks_show",
        "capture_driver_nvrtc_cuda_identity",
        "unsupported_or_unavailable_hardware_counter_is_explicit_not_zero",
    ):
        require(hardware[key] is True, f"hardware gate {key} must remain enabled")

    performance = data["performance_protocol"]
    require(performance["prompt_text"] == "Gravity is", "performance prompt drifted")
    require(performance["prompt_ids"] == [22007, 6463, 314], "performance prompt ids drifted")
    require(performance["decode_steps"] == 32, "decode length drifted")
    require(performance["decoding"] == "greedy", "decoding policy drifted")
    require(performance["warmups_per_process"] == 2, "warmup count drifted")
    require(performance["measured_iterations_per_process"] == 5, "iteration count drifted")
    require(performance["independent_processes"] == 5, "process count drifted")
    require(performance["primary_latency_metric"] == "request_total_ms", "latency metric drifted")
    require(performance["latency_summary"] == ["median", "p95"], "latency summary drifted")
    for key in (
        "model_load_excluded_from_request_total",
        "fresh_session_per_measured_request",
        "materialization_or_conversion_time_reported_separately",
        "peak_temporary_memory_reported_separately",
        "memory_traffic_counters_reported_when_available",
    ):
        require(performance[key] is True, f"performance gate {key} must remain enabled")

    accounting = data["representation_accounting"]
    require(accounting["experiment_scope"] == "model_weights_only", "weight experiment scope drifted")
    require(accounting["kv_cache_is_separate_experiment"] is True, "KV must remain a separate experiment")
    require(accounting["shared_segment_rule"] == "count_each_canonical_physical_segment_identity_exactly_once", "shared-segment rule drifted")
    require(accounting["observational_cuda_free_memory_delta_is_not_exact_resident_accounting"] is True, "CUDA free-memory deltas cannot become exact accounting")
    require(accounting["candidate_without_exact_serialized_and_resident_accounting_is_inadmissible"] is True, "inexact candidates must fail closed")

    baselines = data["required_fixed_baseline_families_before_allocator_go"]
    require(baselines["dense_reference"] is True, "dense reference baseline is mandatory")
    require(baselines["fixed_low_bit_at_least_one_verified_4_bit"] is True, "4-bit baseline is mandatory")
    require(baselines["fixed_low_bit_at_least_one_verified_2_bit_or_lower"] is True, "<=2-bit baseline is mandatory")
    require(
        baselines["structural_at_least_one"] == ["sparse", "low_rank", "codebook", "heterogeneous_residual"],
        "structural baseline family set drifted",
    )

    rules = data["candidate_rules"]
    for key, value in rules.items():
        require(value is True, f"candidate rule {key} must remain enabled")

    thresholds = data["acceptance_thresholds"]
    exact_keys(thresholds, {"status", *THRESHOLD_FIELDS}, "acceptance_thresholds")
    require(
        thresholds["status"] == "unfrozen_pending_dense_and_fixed_baseline_evidence_on_development_split",
        "acceptance thresholds must remain unfrozen in preregistration v1",
    )
    for field in THRESHOLD_FIELDS:
        require(thresholds[field] is None, f"acceptance threshold {field} must remain null before baseline evidence")

    authorization = data["authorization"]
    exact_keys(
        authorization,
        {
            "final_test_execution_authorized",
            "elastic_allocator_or_search_authorized",
            "real_model_result_claimed",
            "performance_claimed",
            "scientific_novelty_claimed",
        },
        "authorization",
    )
    for key, value in authorization.items():
        require(value is False, f"authorization/claim {key} must remain false in preregistration v1")


def load_manifest(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as handle:
        value = json.load(handle, object_pairs_hook=no_duplicate_object)
    require(isinstance(value, dict), "manifest root must be an object")
    return value


def self_test(data: dict[str, Any]) -> None:
    mutations: list[tuple[str, Any]] = []

    with_threshold = copy.deepcopy(data)
    with_threshold["acceptance_thresholds"]["max_quality_degradation"] = 0.01
    mutations.append(("premature threshold", with_threshold))

    unlocked_test = copy.deepcopy(data)
    unlocked_test["data_partitions"]["final_test"]["access"] = "unlocked"
    mutations.append(("unlocked final test", unlocked_test))

    allocator_enabled = copy.deepcopy(data)
    allocator_enabled["authorization"]["elastic_allocator_or_search_authorized"] = True
    mutations.append(("premature allocator authorization", allocator_enabled))

    fabricated_low_bit = copy.deepcopy(data)
    fabricated_low_bit["runtime"]["low_bit_full_model_runtime_qualified_at_preregistration"] = True
    mutations.append(("fabricated low-bit qualification", fabricated_low_bit))

    for name, mutated in mutations:
        try:
            validate_manifest(mutated)
        except ValidationError:
            continue
        raise AssertionError(f"validator accepted forbidden mutation: {name}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("manifest", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    try:
        manifest = load_manifest(args.manifest)
        validate_manifest(manifest)
        if args.self_test:
            self_test(manifest)
    except (OSError, json.JSONDecodeError, ValidationError, AssertionError) as error:
        print(f"Stage-B preregistration validation failed: {error}")
        return 1

    print("Stage-B preregistration validation passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
