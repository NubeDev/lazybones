# lazybones — multi-agent build orchestrator (Rust/cargo). Developer tasks.
#
#   make build         build the workspace (lazybonesd)
#   make import         import the seed workfile into the embedded DB (idempotent)
#   make serve          run the REST daemon (open store + serve)
#   make dev            seed + serve the daemon AND run the dashboard (backend + frontend)
#   make dev-backend    backend only: import the seed queue, then serve (no UI)
#   make demo           end-to-end walkthrough: build, seed, serve, curl the API, tear down
#   make test           cargo test (workspace)
#   make lint           cargo clippy (workspace, -D warnings)
#   make fmt            cargo fmt
#   make clean          cargo clean + remove the runtime dir (.lazy: DB + worktrees)
#   make wipe           delete ONLY the runtime dir (.lazy) — keep the cargo target
#   make kill           free BOTH dev ports (api + ui) / reap orphaned daemon + vite
#   make install-hcom   check the hcom engine is installed; install the loop script if present
#   make secrets-init   scaffold a gitignored .env from .env.example (agent CLI keys the daemon reads)
#   make ui             install deps + build the web dashboard bundle (ui/dist)
#   make ui-install     install the UI's npm dependencies (ui/node_modules)
#   make ui-dev         run the dashboard in the browser (Vite dev server)
#   make ui-desktop     run the dashboard as a native Tauri window
#   make ui-build       production web bundle (ui/dist) — assumes deps installed
#   make ui-bundle      build the desktop installer (Tauri)
#   make ui-clean       remove the UI build outputs (dist, node_modules, tauri target)
#
# lazybones is the durable queue + the green-build gate (the `lazybonesd` daemon).
# The orchestration *loop* is hcom — an external tool (`hcom run lazybones`), not a
# crate in this workspace. See README.md / SCOPE.md.

# The daemon binary the workspace builds.
BIN := lazybonesd

# Config + seed files (committed). The daemon resolves the rest from the DB after
# the first import, so these are only the seed/boot inputs.
CONFIG       ?= lazybones.yaml
WORKFILE     ?= workfile.yaml
# Every key in lazybones.yaml is overridable by LAZYBONES_*; the daemon needs
# LAZYBONES_CONFIG to find the file. Export it for every recipe that runs the binary.
export LAZYBONES_CONFIG := $(CONFIG)

# API bind. Must MATCH `api.bind` in $(CONFIG) (default 127.0.0.1:46787) — it's used
# here only to curl the demo and to target `make kill`. Override BOTH together
# (e.g. `make serve LAZYBONES_BIND=127.0.0.1:9000 BIND=127.0.0.1:9000`).
BIND ?= 127.0.0.1:46787
HOST := $(word 1,$(subst :, ,$(BIND)))
PORT := $(word 2,$(subst :, ,$(BIND)))

# The dashboard's Vite dev server port (server.port in ui/vite.config). `make dev`
# and `make ui-dev` print this; `make kill` frees it so a leaked Vite never wedges
# the next boot. Override together with the vite config if you change it.
UI_PORT ?= 51840

# The loop authenticates with this bearer token (config key `loop_token`, default
# `lazybones-loop`). Used by the demo's authenticated calls.
LOOP_TOKEN ?= lazybones-loop

# Runtime dir the daemon + loop write to (embedded SurrealDB files under
# data_dir, plus the worktrees the loop creates). Gitignored as /.lazy. Pinned
# here so `wipe`/`clean` know what to remove.
RUNTIME_DIR ?= .lazy

# Built debug binary path (used by the demo to run without re-invoking cargo each call).
BIN_PATH := target/debug/$(BIN)

# --- hcom: the orchestration engine (external tool, not a crate here) ---
# hcom is installed as its own binary (https://github.com/aannoo/hcom — verify with
# `hcom status`). `make install-hcom` fetches it via the official shell installer.
# The loop is an hcom workflow script installed into $(HCOM_SCRIPTS); we copy it
# there from $(LOOP_SCRIPT) when it exists.
HCOM_SCRIPTS   ?= $(HOME)/.hcom/scripts
HCOM_INSTALLER ?= https://github.com/aannoo/hcom/releases/latest/download/hcom-installer.sh
HCOM_RELEASES  ?= https://github.com/aannoo/hcom/releases
LOOP_SCRIPT    := scripts/lazybones.sh

