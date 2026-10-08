# V2 Phase 3 — Host bridge permission boundary (foundation)

This is a **stacked, unmerged** security increment based on V2 core PR #6.
It does **not** currently grant Cowork access to Windows CMD, PowerShell,
files, networks, or the Internet. No command runner or MCP server is exposed.

## Design requirements

- Cowork's own VM and the Windows host are different security domains. The
  proxy cannot grant guest OS capabilities just by changing an API endpoint.
- Every model-suggested shell command is a **pending request** only. It requires
  explicit approval from a trusted Windows UI that shows the exact operation,
  command, working directory, and possible effects.
- Approval is short-lived, single-use and bound to an exact operation and
  request string. Replaying or modifying an approved request is rejected.
- Existing filesystem paths are canonicalized before scope checks; new files
  are confined using their canonical parent. Symlinks must not allow reads or
  writes outside approved workspaces.
- Windows shell execution and public networking default disabled. A future
  network fetcher must prevent SSRF, DNS rebinding, redirects to private IPs,
  credential leakage and oversized downloads; **this module does not fetch**.
- The final tool runner must be least-privileged and propagate cancellation,
  enforce timeouts, resource and output bounds, redact secrets, audit allowed
  operations, and disallow unattended writes/destructive actions.
- Do not expose `approve_from_host_ui` through provider-facing HTTP or MCP.

## Current deliverable

`src-tauri/src/v2/host_permissions.rs` implements a pure permission
lifecycle with unit tests. It is intentionally unconnected to the executing
proxy while Windows UI review and MCP transport are being designed.

## Release gate

Real Windows approval prompts, user denial/revocation, host-vs-VM disclosure,
path-confinement escape tests, public URL resolution safeguards, and real
Claude Desktop Code/Cowork acceptance tests are mandatory before enabling.
