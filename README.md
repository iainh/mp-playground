# mp-playground

Small Axum REST application that demonstrates using `../mp-config`,
`../axum-health`, and `../axum-fault-tolerance` together.

## Run

```sh
cargo run
```

The app reads `application.toml` and then environment variables. Environment
variables use relaxed MicroProfile-style names, so `SERVER_PORT=4000 cargo run`
overrides `server.port`.

## Endpoints

- `GET /` returns the configured service name and available routes.
- `GET /config` shows selected resolved config values and the source candidates
  considered for `server.port`.
- `GET /inventory/:sku` calls a simulated upstream service through timeout,
  retry, circuit-breaker, bulkhead, and fallback policies.
- `GET /circuit` reports the current circuit-breaker state.
- `GET /internal/health`, `/internal/health/live`, `/internal/health/ready`,
  `/internal/health/started` are provided by `axum-health`.

Try the fault-tolerance paths:

```sh
curl 'http://127.0.0.1:3000/inventory/abc?mode=ok'
curl 'http://127.0.0.1:3000/inventory/abc?mode=flaky'
curl 'http://127.0.0.1:3000/inventory/abc?mode=fail'
curl 'http://127.0.0.1:3000/inventory/abc?mode=slow'
```

## Integration points demonstrated

- `mp-config` now parses duration strings such as `250ms` directly into
  `Duration` values.
- `axum-health` now supports `router_at`, used here to mount health endpoints
  under `/internal/health`.
- The health-check macro makes backend-specific checks pleasant, but examples
  need to show how to share application state without introducing cyclic setup.
- `axum-fault-tolerance` now provides `FaultToleranceConfig` behind its
  `mp-config` feature, removing the local policy-building glue.
- `CircuitBreaker::health_check` now exposes circuit state as an `axum-health`
  readiness check behind the `axum-health` feature.