# --- agent CLIs + secrets: the tools the loop spawns to do the work ---
# lazybones is the queue + gate; the actual coding is done by an agent CLI per task
# (config `agent_tool`: claude | codex | gemini | opencode ...). Those CLIs read
# their credentials from the environment. $(ENV_FILE) (gitignored, scaffolded from
# $(ENV_EXAMPLE)) holds those keys; the daemon loads it so the agents inherit them.
ENV_FILE    ?= .env
ENV_EXAMPLE ?= .env.example

# --- ui: the dashboard (React + Tailwind + Tauri; browser or desktop) ---
# A self-contained npm project under $(UI_DIR) with its own Tauri shell. It is NOT
# a cargo crate in this workspace — it talks to lazybonesd over REST. $(NPM) drives
# install/build; the desktop targets go through the Tauri CLI via `npm run`.
UI_DIR ?= ui
NPM    ?= npm

.PHONY: build import serve dev dev-backend demo test lint fmt clean wipe kill \
        install-hcom secrets-init \
        ui ui-install ui-dev ui-desktop ui-build ui-bundle ui-clean

build:
	cargo build

build-release:
	cargo build --release

# Import the seed workfile into the embedded DB. Idempotent — re-run to reconcile
# (upserts task documents + the depends_on graph). Builds first so the binary exists.
import: build
	cargo run --bin $(BIN) -- import $(WORKFILE)

# Open the store and serve the REST API (the default subcommand). Ctrl-C stops it.
serve: build
	@echo "lazybonesd → http://$(BIND)   (health: curl http://$(BIND)/health)"
	cargo run --bin $(BIN) -- serve

# The full quickstart: seed the queue, boot the daemon in the background, then run
# the dashboard's Vite dev server in the foreground — backend + frontend together.
# The trap reaps the daemon on any exit (Ctrl-C, failure) so it never orphans the
# port. We poll /health instead of a blind sleep so the UI doesn't race the bind.
# `make dev-backend` is the old backend-only flow (import + serve, no UI).
dev: import ui-install
	@echo "--- booting lazybonesd in the background (http://$(BIND)) ---"
	@trap 'kill $$SERVER_PID 2>/dev/null; wait $$SERVER_PID 2>/dev/null; echo; echo "=== dev stopped — daemon reaped ==="' EXIT INT TERM; \
	$(BIN_PATH) serve >/tmp/lazybones-dev.log 2>&1 & \
	SERVER_PID=$$!; \
	echo "waiting for /health ..."; \
	i=0; \
	until curl -sf http://$(BIND)/health >/dev/null 2>&1; do \
		i=$$((i+1)); \
		if ! kill -0 $$SERVER_PID 2>/dev/null; then echo "daemon exited early; log:"; cat /tmp/lazybones-dev.log; exit 1; fi; \
		if [ $$i -ge 100 ]; then echo "timed out waiting for /health"; exit 1; fi; \
		sleep 0.1; \
	done; \
	echo "lazybonesd ready → http://$(BIND)  (log: /tmp/lazybones-dev.log)"; \
	echo "--- starting dashboard (browser) → http://localhost:$(UI_PORT) ---"; \
	cd $(UI_DIR) && $(NPM) run dev

# Backend-only quickstart: seed the queue, then serve in the foreground (no UI).
# `import` is idempotent so this is safe to re-run.
dev-backend: import serve

# End-to-end walkthrough you can watch: build, import the seed, boot the daemon in
# the background, exercise the REST surface with curl, then tear it down. No state is
# left running. Uses a fresh runtime dir so the output is reproducible.
#
# The trap reaps the backgrounded daemon on any exit (success, failure, Ctrl-C) so
# the demo never leaves an orphan holding the port. We poll /health instead of a
# blind sleep so the curls don't race the daemon's bind.
demo: build
	@echo "=== lazybones demo — seeding a fresh queue and exercising the API ==="
	@rm -rf $(RUNTIME_DIR)
	@$(MAKE) --no-print-directory import
	@echo
	@echo "--- booting lazybonesd in the background (http://$(BIND)) ---"
	@trap 'kill $$SERVER_PID 2>/dev/null; wait $$SERVER_PID 2>/dev/null; echo; echo "=== demo done — daemon stopped, runtime left in $(RUNTIME_DIR) ==="' EXIT INT TERM; \
	$(BIN_PATH) serve >/tmp/lazybones-demo.log 2>&1 & \
	SERVER_PID=$$!; \
	echo "waiting for /health ..."; \
	i=0; \
	until curl -sf http://$(BIND)/health >/dev/null 2>&1; do \
		i=$$((i+1)); \
		if ! kill -0 $$SERVER_PID 2>/dev/null; then echo "daemon exited early; log:"; cat /tmp/lazybones-demo.log; exit 1; fi; \
		if [ $$i -ge 100 ]; then echo "timed out waiting for /health"; exit 1; fi; \
		sleep 0.1; \
	done; \
	echo; echo "--- GET /health ---"; \
	curl -s http://$(BIND)/health; echo; \
	echo; echo "--- GET /tasks (all, freshly imported) ---"; \
	curl -s http://$(BIND)/tasks; echo; \
	echo; echo "--- POST /tasks/promote (pending -> ready; needs the loop token) ---"; \
	curl -s -X POST http://$(BIND)/tasks/promote -H 'authorization: Bearer $(LOOP_TOKEN)'; echo; \
	echo; echo "--- GET /tasks?status=ready ---"; \
	curl -s 'http://$(BIND)/tasks?status=ready'; echo

