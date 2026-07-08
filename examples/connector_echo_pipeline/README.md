# Connector echo pipeline

This example shows the smallest complete connector path:

```text
src/main.num
  -> num.toml [connectors]
  -> python/echo.py
  -> generated/connectors.d.ts
  -> generated/connectors.py
  -> generated/NumConnectorSdk.java
  -> generated/num_connectors.h
  -> javascript/echo-consumer.js
  -> java/EchoConnectorFixture.java
  -> c/echo_connector_fixture.c
```

`num` owns the connector contract and workflow checks. Python owns the real
process implementation and imports the generated Python stub for the egress
context shape. JavaScript/TypeScript consumers can use the generated types
without owning the `.num` parser or policy model.
Java/JVM consumers can implement the generated Java interfaces and checked
failure contract without a Num-owned JVM runtime.
C consumers can compile against a generated safe-wrapper header that requires
structured status results, audit context, and timeout metadata instead of raw
native calls.

## Check the Num contract

```bash
num check examples/connector_echo_pipeline
```

## Probe the Python connector

```bash
num connector probe examples/connector_echo_pipeline echo.reply --arg '"hello"' --json
```

The probe starts `python/echo.py`, sends the runtime connector JSON payload on
stdin, and expects one JSON value on stdout.

## Generate connector SDKs

```bash
num connector-sdk examples/connector_echo_pipeline \
  --out examples/connector_echo_pipeline/generated/connectors.d.ts
num connector-sdk examples/connector_echo_pipeline \
  --language python \
  --out examples/connector_echo_pipeline/generated/connectors.py
num connector-sdk examples/connector_echo_pipeline \
  --language java \
  --out examples/connector_echo_pipeline/generated/NumConnectorSdk.java
num connector-sdk examples/connector_echo_pipeline \
  --language c \
  --out examples/connector_echo_pipeline/generated/num_connectors.h
```

The generated files are intentionally checked in so JS/TS, Python, JVM, and C
consumers can inspect the contract without running the generator first.

The Java fixture in `java/EchoConnectorFixture.java` implements the generated
interfaces. It is a compile-time contract fixture only; classpath management,
JVM lifecycle, async callbacks, and runtime adapter execution are outside this
example.

The C fixture in `c/echo_connector_fixture.c` implements the generated C symbol
and can be syntax-checked with a system C compiler:

```bash
cc -std=c11 -fsyntax-only \
  -I examples/connector_echo_pipeline/generated \
  examples/connector_echo_pipeline/c/echo_connector_fixture.c
```

It is also a compile-time contract fixture only. Raw pointers, callbacks,
shared memory, unmanaged native threads, and executable native runtime adapter
loading are outside this example.

## Run the workflow

```bash
num run examples/connector_echo_pipeline
```

The workflow calls `echo.reply`, receives a Python-produced `Text`, and records
an audit event after the connector boundary succeeds.
