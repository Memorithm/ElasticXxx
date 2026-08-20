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
- [x] Blumofe et al. (1996), *Cilk: An Efficient Multithreaded Runtime System*
- [x] Agrawal, He & Leiserson (2007), *Adaptive Work Stealing with Parallelism Feedback*
- [x] Zanini et al. (2009), *Multicore Thermal Management with Model Predictive Control*
- [x] Hoffmann et al. (2011), *Dynamic Knobs for Responsive Power-Aware Computing*
- [x] Teich, Schröder-Preikschat & Herkersdorf (2013), *Invasive Computing — Common Terms and Granularity of Invasion*
- [x] Eastep et al. (2017), *Global Extensible Open Power Manager*
- [x] Qiao et al. (2021), *Pollux: Co-adaptive Cluster Scheduling for Goodput-Optimized Deep Learning*
- [x] Zheng et al. (2022), *Alpa: Automating Inter- and Intra-Operator Parallelism for Distributed Deep Learning*
- [x] Wang et al. (2023), *BWoS: Formally Verified Block-based Work Stealing for Parallel Processing*
- [x] Huber et al. (2024), *Design Principles of Dynamic Resource Management for High-Performance Parallel Programming Models*
- [x] Sandås et al. (2026), *Seamless Execution of Malleable Applications in Controlled and Production HPC Environments*
- [x] Dynamic / malleable HPC runtimes — representative design + production reviews complete; additional papers may be added as mechanisms require
- [x] Xiang et al. (2024), *NOMAD: Non-Exclusive Memory Tiering via Transactional Page Migration*
- [x] Xu et al. (2024), *FlexMem: Adaptive Page Profiling and Migration for Tiered Memory*
- [x] Liu et al. (2025), *Tiered Memory Management Beyond Hotness*
- [x] Heterogeneous-memory runtimes — representative transition, feedback, and performance-driven tiering reviews complete; additional papers may be added as mechanisms require
- [x] Mandal et al. (2020), *Online Adaptive Learning for Runtime Resource Management of Heterogeneous SoCs*
- [x] Model-predictive and learned resource control — representative online-model, imitation-learning, multi-rate and explicit-NMPC review complete; production safe-RL systems such as AWARE remain relevant to the later cloud-autoscaling review
- [x] Hoffmann, Aehlig & Hofmann (2011), *Multivariate Amortized Resource Analysis*
- [x] Das, Hoffmann & Pfenning (2018), *Work Analysis with Resource-Aware Session Types*
- [x] Weiss, Gierczak, Patterson & Ahmed (2021), *Oxide: The Essence of Rust*
- [x] Resource-aware type systems — representative quantitative-bound, linear-protocol and Rust ownership/borrowing reviews complete; further refinement/dependent/effect systems may be added as specific Elastic mechanisms require
- [x] Rzadca et al. (2020), *Autopilot: workload autoscaling at Google*
- [x] Qiu et al. (2023), *AWARE: Automate Workload Autoscaling with Reinforcement Learning in Production Cloud Systems*
- [x] Cloud elasticity and autoscaling — representative production cost/risk autoscaling plus learned-policy lifecycle/fallback review complete; additional predictive and uncertainty-aware systems may be added as mechanisms require
- [x] Adaptive task runtimes and work stealing — foundational decentralized scheduling, parallelism feedback, fine-grained synchronization and weak-memory correctness reviews complete; further locality/task-graph runtimes may be added as mechanisms require
- [x] Energy- and thermal-aware execution — representative application-quality adaptation, hierarchical power-budget redistribution and predictive thermal-control reviews complete
- [ ] LLM scheduling / KV-cache management / disaggregated serving

## Classification

- **ADOPT** — mechanism remains strong and general enough to retain substantially unchanged.
- **ADAPT** — underlying idea is useful, but the mechanism should be generalized or altered.
- **REJECT** — mechanism is incompatible with the emerging model or unnecessary for the intended scope.
- **INVESTIGATE** — evidence is insufficient; compare formally or experimentally before deciding.