test:
	cargo test --workspace

lint:
	cargo clippy --workspace --all-targets -- -D warnings

fmt:
	cargo fmt

clean:
	cargo clean
	rm -rf $(RUNTIME_DIR)

# Wipe ONLY the runtime dir (embedded DB + worktrees) — reset to an empty queue
# without paying for a full `cargo clean` + recompile. Stops a running daemon first
# so nothing holds the DB lock as the dir is removed.
wipe: kill
	rm -rf $(RUNTIME_DIR)
	@echo "wiped runtime dir $(RUNTIME_DIR) (cargo target kept)"

# Free BOTH dev ports and reap whatever a crashed run left behind — the daemon AND
# the dashboard's Vite dev server. `make dev`'s trap only reaps the daemon, so a
# hard-killed dev session leaks Vite holding the UI port; the next `make dev` then
# dies with "Port $(UI_PORT) is already in use". This target kills both, no matter what.
#
# Each service is freed two ways so one absent tool can't leave a straggler:
#   - fuser frees anything bound to the port (absent on some boxes — hence the ||true);
#   - pkill matches the process by an identifying string.
# The pkill patterns lead with a bracket class (`[l]azybonesd`, `[v]ite`) so the
# pattern STRING contains no literal match for itself and pkill never kills its own
# shell. The vite pattern is anchored to this repo's path so we never touch an
# unrelated vite elsewhere on the box. SIGTERM first (let the embedded store close
# cleanly), then poll and escalate stragglers to SIGKILL.
VITE_PAT := $(CURDIR)/$(UI_DIR)/node_modules/.bin/[v]ite
kill:
	-@fuser -TERM -k $(PORT)/tcp 2>/dev/null || true
	-@fuser -TERM -k $(UI_PORT)/tcp 2>/dev/null || true
	-@pkill -TERM -f '[l]azybonesd' 2>/dev/null || true
	-@pkill -TERM -f '$(VITE_PAT)' 2>/dev/null || true
	@i=0; \
	while pgrep -f '[l]azybonesd' >/dev/null 2>&1 || pgrep -f '$(VITE_PAT)' >/dev/null 2>&1; do \
		i=$$((i+1)); \
		if [ $$i -ge 80 ]; then \
			pkill -KILL -f '[l]azybonesd' 2>/dev/null || true; \
			pkill -KILL -f '$(VITE_PAT)' 2>/dev/null || true; \
			break; \
		fi; \
		sleep 0.1; \
	done
	@echo "freed ports $(PORT) (api) + $(UI_PORT) (ui) and killed any orphaned $(BIN)/vite"

