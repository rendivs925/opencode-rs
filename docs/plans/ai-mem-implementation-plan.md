# ai-mem: Full Implementation Plan

## Overview

Fork and extend [ai-mem](https://github.com/rendivs925/ai-mem) (claude-mem fork) with brain-inspired memory architecture based on the research document `docs/AI Memory Data Structures Inspired by Humans.md`.

---

## Project Setup

### Step 1: Clone Repository
```bash
# Clone the fork to current directory
git clone https://github.com/rendivs925/ai-mem.git
```

### Step 2: Update .gitignore
Add to `/home/rendi/projects/opencode-rs/.gitignore`:
```
ai-mem/
```

---

## Architecture

### Directory Structure

```
ai-mem/
├── src/
│   ├── types/                    # Domain types (AGENTS.md: Layering)
│   │   ├── mod.ts
│   │   ├── memory.ts            # CMU, enums, interfaces
│   │   └── error.ts             # Typed errors
│   │
│   ├── config/                   # Configuration
│   │   ├── mod.ts
│   │   └── settings.ts          # Zod-validated settings
│   │
│   ├── db/                       # Data access layer
│   │   ├── mod.ts
│   │   ├── schema.ts            # Drizzle schema
│   │   ├── cmu-repo.ts         # CMU CRUD operations
│   │   └── skill-repo.ts       # Procedural memory
│   │
│   ├── engine/                   # Business logic layer
│   │   ├── mod.ts
│   │   ├── memory.ts           # Core orchestration (~50 LOC)
│   │   ├── compressor.ts       # LLM compression
│   │   ├── activation.ts       # ACT-R calculations
│   │   ├── decay-worker.ts     # Background decay job
│   │   │
│   │   ├── pruning/            # Memory management
│   │   │   ├── mod.ts
│   │   │   ├── dedupe.ts       # Cosine similarity dedup
│   │   │   ├── merge.ts        # Semantic merging
│   │   │   ├── cull.ts        # Importance culling
│   │   │   └── consolidation.ts # Sleep cycle
│   │   │
│   │   └── procedural/         # Skill extraction
│   │       ├── mod.ts
│   │       └── extractor.ts
│   │
│   ├── retrieval/               # Search layer
│   │   ├── mod.ts
│   │   ├── fts5.ts            # Full-text search
│   │   ├── vector.ts          # Qdrant search
│   │   └── spreading.ts       # Graph traversal (spreading activation)
│   │
│   └── utils/                   # Shared utilities
│       ├── mod.ts
│       └── math.ts             # cosineSimilarity, etc.
│
├── viewer/                      # TypeScript TUI (existing)
├── plugin/                      # OpenCode plugin (existing)
├── docs/                        # Documentation
└── package.json
```

---

## Implementation Phases

### Phase 1: Core Types & Schema

#### 1.1 Types (`src/types/memory.ts`)

```typescript
// AGENTS.md: Use enums, not strings
export enum MemoryTier {
  Sensory = "sensory",
  Working = "working",
  Episodic = "episodic",
  Semantic = "semantic",
  Procedural = "procedural",
}

export enum MemoryType {
  Bugfix = "bugfix",
  Feature = "feature",
  Decision = "decision",
  Discovery = "discovery",
  Refactor = "refactor",
  Change = "change",
}

export enum EmotionalTag {
  Frustration = "frustration",
  Breakthrough = "breakthrough",
  Learning = "learning",
  Confusion = "confusion",
  Satisfaction = "satisfaction",
}

export interface CMU {
  id: string;
  sessionId: string;
  project: string;
  tier: MemoryTier;
  memoryType: MemoryType;
  content: MemoryContent;
  metadata: CMUMetadata;
  tags: EmotionalTag[];
  associations: string[];  // Linked CMU IDs
}

export interface MemoryContent {
  title: string;
  narrative: string;
  facts: string[];
  concepts: string[];
  filesRead: string[];
  filesModified: string[];
  rawOutput?: string;  // For sensory tier
}

export interface CMUMetadata {
  createdAt: number;
  lastAccessed: number;
  accessCount: number;
  importance: number;      // [0.0, 1.0]
  baseActivation: number; // ACT-R calculated
  decayRate: number;     // Tier-specific
}

export interface SearchFilters {
  tiers?: MemoryTier[];
  types?: MemoryType[];
  projects?: string[];
  minImportance?: number;
  since?: number;
}

export interface RetrievalResult {
  cmu: CMU;
  score: number;
  activation: number;
  source: 'fts5' | 'vector' | 'spreading';
}
```

#### 1.2 Error Types (`src/types/error.ts`)

```typescript
// AGENTS.md: Typed errors with single log point
export class MemoryError extends Error {
  constructor(message: string, public readonly cause?: unknown) {
    super(message);
    this.name = "MemoryError";
  }
}

export class CompressionError extends MemoryError { /* ... */ }
export class RetrievalError extends MemoryError { /* ... */ }
export class ActivationError extends MemoryError { /* ... */ }
export class PruningError extends MemoryError { /* ... */ }
```

#### 1.3 Database Schema (`src/db/schema.ts`)

```typescript
// AGENTS.md: Typed schema with Drizzle
import { sqliteTable, text, integer, real } from 'drizzle-orm/sqlite-core';

export const cmus = sqliteTable('cmus', {
  id: text('id').primaryKey(),
  sessionId: text('session_id').notNull(),
  project: text('project').notNull(),
  tier: text('tier', { 
    enum: ['sensory', 'working', 'episodic', 'semantic', 'procedural'] 
  }).notNull(),
  memoryType: text('memory_type', { 
    enum: ['bugfix', 'feature', 'decision', 'discovery', 'refactor', 'change'] 
  }).notNull(),
  
  content: text('content').notNull(),  // Serialized MemoryContent
  createdAt: integer('created_at').notNull(),
  lastAccessed: integer('last_accessed').notNull(),
  accessCount: integer('access_count').default(0),
  importance: real('importance').default(0.5),
  baseActivation: real('base_activation').default(0),
  decayRate: real('decay_rate').default(0.5),
  tags: text('tags').default('[]'),
  associations: text('associations').default('[]'),
  vectorId: text('vector_id'),
});

export const skills = sqliteTable('skills', {
  id: text('id').primaryKey(),
  name: text('name').notNull(),
  pattern: text('pattern').notNull(),
  usageCount: integer('usage_count').default(0),
  lastUsed: integer('last_used'),
  createdAt: integer('created_at').notNull(),
});
```

---

### Phase 2: ACT-R Activation System

#### 2.1 Activation Calculations (`src/engine/activation.ts`)

```typescript
// AGENTS.md: Pure functions, no unwrap/panic
import { CMUMetadata } from "../types/memory";
import { ActivationError } from "../types/error";

const DEFAULT_DECAY_PARAMETER = 0.5;

export function calculateBaseActivation(
  accessCount: number,
  lastAccessed: number,
  decayParameter: number = DEFAULT_DECAY_PARAMETER
): number {
  if (accessCount < 1) return 0;
  
  const now = Math.floor(Date.now() / 1000);
  const elapsed = Math.max(now - lastAccessed, 1);
  
  // B = ln(Σ(t_i^-d)) from ACT-R theory
  const sum = accessCount * Math.pow(elapsed, -decayParameter);
  return Math.log(sum);
}

export function calculateRetentionScore(metadata: CMUMetadata): number {
  const { baseActivation, importance, accessCount } = metadata;
  const frequency = Math.log2(Math.max(accessCount, 1));
  return baseActivation * importance * frequency;
}

export function shouldDecay(
  metadata: CMUMetadata,
  threshold: number,
  decayParameter: number = DEFAULT_DECAY_PARAMETER
): boolean {
  const newActivation = calculateBaseActivation(
    metadata.accessCount,
    metadata.lastAccessed,
    decayParameter
  );
  
  return calculateRetentionScore({ ...metadata, baseActivation: newActivation }) < threshold;
}
```

---

### Phase 3: Associative Graph

#### 3.1 Memory Graph (`src/engine/graph.ts`)

```typescript
// Based on research: spreading activation
import { CMU } from "../types/memory";

export interface MemoryNode {
  id: string;
  cmu: CMU;
  activation: number;
}

export class MemoryGraph {
  private nodes: Map<string, MemoryNode> = new Map();
  
  addNode(cmu: CMU): void {
    this.nodes.set(cmu.id, {
      id: cmu.id,
      cmu,
      activation: cmu.metadata.baseActivation,
    });
  }
  
  addAssociation(fromId: string, toId: string, strength: number): void {
    // Bidirectional links like neural networks
    // ...
  }
  
  // Spreading activation retrieval
  retrieveContext(seedIds: string[], iterations: number = 3): string[] {
    // A_j = B_j + Σ(W_ij * S_i) from research
    // ...
  }
}
```

---

### Phase 4: Pruning Strategies

#### 4.1 Deduplication (`src/engine/pruning/dedupe.ts`)

```typescript
import { CMU } from "../../types/memory";
import { cosineSimilarity } from "../../utils/math";

export interface DedupeCandidate {
  keep: CMU;
  remove: CMU;
  similarity: number;
}

export function findDuplicates(cmus: CMU[], threshold: number = 0.95): DedupeCandidate[] {
  const duplicates: DedupeCandidate[] = [];
  // O(n²) comparison - optimize for large datasets
  // ...
  return duplicates;
}
```

#### 4.2 Semantic Merging (`src/engine/pruning/merge.ts`)

```typescript
// LLM-as-a-Judge synthesis from research
export async function mergeSimilar(cmus: CMU[]): Promise<CMU[]> {
  // Use LLM to synthesize similar memories into one
  // ...
}
```

#### 4.3 Consolidation (`src/engine/pruning/consolidation.ts`)

```typescript
// Sleep cycle from research
export async function runConsolidation(engine: MemoryEngine): Promise<void> {
  // 1. Run spreading activation to find associations
  // 2. Merge similar memories
  // 3. Update importance based on access patterns
  // 4. Prune low-activation memories
}
```

---

### Phase 5: Configuration

#### 5.1 Settings (`src/config/settings.ts`)

```typescript
import { z } from "zod";

const TierConfigSchema = z.object({
  decay: z.number().min(0).max(1),
  maxCount: z.number().positive(),
});

const ActRConfigSchema = z.object({
  decayParameter: z.number().min(0).max(1).default(0.5),
  activationThreshold: z.number().min(0).default(0.1),
});

const PruningConfigSchema = z.object({
  dedupeSimilarity: z.number().min(0).max(1).default(0.95),
  importanceThreshold: z.number().min(0).max(1).default(0.2),
  consolidateOnIdleMinutes: z.number().positive().default(15),
});

const VectorDbConfigSchema = z.object({
  provider: z.enum(["none", "qdrant", "chroma"]).default("none"),
  url: z.string().url().optional(),
});

export const SettingsSchema = z.object({
  observations: z.number().min(1).max(200).default(50),
  sessions: z.number().min(1).max(50).default(10),
  tiers: z.record(z.nativeEnum(MemoryTier), TierConfigSchema),
  actr: ActRConfigSchema,
  pruning: PruningConfigSchema,
  vectorDb: VectorDbConfigSchema,
});

export type Settings = z.infer<typeof SettingsSchema>;
```

---

## Feature Summary (Research-Based)

| Feature | Research Basis | Priority |
|---------|---------------|----------|
| **Two-tier memory** | Multi-layer hierarchy (page 4) | ✓ (exists) |
| **ACT-R activation** | Base-level equation (page 5) | HIGH |
| **Spreading activation** | Associative retrieval (page 6) | HIGH |
| **Ebbinghaus forgetting** | Temporal decay (page 5) | HIGH |
| **Semantic merging** | Pruning strategies (page 9) | MEDIUM |
| **Procedural memory** | Skills extraction (page 3) | MEDIUM |
| **Sleep consolidation** | Sleep/pruning cycle (page 8) | MEDIUM |

---

## AGENTS.md Compliance Checklist

| Rule | Implementation |
|------|----------------|
| No `unwrap()`/`panic!()` | All errors use typed `Result<T, E>` |
| No `.clone()` | Use references, `Arc`, ownership |
| No hardcoded secrets | Environment variables via config |
| No debug prints | Proper logging with tracing |
| Files ≤300 LOC | Split into submodules |
| Functions ≤60 LOC | Extract to helper functions |
| Typed enums | `MemoryTier`, `MemoryType`, `EmotionalTag` |
| Typed interfaces | `CMU`, `CMUMetadata`, `RetrievalResult` |
| Zod validation | All config inputs validated |
| Single error log | Each operation logs once |
| Layering | `db/` → data, `engine/` → business, `types/` → domain |

---

## File Creation Order

1. `src/types/memory.ts` - Core types + enums
2. `src/types/error.ts` - Error types
3. `src/config/settings.ts` - Zod config
4. `src/db/schema.ts` - Drizzle schema
5. `src/db/cmu-repo.ts` - CRUD operations
6. `src/utils/math.ts` - Similarity functions
7. `src/engine/activation.ts` - ACT-R calculations
8. `src/engine/graph.ts` - Memory graph
9. `src/engine/pruning/dedupe.ts` - Deduplication
10. `src/engine/pruning/merge.ts` - Semantic merging
11. `src/engine/pruning/cull.ts` - Importance culling
12. `src/engine/pruning/consolidation.ts` - Sleep cycle
13. `src/engine/decay-worker.ts` - Background decay
14. `src/engine/compressor.ts` - LLM compression (enhanced)
15. `src/engine/memory.ts` - Core orchestration
16. `src/retrieval/fts5.ts` - Full-text search
17. `src/retrieval/vector.ts` - Qdrant search
18. `src/retrieval/spreading.ts` - Graph retrieval
