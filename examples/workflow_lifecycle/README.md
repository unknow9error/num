# Workflow Lifecycle Fixture

This example keeps workflow lifecycle behavior visible through `.num` tests and
durable workflow CLI commands.

Run the direct workflow fixtures:

```bash
num test examples/workflow_lifecycle
```

Exercise the file-backed event worker without distributed infrastructure:

```bash
rm -rf examples/workflow_lifecycle/.num-state examples/workflow_lifecycle/audit
num workflow enqueue examples/workflow_lifecycle start wf_lifecycle wait_resume_checkpoint --event-id evt-start
num workflow enqueue examples/workflow_lifecycle wait wf_lifecycle --event-id evt-wait
num workflow enqueue examples/workflow_lifecycle resume wf_lifecycle --event-id evt-resume
num workflow enqueue examples/workflow_lifecycle resume wf_lifecycle --event-id evt-resume
num workflow enqueue examples/workflow_lifecycle complete wf_lifecycle --event-id evt-complete
num workflow drain examples/workflow_lifecycle --max-events 10 --json
num workflow-report examples/workflow_lifecycle --json
```

The repeated `evt-resume` enqueue is intentional: the worker should treat the
second event as a replay and keep one durable transition for that event id.
