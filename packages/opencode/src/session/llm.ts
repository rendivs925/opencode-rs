import { Installation } from "@/installation"
import { Provider } from "@/provider/provider"
import { Log } from "@/util/log"
import {
  streamText,
  wrapLanguageModel,
  type ModelMessage,
  type StreamTextResult,
  type Tool,
  type ToolSet,
  tool,
  jsonSchema,
} from "ai"
import { mergeDeep, pipe } from "remeda"
import { ProviderTransform } from "@/provider/transform"
import { Config } from "@/config/config"
import { Instance } from "@/project/instance"
import type { Agent } from "@/agent/agent"
import type { MessageV2 } from "./message-v2"
import { Plugin } from "@/plugin"
import { SystemPrompt } from "./system"
import { Flag } from "@/flag/flag"
import { PermissionNext } from "@/permission/next"
import { Auth } from "@/auth"

export namespace LLM {
  const log = Log.create({ service: "llm" })
  export const OUTPUT_TOKEN_MAX = ProviderTransform.OUTPUT_TOKEN_MAX
  const TTC_MAX_DRAFT_CHARS = 8000
  const TTC_MAX_TASK_CHARS = 4000
  const TTC_JUDGE_SYSTEM = [
    "You are a strict evaluator.",
    "Choose which candidate better satisfies the task.",
    'Return ONLY JSON: {"winner":"A"} or {"winner":"B"}.',
    "No markdown, no extra text.",
  ].join(" ")

  export type StreamInput = {
    user: MessageV2.User
    sessionID: string
    model: Provider.Model
    agent: Agent.Info
    system: string[]
    abort: AbortSignal
    messages: ModelMessage[]
    small?: boolean
    tools: Record<string, Tool>
    retries?: number
    toolChoice?: "auto" | "required" | "none"
  }

  export type StreamOutput = StreamTextResult<ToolSet, unknown>

  type TTCConfig = {
    enabled: boolean
    local_only: boolean
    strategy: "knockout"
    samples: number
    comparisons: number
  }

