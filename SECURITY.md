# Security policy

## Supported versions

Security fixes land on the latest minor release line. Older builds are not
supported; upgrade from the Releases page.

## Reporting a vulnerability

Please **do not open a public issue** for security problems. Report privately
using GitHub's *Report a vulnerability* button on the repository's **Security**
tab (Security advisories), or contact the maintainers through the organisation.

Include:

- affected version / commit (`git rev-parse HEAD`),
- steps to reproduce or a proof-of-concept (a minimal `.icraft` or SVG file is
  ideal for parser issues),
- impact assessment (what an attacker could achieve).

We aim to acknowledge reports within 3 working days and to ship a fix or
mitigation before public disclosure. Credit is given in the release notes unless
you prefer to stay anonymous.

## Threat model notes

Icon Crafter treats **imported files and plugins as untrusted input**:

- `.icraft` containers are validated (size/count/string/depth limits, schema
  version check) before any node is touched.
- SVG import parses with `usvg` and only converts path geometry; scripts,
  external references and event handlers are ignored.
- Images are decoded with size/pixel caps and magic-byte sniffing.
- Plugins receive an op-scoped, declared-permission API through the host; they
  never get direct filesystem, network or arbitrary code execution.
- The backend enforces authorization on every route, hashes passwords with
  Argon2id, stores only SHA-256 token hashes, applies rate limits and CSRF
  double-submit protection.

Automated checks in CI: `cargo audit` (advisories), `cargo deny` (licenses and
duplicate/denied crates), `gitleaks` (secret scanning, full history) and a
repository hygiene job (no build output, secrets or oversized files).
