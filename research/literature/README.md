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

- [x] Chandy & Lamport (1985), *Distributed Snapshots: Determining Global States of Distributed Systems*
- [x] Moreau & Queinnec (2005), *Resource Aware Programming* — initial mechanism review
- [x] Blumofe et al. (1996), *Cilk: An Efficient Multithreaded Runtime System*
- [x] Agrawal, He & Leiserson (2007), *Adaptive Work Stealing with Parallelism Feedback*
- [x] He & Geng (2008), *Active Learning of Causal Networks with Intervention Experiments and Optimal Designs*
- [x] Zanini et al. (2009), *Multicore Thermal Management with Model Predictive Control*
- [x] Hoffmann et al. (2011), *Dynamic Knobs for Responsive Power-Aware Computing*
- [x] Teich, Schröder-Preikschat & Herkersdorf (2013), *Invasive Computing — Common Terms and Granularity of Invasion*
- [x] McSherry et al. (2013), *Differential Dataflow*
- [x] Murray et al. (2013), *Naiad: A Timely Dataflow System*
- [x] Carbone et al. (2015), *Lightweight Asynchronous Snapshots for Distributed Dataflows*
- [x] Eastep et al. (2017), *Global Extensible Open Power Manager*
- [x] Lindgren et al. (2018), *Experimental Design for Cost-Aware Learning of Causal Graphs*
- [x] Mai et al. (2018), *Chi: A Scalable and Programmable Control Plane for Distributed Stream Processing Systems*
- [x] Hoffmann et al. (2019), *Megaphone: Latency-conscious State Migration for Distributed Streaming Dataflows*
- [x] Agrawal et al. (2019), *ABCD-Strategy: Budgeted Experimental Design for Targeted Causal Structure Discovery*
- [x] Mao et al. (2021), *Trisk: Task-Centric Data Stream Reconfiguration*
- [x] Qiao et al. (2021), *Pollux: Co-adaptive Cluster Scheduling for Goodput-Optimized Deep Learning*
- [x] Gu et al. (2022), *Meces: Latency-efficient Rescaling via Prioritized State Migration*
- [x] Wang et al. (2022), *Fries: Fast and Consistent Runtime Reconfiguration in Dataflow Systems with Transactional Guarantees*
- [x] Zheng et al. (2022), *Alpa: Automating Inter- and Intra-Operator Parallelism for Distributed Deep Learning*
- [x] Mao et al. (2023), *StreamOps: Cloud-Native Runtime Management for Streaming Services in ByteDance*
- [x] Sheng et al. (2023), *FlexGen: High-Throughput Generative Inference of Large Language Models with a Single GPU*
- [x] Zhang et al. (2023), *H2O: Heavy-Hitter Oracle for Efficient Generative Inference of Large Language Models*
- [x] Kwon et al. (2023), *Efficient Memory Management for Large Language Model Serving with PagedAttention*
- [x] Wang et al. (2023), *BWoS: Formally Verified Block-based Work Stealing for Parallel Processing*
- [x] Tang et al. (2024), *QUEST: Query-Aware Sparsity for Efficient Long-Context LLM Inference*
- [x] Lee et al. (2024), *InfiniGen: Efficient Generative Inference of Large Language Models with Dynamic KV Cache Management*
- [x] Sun et al. (2024), *Llumnix: Dynamic Scheduling for Large Language Model Serving*
- [x] Zhong et al. (2024), *DistServe: Disaggregating Prefill and Decoding for Goodput-optimized Large Language Model Serving*
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
- [x] Qin et al. (2025), *Mooncake: Trading More Storage for Less Computation — A KVCache-centric Architecture for Serving LLM Chatbot*
- [x] Chen et al. (2025), *IMPRESS: An Importance-Informed Multi-Tier Prefix KV Storage System for Large Language Model Inference*
- [x] LLM scheduling / KV-cache management / disaggregated serving — representative logical paging, joint placement/compression, additive/non-additive selection, query-conditioned utility, selective prefetch, live migration, prefill/decode disaggregation and distributed multi-tier KV-cache reviews complete; further prefix-cache and semantic-compression work may be added as mechanisms require
- [x] Acar, Blelloch & Harper (2002), *Adaptive Functional Programming*
- [x] Green, Karvounarakis & Tannen (2007), *Provenance Semirings*
- [x] Ahmad et al. (2012), *DBToaster: Higher-order Delta Processing for Dynamic, Frequently Fresh Views*
- [x] Mokhov, Mitchell & Peyton Jones (2018), *Build Systems à la Carte*
- [x] Derived-resource provenance / incremental repair — representative lineage, change-propagation, delta-maintenance and validity-trace reviews complete; additional incremental-computation systems may be added as mechanisms require
- [x] Incremental/differential dataflow + consistent checkpointing — representative partial-order versioning, progress-frontier, delta-trace, global-snapshot, topology-aware barrier-snapshot, fine-grained live migration, priority-aware migration and transactional reconfiguration mechanisms reviewed; additional distributed incremental-state systems may be added as required
- [x] Reconfiguration primitives / production control planes — task-centric primitive composition, policy/mechanism separation, trigger modes, diagnosis-before-act and production policy arbitration reviewed through Trisk and StreamOps
- [x] Active diagnosis / causal experiment design — representative sequential minimax/entropy intervention selection, minimum-cost structural design, and targeted finite-budget Bayesian experimental design reviewed; production-safe decision-focused probing remains an Elastic hypothesis requiring experiments

## Classification

- **ADOPT** — mechanism remains strong and general enough to retain substantially unchanged.
- **ADAPT** — underlying idea is useful, but the mechanism should be generalized or altered.
- **REJECT** — mechanism is incompatible with the emerging model or unnecessary for the intended scope.
- **INVESTIGATE** — evidence is insufficient; compare formally or experimentally before deciding.
