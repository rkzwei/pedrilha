.PHONY: build run dev clean

## build: compile Tailwind CSS, trunk WASM frontend, and API binary.
build:
	cd crates/frontend && npm install && npm run build:css
	trunk build --release
	cargo build --release -p gem-finder-api

## run: single-process deployment — API serves the pre-built frontend from dist/.
## On Linux/macOS. Windows users: use scripts/run.ps1 instead.
run: build
	export $$(grep -v '^#' SECRETS.env | xargs) && SERVE_FRONTEND=1 ./target/release/gem-finder-api

## dev: start API backend + trunk dev server in parallel (frontend proxied to :3000).
dev:
	cargo run -p gem-finder-api &
	trunk serve

clean:
	cargo clean
	rm -rf dist crates/frontend/assets/tailwind.css
