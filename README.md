# mp-playground

`mp-playground` is a small Axum REST application used to exercise the local
MicroProfile-inspired crates together in one service. It loads configuration
from `application.toml` plus environment variables, starts an HTTP server, opens
configured SQLite datasource pools, exposes health checks, and wraps a simulated
inventory upstream with fault-tolerance policies.

## Run

```sh
cargo run
```

The default listener is `127.0.0.1:3000`. Environment variables use relaxed
MicroProfile-style names, so this overrides `server.port`:

```sh
SERVER_PORT=4000 cargo run
```

## Endpoints

- `GET /` returns the configured service name, greeting, and route list.
- `GET /config` reports the resolved app config and shows the candidates
  considered for `server.port`.
- `GET /database` verifies the default, `audit`, and `events` SQLite pools.
- `GET /inventory/{sku}?mode=ok|flaky|fail|slow` calls a simulated upstream
  through retry, timeout, bulkhead, circuit-breaker, and fallback policies.
- `GET /circuit` reports the inventory circuit-breaker state.
- `GET /internal/health`, `/internal/health/live`,
  `/internal/health/ready`, and `/internal/health/started` are mounted health
  endpoints.

Try the fault-tolerance paths:

```sh
curl 'http://127.0.0.1:3000/inventory/abc?mode=ok'
curl 'http://127.0.0.1:3000/inventory/abc?mode=flaky'
curl 'http://127.0.0.1:3000/inventory/abc?mode=fail'
curl 'http://127.0.0.1:3000/inventory/abc?mode=slow'
```

## Dependency Coverage

| Dependency | Where it is exercised | What this app verifies |
| --- | --- | --- |
| `mp-config` | `src/config.rs`, `src/lib.rs`, `application.toml`, `/config` | Default TOML sources, expression expansion, environment override mapping, `ConfigProperties` derive, nested config, `kebab-case`, `camelCase`, explicit field names, string defaults, Rust defaults, optional values, and `Config::explain`. |
| `mp-config-sqlx` | `src/database.rs`, `application.toml`, `/database` | `Datasources` derive, custom datasource prefix, default datasource selection, field-name datasource selection, explicit `#[datasource(name = "events")]`, SQLite pool connection, and pool tuning from config. |
| `mp-config-tracing` | `src/main.rs`, `[logging]` in `application.toml` | Loading `TracingConfig` from the shared `mp-config` source and installing `tracing-subscriber` before the app starts. |
| `axum-health` | `src/health.rs`, `src/lib.rs`, `/internal/health/*` | `#[health_check]`, liveness and startup checks, shared app state in checks, `Health::builder`, and mounting health routes below `/internal/health`. |
| `axum-fault-tolerance` | `src/config.rs`, `src/inventory.rs`, `src/lib.rs`, `/inventory/*`, `/circuit` | `FaultToleranceConfig` loaded through `mp-config`, policy construction, timeout, retry, bulkhead, fallback, circuit-breaker state, and circuit-breaker readiness integration with `axum-health`. |
| `axum` | `src/lib.rs`, `src/routes.rs`, `src/database.rs`, `src/inventory.rs`, `src/error.rs` | Router composition, typed extractors, JSON responses, shared state, and error-to-response conversion. |
| `sqlx` | `src/database.rs`, `/database` | In-memory SQLite pools, schema initialization, inserts, and query checks across the configured datasources. |

## Module Map

- `src/lib.rs` wires the app together: configuration load, state construction,
  policy construction, router setup, and tests.
- `src/config.rs` contains the typed application config loaded with
  `ConfigProperties`.
- `src/database.rs` owns the derived datasource container, database
  initialization, and `/database`.
- `src/health.rs` owns the `axum-health` checks.
- `src/inventory.rs` owns the simulated upstream and fault-tolerant inventory
  handler.
- `src/routes.rs` owns the index, config report, and circuit report handlers.
- `src/error.rs` maps application errors into HTTP responses.
- `src/state.rs` contains shared Axum state.

## Configuration Notes

`application.toml` intentionally includes examples for the macro and config
forms this project is meant to cover:

- `[server]` and `[server.http]` exercise prefixed and nested `mp-config`
  structs with `kebab-case` fields.
- `[client] requestTimeoutMs` exercises `camelCase` field renaming.
- `[service] display-name` exercises explicit field-name mapping.
- `[datasource]`, `[datasource.audit]`, and `[datasource.events]` exercise the
  default, inferred named, and explicitly named datasource macro forms.
- `[fault-tolerance]` uses duration strings such as `250ms` and `1500ms`.
- `[logging]` drives `mp-config-tracing`.
