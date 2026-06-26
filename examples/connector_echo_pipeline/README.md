# Connector echo pipeline

This example shows the smallest complete connector path:

```text
src/main.num
  -> num.toml [connectors]
  -> python/echo.py
  -> generated/connectors.d.ts
  -> generated/connectors.py
  -> javascript/echo-consumer.js
```

`num` owns the connector contract and workflow checks. Python owns the real
process implementation and imports the generated Python stub for the egress
context shape. JavaScript/TypeScript consumers can use the generated types
without owning the `.num` parser or policy model.

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
```

The generated files are intentionally checked in so JS/TS and Python consumers
can inspect the contract without running the generator first.

## Run the workflow

```bash
num run examples/connector_echo_pipeline
```

The workflow calls `echo.reply`, receives a Python-produced `Text`, and records
an audit event after the connector boundary succeeds.
