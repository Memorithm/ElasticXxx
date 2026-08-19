# ElasticXxx Literature & Mechanism Review

This directory tracks prior work relevant to adaptive, resource-aware, elastic, heterogeneous, and type-aware computing.

The review is intentionally **mechanism-oriented**. A paper is not included merely to support a broad statement; it is analyzed to determine exactly what problem it solves, how it represents resources, which variables it observes and controls, what guarantees it provides, what results it reports, and which mechanisms ElasticXxx should adopt, adapt, reject, or investigate.

## Review template

Each paper should answer, where the source supports an answer:

1. **Problem** — What exact problem is being solved?
2. **Resource model** — What counts as a resource?
3. **Observability** — What is measured or exposed?
4. **Decision variables** — What can change at runtime?
5. **Objective** — What is optimized?
6. **Constraints** — What must remain true?
7. **Planner / controller** — Rules, heuristic, DP, ILP, MPC, ML, etc.?
8. **Transition model** — How does the system move between configurations?
9. **Adaptation granularity** — At what level and frequency?
10. **Cost model** — Are planning and transition costs accounted for?
11. **Safety** — What prevents illegal or harmful adaptation?
12. **Reversibility** — Can transitions be rolled back?
13. **Results** — What is actually demonstrated or measured?
14. **Limitations** — What do the authors explicitly acknowledge?
15. **Elastic relation** — ADOPT / ADAPT / REJECT / INVESTIGATE.
16. **Alternative Elastic mechanism** — What might ElasticXxx do differently?
17. **Experiment** — How could the alternative be tested fairly?

## Evidence discipline

Every note must distinguish among:

- **SOURCE-DERIVED** — directly supported by the cited paper or authoritative publication record;
- **ELASTIC PROPOSAL** — our current design direction;
- **INFERENCE** — a conclusion drawn from the source and explicitly identified as such;
- **OPEN QUESTION** — not yet established;
- **EXPERIMENT REQUIRED** — requires implementation and measurement.

Do not claim novelty merely because a mechanism has not yet appeared in this review. Novelty requires a sufficiently broad literature search.

## Initial review queue

- [x] Moreau & Queinnec (2005), *Resource Aware Programming* — initial mechanism review
- [x] Teich, Schröder-Preikschat & Herkersdorf (2013), *Invasive Computing — Common Terms and Granularity of Invasion*
- [x] Qiao et al. (2021), *Pollux: Co-adaptive Cluster Scheduling for Goodput-Optimized Deep Learning*
- [ ] Alpa — automatic parallelization planning
- [ ] Dynamic / malleable HPC runtimes
- [ ] Heterogeneous-memory runtimes
- [ ] Model-predictive and learned resource control
- [ ] Resource-aware type systems
- [ ] Cloud elasticity and autoscaling
- [ ] Adaptive task runtimes and work stealing
- [ ] Energy- and thermal-aware execution
- [ ] LLM scheduling / KV-cache management / disaggregated serving

## Classification

- **ADOPT** — mechanism remains strong and general enough to retain substantially unchanged.
- **ADAPT** — underlying idea is useful, but the mechanism should be generalized or altered.
- **REJECT** — mechanism is incompatible with the emerging model or unnecessary for the intended scope.
- **INVESTIGATE** — evidence is insufficient; compare formally or experimentally before deciding.
