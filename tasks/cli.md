# task: cli — lazybonesd binary (parse config, open store, serve)

## Goal
The single binary. Parse boot config (`lazybones.yaml` + `LAZYBONES_*` env), open
the embedded store, and serve the REST API; plus a one-shot workfile import.

## Deliverables
- `Config::load` — file + env overrides for bind, data dir, namespace/database,
  run label, loop token.
- `lazybonesd serve` (default) — open store, build `AppState`, bind, serve with
  graceful shutdown on Ctrl-C.
- `lazybonesd import <workfile.yaml>` — parse the workfile, resolve each `spec:`
  path to its `tasks/<id>.md` text, and `sync_seeds` into the store.

## Done definition
- `lazybonesd import lazybones/workfile.yaml` populates the DB from the seed and
  exits 0; `lazybonesd serve` answers `GET /health` with `{"status":"ok"}`.
