import { countTokens, countTokensFromText } from "@/core/native"

export namespace Token {
  export function estimate(input: string) {
    const text = input || ""
    if (!text) return 0
    return countTokensFromText(text, "cl100k_base")
  }

  export function count(input: string, encoding: string = "cl100k_base") {
    return countTokensFromText(input, encoding)
  }

  export function countFile(filepath: string, encoding: string = "cl100k_base") {
    return countTokens(filepath, encoding)
  }
}