  export async function stream(input: StreamInput) {
    const l = log
      .clone()
      .tag("providerID", input.model.providerID)
      .tag("modelID", input.model.id)
      .tag("sessionID", input.sessionID)
      .tag("small", (input.small ?? false).toString())
      .tag("agent", input.agent.name)
      .tag("mode", input.agent.mode)
    l.info("stream", {
      modelID: input.model.id,
      providerID: input.model.providerID,
    })
    const [language, cfg, provider, auth] = await Promise.all([
      Provider.getLanguage(input.model),
      Config.get(),
      Provider.getProvider(input.model.providerID),
      Auth.get(input.model.providerID),
    ])
    const isCodex = provider.id === "openai" && auth?.type === "oauth"

    const system = []
    system.push(
      [
        // use agent prompt otherwise provider prompt
        // For Codex sessions, skip SystemPrompt.provider() since it's sent via options.instructions
        ...(input.agent.prompt ? [input.agent.prompt] : isCodex ? [] : SystemPrompt.provider(input.model)),
        // any custom prompt passed into this call
        ...input.system,
        // any custom prompt from last user message
        ...(input.user.system ? [input.user.system] : []),
      ]
        .filter((x) => x)
        .join("\n"),
    )

    const header = system[0]
    await Plugin.trigger(
      "experimental.chat.system.transform",
      { sessionID: input.sessionID, model: input.model },
      { system },
    )
    // rejoin to maintain 2-part structure for caching if header unchanged
    if (system.length > 2 && system[0] === header) {
      const rest = system.slice(1)
      system.length = 0
      system.push(header, rest.join("\n"))
    }

    const variant =
      !input.small && input.model.variants && input.user.variant ? input.model.variants[input.user.variant] : {}
    const base = input.small
      ? ProviderTransform.smallOptions(input.model)
      : ProviderTransform.options({
          model: input.model,
          sessionID: input.sessionID,
          providerOptions: provider.options,
        })
    const options: Record<string, any> = pipe(
      base,
      mergeDeep(input.model.options),
      mergeDeep(input.agent.options),
      mergeDeep(variant),
    )
    if (isCodex) {
      options.instructions = SystemPrompt.instructions()
    }

    const params = await Plugin.trigger(
      "chat.params",
      {
        sessionID: input.sessionID,
        agent: input.agent,
        model: input.model,
        provider,
        message: input.user,
      },
      {
        temperature: input.model.capabilities.temperature
          ? (input.agent.temperature ?? ProviderTransform.temperature(input.model))
          : undefined,
        topP: input.agent.topP ?? ProviderTransform.topP(input.model),
        topK: ProviderTransform.topK(input.model),
        options,
      },
    )

    const { headers } = await Plugin.trigger(
      "chat.headers",
      {
        sessionID: input.sessionID,
        agent: input.agent,
        model: input.model,
        provider,
        message: input.user,
      },
      {
        headers: {},
      },
    )

    const maxOutputTokens =
      isCodex || provider.id.includes("github-copilot") ? undefined : ProviderTransform.maxOutputTokens(input.model)

    const tools = await resolveTools(input)
    const wrapped = wrapLanguageModel({
      model: language,
      middleware: [
        {
          async transformParams(args) {
            if (args.type === "stream") {
              // @ts-expect-error
              args.params.prompt = ProviderTransform.message(args.params.prompt, input.model, options)
            }
            return args.params
          },
        },
      ],
    })
    const baseMessages: ModelMessage[] = [
      ...system.map(
        (x): ModelMessage => ({
          role: "system",
          content: x,
        }),
      ),
      ...input.messages,
    ]

    const ttc = resolveTTC(cfg)
    const useTTC = shouldUseTTC({ input, model: input.model, provider, ttc, params, isCodex })
    if (useTTC) {
      const winner = await runTTC({
        input,
        ttc,
        params,
        wrapped,
        maxOutputTokens,
        options: params.options,
        baseMessages,
      })
      if (winner) {
        system.push(
          [
            "<system-reminder>",
            "A test-time-compute selection was run for reliability.",
            "Use the selected draft as your primary trajectory and avoid drifting away from it.",
            "<ttc-selected-draft>",
            winner.slice(0, TTC_MAX_DRAFT_CHARS),
            "</ttc-selected-draft>",
            "</system-reminder>",
          ].join("\n"),
        )
      }
    }

    // LiteLLM and some Anthropic proxies require the tools parameter to be present
    // when message history contains tool calls, even if no tools are being used.
    // Add a dummy tool that is never called to satisfy this validation.
    // This is enabled for:
    // 1. Providers with "litellm" in their ID or API ID (auto-detected)
    // 2. Providers with explicit "litellmProxy: true" option (opt-in for custom gateways)
    const isLiteLLMProxy =
      provider.options?.["litellmProxy"] === true ||
      input.model.providerID.toLowerCase().includes("litellm") ||
      input.model.api.id.toLowerCase().includes("litellm")

    if (isLiteLLMProxy && Object.keys(tools).length === 0 && hasToolCalls(input.messages)) {
      tools["_noop"] = tool({
        description:
          "Placeholder for LiteLLM/Anthropic proxy compatibility - required when message history contains tool calls but no active tools are needed",
        inputSchema: jsonSchema({ type: "object", properties: {} }),
        execute: async () => ({ output: "", title: "", metadata: {} }),
      })
    }

    return streamText({
      onError(error) {
        l.error("stream error", {
          error,
        })
      },
      async experimental_repairToolCall(failed) {
        const lower = failed.toolCall.toolName.toLowerCase()
        if (lower !== failed.toolCall.toolName && tools[lower]) {
          l.info("repairing tool call", {
            tool: failed.toolCall.toolName,
            repaired: lower,
          })
          return {
            ...failed.toolCall,
            toolName: lower,
          }
        }
        return {
          ...failed.toolCall,
          input: JSON.stringify({
            tool: failed.toolCall.toolName,
            error: failed.error.message,
          }),
          toolName: "invalid",
        }
      },
      temperature: params.temperature,
      topP: params.topP,
      topK: params.topK,
      providerOptions: ProviderTransform.providerOptions(input.model, params.options),
      activeTools: Object.keys(tools).filter((x) => x !== "invalid"),
      tools,
      toolChoice: input.toolChoice,
      maxOutputTokens,
      abortSignal: input.abort,
      headers: {
        ...(input.model.providerID.startsWith("opencode")
          ? {
              "x-opencode-project": Instance.project.id,
              "x-opencode-session": input.sessionID,
              "x-opencode-request": input.user.id,
              "x-opencode-client": Flag.OPENCODE_CLIENT,
            }
          : input.model.providerID !== "anthropic"
            ? {
                "User-Agent": `opencode/${Installation.VERSION}`,
              }
            : undefined),
        ...input.model.headers,
        ...headers,
      },
      maxRetries: input.retries ?? 0,
      messages: [
        ...system.map(
          (x): ModelMessage => ({
            role: "system",
            content: x,
          }),
        ),
        ...input.messages,
      ],
      model: wrapped,
      experimental_telemetry: {
        isEnabled: cfg.experimental?.openTelemetry,
        metadata: {
          userId: cfg.username ?? "unknown",
          sessionId: input.sessionID,
        },
      },
    })
  }

