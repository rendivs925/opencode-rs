type Core = {
  countTokens?: (path: string, encoding: string) => number
  countTokensFromText?: (text: string, encoding: string) => number
}

const core = (await import("@opencode-ai/core").catch(() => undefined)) as Core | undefined

export namespace Token {
  const CHARS_PER_TOKEN = 4

  export function estimate(input: string) {
    return Math.max(0, Math.round((input || "").length / CHARS_PER_TOKEN))
  }

  export function count(input: string, encoding: string = "cl100k_base") {
    if (!core?.countTokensFromText) return estimate(input)
    return core.countTokensFromText(input, encoding)
  }

  export function countFile(filepath: string, encoding: string = "cl100k_base") {
    if (!core?.countTokens) return undefined
    return core.countTokens(filepath, encoding)
  }
}
