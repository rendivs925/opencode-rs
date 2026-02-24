const { load } = require("@node-rs/logger")

let native
try {
  native = require("../prebuilds/" + process.platform + "-" + process.arch + "/opencode-core.node")
} catch (e) {
  native = require("@opencode-ai/core-" + process.platform + "-" + process.arch)
}

module.exports = {
  glob: native.glob,
  globParallel: native.glob_parallel,
  countTokens: native.count_tokens,
  countTokensFromText: native.count_tokens_from_text,
  countTokensStreaming: native.count_tokens_streaming,
  isIgnored: native.is_ignored,
  filterPaths: native.filter_paths,
  compilePatterns: native.compile_patterns,
  truncate: native.truncate,
  truncateLines: native.truncate_lines,
  truncateBytes: native.truncate_bytes,
  truncateTail: native.truncate_tail,
  extractArchive: native.extract_archive,
  listArchiveContents: native.list_archive_contents,
  readArchiveEntry: native.read_archive_entry,
  extractTarGz: native.extract_tar_gz,
  writeGlobCache: native.write_glob_cache,
  readGlobCache: native.read_glob_cache,
  writeTokenCache: native.write_token_cache,
  readTokenCache: native.read_token_cache,
  clearCache: native.clear_cache,
  deleteCache: native.delete_cache,
  FileWatcher: native.FileWatcher,
  CompiledIgnore: native.CompiledIgnore,
}