  async function resolveTools(input: Pick<StreamInput, "tools" | "agent" | "user">) {
    const disabled = PermissionNext.disabled(Object.keys(input.tools), input.agent.permission)
    for (const tool of Object.keys(input.tools)) {
      if (input.user.tools?.[tool] === false || disabled.has(tool)) {
        delete input.tools[tool]
      }
    }
    return input.tools
  }

  function resolveTTC(cfg: Awaited<ReturnType<typeof Config.get>>): TTCConfig {
    const raw = cfg.experimental?.ttc
    return {
      enabled: raw?.enabled ?? true,
      local_only: raw?.local_only ?? true,
      strategy: raw?.strategy ?? "knockout",
      samples: Math.max(2, Math.min(9, raw?.samples ?? 3)),
      comparisons: Math.max(1, Math.min(7, raw?.comparisons ?? 3)),
    }
  }

  function isLocalProvider(input: { model: Provider.Model; provider: Provider.Info }) {
    if (input.model.providerID.toLowerCase().includes("ollama")) return true
    if (input.provider.id.toLowerCase().includes("ollama")) return true
    if (isLocalURL(input.provider.options?.["baseURL"])) return true
    if (isLocalURL(input.model.api.url)) return true
    return false
  }

  function isLocalURL(value: unknown) {
    if (typeof value !== "string") return false
    return /(localhost|127\.0\.0\.1|0\.0\.0\.0|\[::1\])/.test(value)
  }

  function shouldUseTTC(input: {
    input: StreamInput
    model: Provider.Model
    provider: Provider.Info
    ttc: TTCConfig
    params: { temperature?: number }
    isCodex: boolean
  }) {
    if (!input.ttc.enabled) return false
    if (input.isCodex) return false
    if (input.input.small) return false
    if (input.input.toolChoice === "required") return false
    if (!input.model.capabilities.temperature) return false
    if (input.ttc.local_only && !isLocalProvider({ model: input.model, provider: input.provider })) return false
    if (input.params.temperature === 0) return false
    return true
  }

