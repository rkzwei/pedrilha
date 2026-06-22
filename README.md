# Gem Finder 💎

Discover hidden gem movies — incredible films buried under blockbusters and wrongly-rated movies.

## What It Does

Many fantastic movies sit at a mid-7 IMDb rating and never get discovered.
Gem Finder uses a weighted algorithm to surface these hidden gems based on:
- IMDb rating sweet spot (6.5–7.9)
- High rating + low vote count (undiscovered factor)
- Movies obscured by nearby blockbusters
- Critic vs. audience score disparity

## Quick Start

### Prerequisites
- Rust (via rustup)
- Docker (optional, for containerized deployment)

### Running Locally

```bash
# Start the API server (uses local SQLite by default)
cargo run --package gem-finder-api
```

Open `http://localhost:3000` in your browser.

### With Turso Remote

```bash
export TURSO_DATABASE_URL=libsql://your-db.turso.io
export TURSO_AUTH_TOKEN=your-token
cargo run --package gem-finder-api
```

### With Docker

```bash
export TURSO_DATABASE_URL=libsql://your-db.turso.io
export TURSO_AUTH_TOKEN=your-token
docker compose -f docker/docker-compose.yml up
```

## Tech Stack

- **Rust** — both backend and frontend (Leptos WASM)
- **Axum** — web framework
- **Turso** — SQLite-compatible edge database
- **Tailwind CSS** — styling

## Project Structure

```
crates/
├── shared/   # Domain types & constants
├── db/       # Turso database layer
├── api/      # Axum backend
└── frontend/ # Leptos frontend
```

## License

MIT