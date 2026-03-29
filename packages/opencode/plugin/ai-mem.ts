// ai-mem OpenCode Plugin
// Provides persistent memory with brain-inspired features for OpenCode

import type { Hooks } from "@opencode-ai/plugin"
import { tool } from "@opencode-ai/plugin"

let memoryStore: Map<string, MemoryEntry> = new Map()

interface MemoryEntry {
  id: string
  sessionId: string
  project: string
  content: {
    title: string
    narrative: string
    facts: string[]
    concepts: string[]
    filesRead: string[]
    filesModified: string[]
  }
  tier: string
  importance: number
  baseActivation: number
  decayRate: number
  createdAt: number
  lastAccessed: number
  accessCount: number
  tags: string[]
  associations: string[]
}

function calculateActivation(accessCount: number, lastAccessed: number, decayRate: number = 0.5): number {
  if (accessCount < 1) return 0
  const elapsed = Math.max(Date.now() / 1000 - lastAccessed, 1)
  const sum = accessCount * Math.pow(elapsed, -decayRate)
  return Math.log(sum)
}

export const name = "ai-mem"

export async function plugin(): Promise<Hooks> {
  return {
    tool: {
      "mem-search": tool({
        description: "Search memories using brain-inspired retrieval with ACT-R activation",
        args: {
          query: tool.schema.string().describe("Search query"),
          limit: tool.schema.number().optional().describe("Max results (default: 10)"),
        },
        async execute(args): Promise<string> {
          const results = searchMemories(args.query, args.limit || 10)
          if (results.length === 0) {
            return "No memories found"
          }
          return results
            .map(
              (r) =>
                `[${r.tier}] ${r.content.title}\n${r.content.narrative.substring(0, 200)}\nImportance: ${r.importance}, Activation: ${r.baseActivation.toFixed(2)}`,
            )
            .join("\n\n---\n\n")
        },
      }),

      "mem-recall": tool({
        description: "Recall specific memories by ID and record access for activation boost",
        args: {
          memoryId: tool.schema.string().describe("Memory ID to recall"),
        },
        async execute(args): Promise<string> {
          const memory = memoryStore.get(args.memoryId)
          if (!memory) {
            return "Memory not found"
          }

          memory.accessCount++
          memory.lastAccessed = Math.floor(Date.now() / 1000)
          memory.baseActivation = calculateActivation(memory.accessCount, memory.lastAccessed, memory.decayRate)

          return `Memory accessed:\nTitle: ${memory.content.title}\nContent: ${memory.content.narrative.substring(0, 300)}\nNew activation: ${memory.baseActivation.toFixed(2)}`
        },
      }),

      "mem-stats": tool({
        description: "Get memory statistics by tier and activation",
        args: {},
        async execute(): Promise<string> {
          const stats = getStats()
          return `Memory Statistics:
Total memories: ${stats.total}
Average activation: ${stats.avgActivation.toFixed(2)}

By tier:
${Object.entries(stats.byTier)
  .map(([tier, count]) => `  ${tier}: ${count}`)
  .join("\n")}`
        },
      }),

      "mem-consolidate": tool({
        description: "Trigger memory consolidation (deduplication, linking, pruning)",
        args: {},
        async execute(): Promise<string> {
          const result = consolidateMemories()
          return `Consolidation complete:
Merged: ${result.merged}
Pruned: ${result.pruned}
Linked: ${result.linked}`
        },
      }),

      "mem-save": tool({
        description: "Save important information to memory",
        args: {
          title: tool.schema.string().describe("Title for the memory"),
          content: tool.schema.string().describe("Content to remember"),
          importance: tool.schema.number().optional().describe("Importance 0-1 (default: 0.5)"),
        },
        async execute(args): Promise<string> {
          const id = crypto.randomUUID()
          const now = Math.floor(Date.now() / 1000)

          const entry: MemoryEntry = {
            id,
            sessionId: "manual",
            project: "default",
            content: {
              title: args.title,
              narrative: args.content,
              facts: [],
              concepts: args.content.split(" ").slice(0, 10),
              filesRead: [],
              filesModified: [],
            },
            tier: "semantic",
            importance: args.importance || 0.5,
            baseActivation: 1.0,
            decayRate: 0.3,
            createdAt: now,
            lastAccessed: now,
            accessCount: 1,
            tags: [],
            associations: [],
          }

          memoryStore.set(id, entry)
          return `Memory saved with ID: ${id}`
        },
      }),
    },

    "tool.execute.after": async (input, output) => {
      if (output.output && output.output.length > 100) {
        const id = crypto.randomUUID()
        const now = Math.floor(Date.now() / 1000)

        const entry: MemoryEntry = {
          id,
          sessionId: input.sessionID,
          project: "default",
          content: {
            title: `Tool: ${input.tool}`,
            narrative: output.output.substring(0, 2000),
            facts: [],
            concepts: [input.tool],
            filesRead: [],
            filesModified: [],
          },
          tier: "episodic",
          importance: 0.5,
          baseActivation: 1.0,
          decayRate: 0.5,
          createdAt: now,
          lastAccessed: now,
          accessCount: 1,
          tags: [],
          associations: [],
        }

        memoryStore.set(id, entry)
      }
    },
  }
}