  async function runTTC(input: {
    input: StreamInput
    ttc: TTCConfig
    params: { temperature?: number; topP?: number; topK?: number; options?: Record<string, any> }
    wrapped: ReturnType<typeof wrapLanguageModel>
    maxOutputTokens: number | undefined
    options: Record<string, any>
    baseMessages: ModelMessage[]
  }) {
    if (input.ttc.strategy !== "knockout") return

    const l = log
      .clone()
      .tag("providerID", input.input.model.providerID)
      .tag("modelID", input.input.model.id)
      .tag("sessionID", input.input.sessionID)
      .tag("ttc", "knockout")
    const samples = await Promise.all(
      Array.from({ length: input.ttc.samples }, (_, i) =>
        sampleCandidate({
          index: i,
          input: input.input,
          params: input.params,
          wrapped: input.wrapped,
          maxOutputTokens: input.maxOutputTokens,
          options: input.options,
          messages: input.baseMessages,
        }),
      ),
    )
    const pool = samples.map((value, index) => ({ value, index })).filter((x) => x.value.trim().length > 0)
    if (pool.length < 2) {
      l.info("ttc skipped: insufficient candidates", {
        generated: samples.length,
        nonEmpty: pool.length,
      })
      return pool[0]?.value
    }

    let nodes = pool
    let rounds = 0
    let comparisons = 0
    while (nodes.length > 1) {
      rounds++
      const next: typeof nodes = []
      for (let i = 0; i < nodes.length; i += 2) {
        const a = nodes[i]
        const b = nodes[i + 1]
        if (!b) {
          next.push(a)
          continue
        }
        const winner = await compareCandidates({
          a: a.value,
          b: b.value,
          k: input.ttc.comparisons,
          input: input.input,
          params: input.params,
          wrapped: input.wrapped,
          maxOutputTokens: input.maxOutputTokens,
          options: input.options,
          messages: input.baseMessages,
        })
        comparisons += input.ttc.comparisons
        next.push(winner === "A" ? a : b)
      }
      nodes = next
    }
    l.info("ttc selected winner", {
      candidates: pool.length,
      rounds,
      comparisons,
      winner: nodes[0].index,
    })
    return nodes[0].value
  }

  async function sampleCandidate(input: {
    index: number
    input: StreamInput
    params: { temperature?: number; topP?: number; topK?: number; options?: Record<string, any> }
    wrapped: ReturnType<typeof wrapLanguageModel>
    maxOutputTokens: number | undefined
    options: Record<string, any>
    messages: ModelMessage[]
  }) {
    const base = input.params.temperature ?? 0.7
    const jitter = (input.index - 1) * 0.15
    const temperature = Math.max(0.2, Math.min(1.2, base + jitter))
    const result = streamText({
      onError() {},
      temperature,
      topP: input.params.topP,
      topK: input.params.topK,
      providerOptions: ProviderTransform.providerOptions(input.input.model, input.options),
      tools: {},
      activeTools: [],
      toolChoice: "none",
      maxOutputTokens: input.maxOutputTokens,
      abortSignal: input.input.abort,
      headers: {
        ...(input.input.model.providerID.startsWith("opencode")
          ? {
              "x-opencode-project": Instance.project.id,
              "x-opencode-session": input.input.sessionID,
              "x-opencode-request": input.input.user.id,
              "x-opencode-client": Flag.OPENCODE_CLIENT,
            }
          : input.input.model.providerID !== "anthropic"
            ? {
                "User-Agent": `opencode/${Installation.VERSION}`,
              }
            : undefined),
        ...input.input.model.headers,
      },
      maxRetries: 0,
      messages: input.messages,
      model: input.wrapped,
    })
    let text = ""
    for await (const chunk of result.textStream) {
      text += chunk
    }
    return text.trim()
  }

