# Elastic Runtime Configuration Schema v1

Versioned declarative configuration for Elastic runtime.

## Format

JSON with schema version field.

```json
{
  "schema_version": "1.0",
  "resources": [
    {
      "id": "ram-budget",
      "class": "CAPACITY_RESOURCE",
      "dimensions": ["CAPACITY"],
      "invariants": ["PreserveContents"],
      "objectives": ["MEMORY_FOOTPRINT"],
      "transitions": [
        {
          "mechanism": "Reinterpret",
          "dimension": "CAPACITY",
          "capability_required": true
        }
      ]
    }
  ],
  "planner": {
    "type": "threshold",
    "high_watermark": 0.8,
    "low_watermark": 0.3,
    "step_fraction": 0.2
  },
  "cadence": "OneShot",
  "mode": "Apply",
  "max_cycles": 0,
  "interval_ms": 1000,
  "emit_events": true,
  "dry_run": false
}
```

## Fields

- `schema_version`: e.g. "1.0"
- `resources`: list of resource declarations
- `planner`: planner configuration
- `cadence`: OneShot | Periodic
- `mode`: OneShot | Periodic | DryRun | ObserveOnly | Apply
- `max_cycles`: 0 = infinite
- `interval_ms`: polling interval for periodic mode
- `emit_events`: boolean
- `dry_run`: boolean
