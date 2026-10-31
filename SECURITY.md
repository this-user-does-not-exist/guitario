# Security Policy

Guitario processes complex score formats (Guitar Pro ZIP/XML, its native JSON) and exposes a CLI with scripts. Treat files, paths, metadata, automation requests, and command parameters as potentially malicious.

## Reporting a vulnerability

Please give the repository maintainers a reasonable opportunity to investigate and coordinate a fix before publishing exploitable details, proof-of-concept code, or weaponized files.

Report privately through GitHub: open the repository's **Security** tab and choose **Report a vulnerability** (private vulnerability reporting). Only the maintainers see the report. Do not open a public issue containing a working exploit, secret, sensitive crash dump, or malicious attachment.

## What to include

A useful report includes:

- affected Guitario version, commit, or release artifact;
- operating system, architecture, build profile, and relevant feature flags;
- the affected component or path;
- security impact and required attacker access;
- exact reproduction steps;
- expected behavior and actual behavior;
- crash logs, backtraces, sanitizer output, or resource measurements when applicable;
- whether the problem reproduces from a clean checkout;
- any known workaround or containment.

Remove credentials, personal paths, private document content, and unrelated system information from logs.

## Malicious test files

Do not attach a potentially harmful or non-redistributable file to a public issue. Initially provide its format, size, cryptographic hash, observed effect, provenance category (for example, synthetic or fuzz-generated), and the command needed to reproduce the problem. Coordinate a private transfer method with maintainers before sending the sample.

Test files accepted into the repository must be minimized, free of personal or proprietary data, legally redistributable, and documented with the expected failure behavior. A fixed parser defect should receive a permanent regression test whenever safe and practical.

## Scope

Security reports may cover:

- Guitar Pro `.gp` (ZIP and `score.gpif` XML), `.gt` and settings parsing;
- decompression bombs, oversized dimensions, integer overflow, memory exhaustion, recursion, or hangs;
- import/export and filesystem path handling, including traversal, symlinks, and unsafe overwrite behavior;
- engine command validation and serialization/deserialization;
- the headless CLI and its JSON scripts;
- audio-device and GPU handling;
- dependencies, CI, packaging, signing, checksums, and release artifact integrity.

General bugs without a security impact should use the project's normal issue and contribution process.

## Current limitations

Guitario has no network surface: there is no control server or MCP server yet (gap G10 in
`docs/gaps.md`). The `.gp` reader caps archive size, decompressed XML size, track count and bar
count before parsing; files are validated before they are accepted. There are no fuzz targets for
the readers yet (gap G11).