# Make the hcom orchestration loop available.
#
# hcom is the *engine* — a standalone tool installed as its own binary, NOT a crate
# in this workspace and NOT something this Makefile builds. So this target does two
# things:
#   1. Verify the `hcom` binary is on PATH. If missing, install it via hcom's
#      official shell installer ($(HCOM_INSTALLER)); on failure, print the manual
#      install options (brew / pipx / release binary) and stop.
#   2. Install the loop script ($(LOOP_SCRIPT)) into hcom's scripts dir so
#      `hcom run lazybones` finds it. The loop script is a tracked follow-up (see
#      README "Not yet built"); until it lands this step is a clear no-op, not an error.
install-hcom:
	@if ! command -v hcom >/dev/null 2>&1; then \
		echo "hcom is not installed (not on PATH) — installing via the official shell installer..."; \
		echo "  source: $(HCOM_INSTALLER)"; \
		curl -fsSL "$(HCOM_INSTALLER)" | sh || { \
			echo; \
			echo "automatic install failed. Install hcom by hand, then re-run 'make install-hcom':"; \
			echo "  brew:  brew install aannoo/hcom/hcom"; \
			echo "  pipx:  uv tool install hcom   (or: pip install hcom)"; \
			echo "  bin:   download from $(HCOM_RELEASES)"; \
			echo "then verify with: hcom status"; \
			exit 1; \
		}; \
		command -v hcom >/dev/null 2>&1 || { \
			echo "hcom installed but not on PATH — open a new shell (or add its bin dir to PATH), then re-run 'make install-hcom'."; \
			exit 1; \
		}; \
	fi
	@echo "hcom found: $$(command -v hcom)  ($$(hcom --version 2>/dev/null | head -1))"
	@if [ -f "$(LOOP_SCRIPT)" ]; then \
		mkdir -p "$(HCOM_SCRIPTS)"; \
		cp "$(LOOP_SCRIPT)" "$(HCOM_SCRIPTS)/"; \
		echo "installed $(LOOP_SCRIPT) -> $(HCOM_SCRIPTS)/  (run with: hcom run lazybones \"<goal>\")"; \
	else \
		echo "loop script $(LOOP_SCRIPT) not present yet (tracked follow-up — see README 'Not yet built')."; \
		echo "hcom is ready; nothing to install. The lazybonesd queue+gate works standalone via 'make dev'."; \
	fi

# Scaffold the gitignored $(ENV_FILE) from $(ENV_EXAMPLE) so you can fill in the
# agent CLI keys (ANTHROPIC_API_KEY, OPENAI_API_KEY, ...). The daemon loads this
# file at boot; GET /agents then reports which keys are present. Never overwrites
# an existing $(ENV_FILE).
secrets-init:
	@if [ -f "$(ENV_FILE)" ]; then \
		echo "$(ENV_FILE) already exists — leaving it untouched."; \
	else \
		cp "$(ENV_EXAMPLE)" "$(ENV_FILE)"; \
		echo "created $(ENV_FILE) from $(ENV_EXAMPLE) — fill in the keys for the tools you use."; \
		echo "(gitignored; GET /agents reports which keys are set)"; \
	fi

# --- the dashboard UI (ui/) ----------------------------------------------------
# The UI lives in its own npm project ($(UI_DIR)) with a Tauri desktop shell. These
# targets are thin wrappers over its package.json scripts so the whole project
# builds from this one Makefile. They cd into $(UI_DIR) in a subshell so the parent
# make's cwd is untouched.

# Install deps if missing, then build the web bundle. The default UI target.
ui: ui-install ui-build

# Install the UI's npm dependencies (idempotent; skips if node_modules exists).
ui-install:
	@if [ ! -d "$(UI_DIR)/node_modules" ]; then \
		echo "--- installing UI deps ($(UI_DIR)/node_modules) ---"; \
		cd $(UI_DIR) && $(NPM) install; \
	else \
		echo "UI deps present ($(UI_DIR)/node_modules) — skipping install"; \
	fi

# Run the dashboard in the browser (Vite dev server on :51840). Point it at a
# running daemon in Settings (default http://$(BIND)). Ctrl-C stops it.
ui-dev: ui-install
	@echo "dashboard (browser) → http://localhost:$(UI_PORT)   (daemon: http://$(BIND))"
	cd $(UI_DIR) && $(NPM) run dev

# Run the dashboard as a native desktop window (Tauri runs Vite under the hood).
ui-desktop: ui-install
	cd $(UI_DIR) && $(NPM) run desktop

# Production web bundle → $(UI_DIR)/dist (tsc type-check + vite build).
ui-build:
	cd $(UI_DIR) && $(NPM) run build

# Build the desktop installer (Tauri). Produces native artifacts under the Tauri
# target dir for the host platform.
ui-bundle: ui-install
	cd $(UI_DIR) && $(NPM) run desktop:build

# Remove UI build outputs: the web bundle, installed deps, and the Tauri target.
ui-clean:
	rm -rf $(UI_DIR)/dist $(UI_DIR)/node_modules $(UI_DIR)/src-tauri/target $(UI_DIR)/src-tauri/gen
	@echo "cleaned UI outputs (dist, node_modules, src-tauri/target, src-tauri/gen)"
