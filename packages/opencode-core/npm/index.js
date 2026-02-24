let native
try {
  native = require("../prebuilds/" + process.platform + "-" + process.arch + "/opencode-core.node")
} catch (e) {
  native = require("@opencode-ai/core-" + process.platform + "-" + process.arch)
}

module.exports = {
  glob: native.glob,
  globParallel: native.globParallel,
  countTokens: native.countTokens,
  countTokensFromText: native.countTokensFromText,
  countTokensStreaming: native.countTokensStreaming,
  isIgnored: native.isIgnored,
  filterPaths: native.filterPaths,
  compilePatterns: native.compilePatterns,
  truncate: native.truncate,
  truncateLines: native.truncateLines,
  truncateBytes: native.truncateBytes,
  truncateTail: native.truncateTail,
  extractArchive: native.extractArchive,
  listArchiveContents: native.listArchiveContents,
  readArchiveEntry: native.readArchiveEntry,
  extractTarGz: native.extractTarGz,
  extractTarBz2: native.extractTarBz2,
  writeGlobCache: native.writeGlobCache,
  readGlobCache: native.readGlobCache,
  writeTokenCache: native.writeTokenCache,
  readTokenCache: native.readTokenCache,
  clearCache: native.clearCache,
  deleteCache: native.deleteCache,
  containsPath: native.containsPath,
  overlapsPath: native.overlapsPath,
  listFiles: native.listFiles,
  searchContent: native.searchContent,
  searchContentAdvanced: native.searchContentAdvanced,
  readFileWindow: native.readFileWindow,
  readDirWindow: native.readDirWindow,
  FileWatcher: native.FileWatcher,
  CompiledIgnore: native.CompiledIgnore,
}
