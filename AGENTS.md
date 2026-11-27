# AGENTS.md: Standing rules for ZCode workers (PREMIUM RADAR)

You are a worker. The commander (Claude Code) wrote your task contract, will review your diff, and decides pass/fail. Follow the contract first; these rules apply to every task.

## Project facts
- Tauri 2 desktop app: Rust workspace (`crates/radar-core`, `crates/recorder-server`, `src-tauri`) + React 19 / TypeScript / Vite (`src/`).
- Rust does ALL financial math, aggregation and data shaping. TypeScript only formats and renders.
- User-facing text is Thai, written for a beginner who knows only what an option is. Never put option jargon in the main message of easy mode (`src/easy/**`).
- Webull credentials live in Rust + OS keyring only.
- Agent work (W6) lives in `crates/radar-core/src/agent/**`, `src-tauri/src/agent_cmd.rs`, `src/agent/**`. Plan: `docs/dev/PLAN-AGENT.md`.

## VERIFY (run what the contract lists; these are the full gates)
- Frontend: `npm run check` (tsc + eslint), `npm run build`
- Rust: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`
- Rust tests: `cargo test --workspace` (radar-core needs `--all-features` for client/stream tests: `cargo test -p radar-core --all-features`)
- Everything: `npm run verify`
- Redirect long output to a file and report only the summary/failing lines.

## Never
- Never read, print, copy or edit `.env`, `deploy/recorder/.env`, secrets, tokens or keyring code paths unless the contract names them.
- Never run `git commit`, `git push`, `git reset`, `git clean`, `git checkout -- <file>` or rewrite history.
- Never add, remove or upgrade a dependency (`package.json`, `Cargo.toml`) unless the contract explicitly allows that exact package.
- Never delete, skip (`#[ignore]`, `.skip`) or weaken a test to make a check pass.
- Never call the Webull Trade API (orders, preview) or any live network service from tests. Use fixtures / wiremock.
- Never change files outside SCOPE. If the task cannot be done inside SCOPE, stop and say why in the report.
- Never change architecture, public command names or data contracts (`src/types/*.ts`) unless the contract says so.

## Conventions
- Match the surrounding code style. Read a similar file before writing a new one.
- Rust: no `unwrap()`/`expect()` on data from the network or the user; return `Result` with a clear message. No broad error swallowing.
- Agent tools: every tool has a "use when / do not use when" description, 1–3 call examples, enums where possible, compact results with `ref`, `as_of`, `source`, and errors that tell the model how to fix the call.
- Comments explain why, not what.

## REPORT (end every run with exactly this)
```
CHANGED: <file list>
COMMANDS: <command> → <pass/fail + key result line>   (one per line)
DOD: <each acceptance criterion> → met / not met (+ why)
UNCERTAIN: <anything you guessed or could not verify>
```
Do not self-approve. Do not paste full logs.
