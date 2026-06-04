# Security Policy

`num` is pre-1.0 language infrastructure. Security-sensitive areas include
policy checks, trust/privacy labels, connector execution, process connectors,
service route authorization, tenant isolation, secrets, and deployment tooling.

## Reporting

Please do not publish exploit details in public issues. Open a private GitHub
security advisory when available, or contact the repository owner directly.

Include:

- affected command, runtime API, or language construct;
- minimal `.num` reproduction;
- expected and actual behavior;
- impact on data flow, authorization, secrets, tenant isolation, or execution.

## Supported Versions

Only the default branch is currently supported.

## Handling

Security fixes should include regression tests and documentation updates when
they change user-visible behavior.

