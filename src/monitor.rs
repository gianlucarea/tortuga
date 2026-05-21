// TODO Phase 3: HTTP polling tasks — one tokio::spawn per endpoint.
// Each task loops: sleep(interval) → reqwest GET with timeout → compare expected status
// → update SharedState + push LogEvent → fire alert on status change.
