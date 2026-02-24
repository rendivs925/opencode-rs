# Rust Rewrite Guide

This guide explains how to rewrite OpenCode performance-critical TypeScript paths in Rust and how to run the project with the Rust core.

## Where Rust Lives

- Rust crate: `packages/opencode-core`
- Main app: `packages/opencode`
- NAPI bridge entry: `packages/opencode-core/npm/index.js`
- TS native wrapper: `packages/opencode/src/core/native.ts`

## Rewrite Workflow

1. Add Rust export
- Implement feature in `packages/opencode-core/src/<feature>.rs`.
- Export with `#[napi]`.

2. Bridge export to JS
- Map export in `packages/opencode-core/npm/index.js`.

3. Wire to app runtime
- Add typed wrapper in `packages/opencode/src/core/native.ts`.
- Replace TS call sites in `packages/opencode/src/**` to use native wrappers.

4. Add/adjust tests
- Update tests in `packages/opencode/test/**` for parity.

5. Build and verify
- Build native module.
- Run typecheck + targeted tests.

## Build and Run

From repo root:

```bash
bun install
```

Build native core:

```bash
cd packages/opencode-core
bunx @napi-rs/cli build --release --platform
```

Run app:

```bash
cd packages/opencode
bun run dev .
```

## Validate Rust Is Active

From `packages/opencode`:

```bash
bun run typecheck
bun test test/util/glob.test.ts test/util/token.test.ts test/file/ignore.test.ts test/tool/truncation.test.ts
```

From `packages/opencode-core`:

```bash
node -e 'const c=require("./npm/index.js"); console.log(typeof c.glob, typeof c.countTokensFromText, typeof c.truncate)'
```

Expected shape: `function function function`.

## Benchmark Commands

From `packages/opencode-core`:

```bash
npm run bench:native
npm run bench:compare
```

Reports:

- `packages/opencode-core/benchmarks/report.md`
- `packages/opencode-core/benchmarks/compare-report.md`

## CI

- Prebuild matrix + verify: `.github/workflows/opencode-core-prebuild.yml`
- Benchmark workflow: `.github/workflows/opencode-core-benchmarks.yml`

