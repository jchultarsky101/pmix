# Security Policy

## Supported versions

Only the latest release on crates.io receives security fixes. Until the first
release, that means the `main` branch.

## Reporting a vulnerability

`pmix` parses untrusted binary and text files, so memory-safety and
denial-of-service issues in the readers are in scope.

Please **do not** open a public issue for a security problem. Instead, use
GitHub's private vulnerability reporting for this repository:

<https://github.com/jchultarsky101/pmix/security/advisories/new>

Include a description of the issue, steps or a sample file that reproduce it,
and the version or commit you tested. You should receive an acknowledgement
within a few days. Once a fix is available it will be released and the
advisory published with credit to the reporter, unless you prefer to remain
anonymous.