function searchMemories(query: string, limit: number): MemoryEntry[] {
  const keywords = query.toLowerCase().split(/\s+/).filter(Boolean)
  const results: Array<{ memory: MemoryEntry; score: number }> = []

  for (const [, memory] of memoryStore) {
    const text =
      `${memory.content.title} ${memory.content.narrative} ${memory.content.concepts.join(" ")}`.toLowerCase()
    const matchCount = keywords.filter((k) => text.includes(k)).length

    if (matchCount > 0) {
      results.push({
        memory,
        score: memory.baseActivation * memory.importance * (matchCount / keywords.length),
      })
    }
  }

  return results
    .sort((a, b) => b.score - a.score)
    .slice(0, limit)
    .map((r) => r.memory)
}

function getStats() {
  const byTier: Record<string, number> = {
    sensory: 0,
    working: 0,
    episodic: 0,
    semantic: 0,
    procedural: 0,
  }

  let totalActivation = 0
  let count = 0

  for (const memory of memoryStore.values()) {
    byTier[memory.tier] = (byTier[memory.tier] || 0) + 1
    totalActivation += memory.baseActivation
    count++
  }

  return {
    total: count,
    byTier,
    avgActivation: count > 0 ? totalActivation / count : 0,
  }
}

function consolidateMemories() {
  let merged = 0
  let pruned = 0
  let linked = 0
  const seen = new Set<string>()

  const memories = Array.from(memoryStore.values())

  for (const memory of memories) {
    if (seen.has(memory.id)) continue

    const similar = memories.filter((m) => {
      if (m.id === memory.id || seen.has(m.id)) return false
      const combined1 = `${memory.content.title} ${memory.content.narrative}`.toLowerCase()
      const combined2 = `${m.content.title} ${m.content.narrative}`.toLowerCase()
      return combined1.substring(0, 100) === combined2.substring(0, 100)
    })

    for (const dup of similar) {
      memoryStore.delete(dup.id)
      seen.add(dup.id)
      merged++
    }
  }

  for (const memory of memories) {
    for (const other of memories) {
      if (memory.id === other.id) continue
      const sharedConcepts = memory.content.concepts.filter((c) => other.content.concepts.includes(c))
      if (sharedConcepts.length > 0 && !memory.associations.includes(other.id)) {
        memory.associations.push(other.id)
        linked++
      }
    }
  }

  const threshold = 0.2
  for (const [id, memory] of memoryStore) {
    const retentionScore = memory.baseActivation * memory.importance * Math.log2(Math.max(memory.accessCount, 1))
    if (retentionScore < threshold) {
      memoryStore.delete(id)
      pruned++
    }
  }

  return { merged, pruned, linked }
}

export default plugin