  async function compareCandidates(input: {
    a: string
    b: string
    k: number
    input: StreamInput
    params: { topP?: number; topK?: number; options?: Record<string, any> }
    wrapped: ReturnType<typeof wrapLanguageModel>
    maxOutputTokens: number | undefined
    options: Record<string, any>
    messages: ModelMessage[]
  }) {
    const task = extractTask(input.messages)
    const votes = await Promise.all(
      Array.from({ length: input.k }, () =>
        judgePair({
          a: input.a,
          b: input.b,
          task,
          input: input.input,
          params: input.params,
          wrapped: input.wrapped,
          maxOutputTokens: input.maxOutputTokens,
          options: input.options,
        }),
      ),
    )
    const countA = votes.filter((x) => x === "A").length
    const countB = votes.length - countA
    if (countA >= countB) return "A" as const
    return "B" as const
  }

  function extractTask(messages: ModelMessage[]) {
    for (let i = messages.length - 1; i >= 0; i--) {
      const msg = messages[i]
      if (msg.role !== "user") continue
      const content =
        typeof msg.content === "string"
          ? msg.content
          : msg.content
              .map((part) => {
                if (part.type === "text") return part.text
                return ""
              })
              .join("\n")
      const text = content.trim()
      if (!text) continue
      return text.slice(0, TTC_MAX_TASK_CHARS)
    }
    return "No explicit task text found. Prefer the candidate that best follows prior context and constraints."
  }

  async function judgePair(input: {
    a: string
    b: string
    task: string
    input: StreamInput
    params: { topP?: number; topK?: number; options?: Record<string, any> }
    wrapped: ReturnType<typeof wrapLanguageModel>
    maxOutputTokens: number | undefined
    options: Record<string, any>
  }) {
    const result = streamText({
      onError() {},
      temperature: 0,
      topP: input.params.topP,
      topK: input.params.topK,
      providerOptions: ProviderTransform.providerOptions(input.input.model, input.options),
      tools: {},
      activeTools: [],
      toolChoice: "none",
      maxOutputTokens: Math.min(input.maxOutputTokens ?? 256, 256),
      abortSignal: input.input.abort,
      headers: {
        ...(input.input.model.providerID.startsWith("opencode")
          ? {
              "x-opencode-project": Instance.project.id,
              "x-opencode-session": input.input.sessionID,
              "x-opencode-request": input.input.user.id,
              "x-opencode-client": Flag.OPENCODE_CLIENT,
            }
          : input.input.model.providerID !== "anthropic"
            ? {
                "User-Agent": `opencode/${Installation.VERSION}`,
              }
            : undefined),
        ...input.input.model.headers,
      },
      maxRetries: 0,
      messages: [
        {
          role: "system",
          content: TTC_JUDGE_SYSTEM,
        },
        {
          role: "user",
          content: [
            "<task>",
            input.task,
            "</task>",
            "<candidate_a>",
            input.a.slice(0, TTC_MAX_DRAFT_CHARS),
            "</candidate_a>",
            "<candidate_b>",
            input.b.slice(0, TTC_MAX_DRAFT_CHARS),
            "</candidate_b>",
            'Return JSON only: {"winner":"A"} or {"winner":"B"}',
          ].join("\n"),
        },
      ],
      model: input.wrapped,
    })
    let text = ""
    for await (const chunk of result.textStream) {
      text += chunk
    }
    return parseWinner(text)
  }

  function parseWinner(text: string) {
    const json = text.match(/"winner"\s*:\s*"([AB])"/i)?.[1]?.toUpperCase()
    if (json === "B") return "B" as const
    if (json === "A") return "A" as const
    const plain = text.match(/\b([AB])\b/i)?.[1]?.toUpperCase()
    if (plain === "B") return "B" as const
    return "A" as const
  }

  // Check if messages contain any tool-call content
  // Used to determine if a dummy tool should be added for LiteLLM proxy compatibility
  export function hasToolCalls(messages: ModelMessage[]): boolean {
    for (const msg of messages) {
      if (!Array.isArray(msg.content)) continue
      for (const part of msg.content) {
        if (part.type === "tool-call" || part.type === "tool-result") return true
      }
    }
    return false
  }
}
