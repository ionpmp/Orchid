# Security Policy

## Supported Versions

During the MVP/pre-alpha phase, only the latest release is supported.

| Version | Supported |
|---|---|
| 0.1.x | ✅ Yes |
| < 0.1 | ❌ No |

## Reporting a Vulnerability

**Do not open a public issue for security problems.**

Send a private vulnerability report through GitHub Security Advisories:
https://github.com/ionpmp/Orchid/security/advisories/new

Please include:
- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Orchid and Windows versions affected
- A suggested fix, if you have one

We will aim to respond within 72 hours.

## Areas of Particular Concern

- **Password Manager** (KDBX4 format)
- **File Encryption** (age)
- **Biometrics** (Windows Hello integration)
- **Network Clients** (SFTP, SMB, WebDAV)
