# Local Model Reliability Playbook

## Objective

Make local models reliably outperform larger models on scoped agentic tasks by using a stronger inference stack, not single-shot prompting.

## Core Principle

A local model can punch above its weight when it is:

1. Tool-grounded
2. Forced through iterative reasoning/action loops
3. Verified at test time
4. Fed compact, high-signal context

## Recommended Stack (Priority Order)

### 1) ReAct Loop as Default Runtime

Use a strict `Think -> Act -> Observe` loop:

- For repository or implementation requests, call tools first (`glob/read/grep/task`) before broad clarification.
- Require one concrete action per cycle.
- Stop only on:
  - final answer with evidence
  - hard blocker (missing external input)
  - max cycles reached

Reference: ReAct (2022)  
https://arxiv.org/abs/2210.03629

### 2) TTC as Optional Reliability Booster

Keep TTC opt-in (not default):

- Generate `N` candidates
- Select via pairwise judge (knockout/league)
- Use for high-risk tasks only (security, migration, production changes)

References:

- Self-Consistency: https://arxiv.org/abs/2203.11171
- Provable TTC scaling laws (v5, Oct 28, 2025): https://arxiv.org/abs/2411.19477

### 3) Verifier Pass After Draft/Winner

Run a verifier to detect instruction/factual errors and optionally rewrite:

- Compare original vs rewrite using pairwise judge
- Keep rewrite only if judged better

References:

- Chain-of-Verification: https://arxiv.org/abs/2309.11495
- Self-Refine: https://arxiv.org/abs/2303.17651

### 4) Retrieval and Memory Hierarchy

Do not rely on long context alone. Use:

- Retrieval over relevant files
- Per-cycle compressed observation memory
- Short rolling working memory + compact long-term summary

References:

- Lost in the Middle: https://arxiv.org/abs/2307.03172
- Long-context limits: https://arxiv.org/abs/2310.08560
- LongBench: https://arxiv.org/abs/2308.14508

### 5) Constrained Structured Output

Use schema-constrained outputs for planner state and tool arguments:

- Reduce malformed tool calls
- Reduce drift in local model outputs

Reference: Ollama Structured Outputs  
https://docs.ollama.com/capabilities/structured-outputs

### 6) Serving Optimizations to Buy Test-Time Compute

Reduce latency to spend budget on extra reasoning passes:

- Prefix caching
- Speculative decoding (if backend supports)

References:

- Prefix caching: https://docs.vllm.ai/en/latest/design/prefix_caching/
- Speculative decoding: https://docs.vllm.ai/en/latest/features/speculative_decoding/

## Practical Defaults

Use these defaults for local providers (`ollama`, `lmstudio`, localhost APIs):

- ReAct enabled: `true`
- ReAct max cycles: `3`
- Force tool-first for repo/code intents: `true`
- Observation compression: `true`
- Anti-repeat guard: `true`
- TTC enabled by default: `false`
- TTC escalation for high-risk tasks: `samples=5`, `comparisons=5`
- Verifier enabled for high-risk tasks: `true`

## Failure Modes and Guardrails

### Failure: Generic clarification loops

Guardrail:

- If no tool call and no concrete progress in first cycle, auto-inject one corrective reminder and continue.
- Limit to one automatic recovery injection per turn.
- Prefer this in build mode; avoid duplicate behavior in plan mode.

### Failure: Context drift on long tasks

Guardrail:

- Keep a compact running memory after each tool observation:
  - facts found
  - constraints
  - next action
- Feed summaries, not raw history, into next cycle when possible.

### Failure: Invalid or noisy tool calls

Guardrail:

- Enforce JSON/schema for tool call args where possible.
- Add simple validation and one repair attempt.

## Rollout Plan

1. Phase 1 (now): ReAct-first local loop, anti-repeat guard, tool-first heuristics, TTC disabled by default.
2. Phase 2: Observation compression and memory hierarchy.
3. Phase 3: High-risk adaptive TTC + verifier as escalation path.
4. Phase 4: Telemetry-based tuning (progress score, repeat rate, tool-call rate, blocker rate).

## Success Metrics

Track per session:

- `% turns with useful tool call by cycle 1`
- `% sessions ending in repeated clarification`
- `% sessions requiring manual user correction`
- median time-to-first-concrete-action
- task completion rate on benchmarked repo workflows

## Notes

- This strategy improves reliability on scoped agentic tasks, not universal IQ.
- Local models beat larger ones in practice when orchestration quality is better.
