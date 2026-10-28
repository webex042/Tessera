# Loopback HTTP Client (`tsr-infer::http`)

To enforce strict air-gapping and eliminate third-party networking code from non-network crates, `tsr-infer::http` implements a minimal loopback HTTP/1.1 client using only standard library networking (`std::net`).

---

## 1. Zero External HTTP Dependencies

Traditional Rust HTTP clients (`reqwest`, `hyper`, `ureq`) pull in complex TLS stacks, DNS resolvers, and proxy detection mechanisms that violate Tessera's compile-time network ban.

`tsr-infer::http` provides a ~350-line custom HTTP/1.1 client that:
- Refuses to connect to any IP address other than `127.0.0.1` or `::1`.
- Completely ignores system HTTP proxy environment variables (`HTTP_PROXY`, `ALL_PROXY`).
- Allocates a single request per TCP connection with strict connect, write, read, and total wall-clock deadlines.
- Implements chunked transfer encoding parsing for streaming token responses.
- Enforces strict response size limits to prevent out-of-memory denial-of-service from runaway sidecars.

---

## 2. Cancellation Tokens

Interactive user queries (Ask panel or in-place drafts) can be cancelled at any point by the user:
```rust
pub struct CancellationToken {
    cancelled: AtomicBool,
}
```
The loopback client checks the cancellation token every 50ms during streaming reads:
- When cancelled, the client closes the TCP socket immediately.
- `llama-server` detects the socket disconnect and halts GPU computation.
- Memory and compute cycles are freed immediately for the next scheduled request.
