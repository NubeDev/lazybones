# lazybones UI

The orchestration dashboard for [`lazybonesd`](../README.md) — a Codex-styled
control surface for the durable task queue + gate. One codebase runs **two ways**:

- **Browser** — `npm run dev`, talks to lazybonesd over HTTP.
- **Desktop** — `npm run desktop`, the same UI in a native Tauri window.

There is no desktop-only render path; the bridge is feature-detected at runtime
(`isDesktop()`), so the browser build is a first-class target, not a fallback.

## Stack

| Layer | Choice |
| --- | --- |
| Framework | React 19 + TypeScript + Vite 6 |
| Styling | Tailwind v4 (`@theme` tokens, OKLCH palette) |
| Components | shadcn-style primitives over Radix UI |
| Data | TanStack Query (polling; SSE lands when lazybonesd ships `/stream`) |
| Desktop | Tauri 2 |
| Icons | lucide-react |

## Run

```sh
cd ui
npm install

# browser (Vite dev server on :1420)
npm run dev

# desktop (native window; runs Vite under the hood)
npm run desktop

# production web bundle
npm run build && npm run preview

# desktop installer
npm run desktop:build
```

Point it at a running daemon in **Settings** (default `http://127.0.0.1:7878`,
loop token `lazybones-loop`), or bake it in with `VITE_API_BASE`.

## Layout

One concern per file, many small files (mirrors the daemon's `≤400-line,
verb-per-file` rule):

```
src/
  app/            # shell, router, providers, navigation model
  components/
    ui/           # shadcn-style primitives (button, card, badge, dialog, …)
    layout/       # sidebar, topbar, theme toggle, connection status
  features/
    dashboard/    # stat cards, lifecycle bar, in-flight panel
    tasks/        # board, columns, cards, detail inspector
    runs/         # transition timeline
    settings/     # daemon connection config
  lib/
    api/          # one file per endpoint over a single fetch boundary
    hooks/        # React Query hooks
    theme/        # dark/light provider
    utils/        # cn(), platform + time helpers
  types/          # mirrors of the Rust Task / Event / Status models
  styles/         # the Tailwind v4 design system
src-tauri/        # the desktop shell (thin: open a window, host the web UI)
```

## What it shows

Everything is read/driven over the daemon's REST surface:

- **Dashboard** — totals, % complete, what's in flight, the lifecycle breakdown.
- **Tasks** — a kanban board (one column per lifecycle status) with a slide-in
  inspector: spec, dependencies (resolved + clickable), claim/worktree/branch/commit
  state, heartbeat, and a **Block** action.
- **Run history** — the full transition log as rows (`from → to`, actor, time).
- **Settings** — daemon address + loop token, persisted locally.
