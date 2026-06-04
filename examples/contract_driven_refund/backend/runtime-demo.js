const contract = require("../generated/refund.contract.json");

class ContractViolation extends Error {
  constructor(message) {
    super(message);
    this.name = "ContractViolation";
  }
}

class NumRuntime {
  constructor({ contract, connectors, permissions, audit, approvals, rollbacks }) {
    this.contract = contract;
    this.connectors = connectors;
    this.permissions = permissions;
    this.audit = audit;
    this.approvals = approvals;
    this.rollbacks = rollbacks;
  }

  async run(workflowName, { input, user }) {
    if (workflowName !== this.contract.workflow.name) {
      throw new ContractViolation(`unknown workflow: ${workflowName}`);
    }

    for (const permission of this.contract.workflow.entryPermissions) {
      await this.requirePermission(user, permission);
    }

    const payment = await this.connectors.payments.find(input.request.payment_id);
    const risk = await this.connectors.ai.assess_refund_risk(input.request);
    const aiStep = this.contract.workflow.steps.find((step) => step.kind === "aiDecision");

    if (risk.confidence < aiStep.minimumConfidence) {
      await this.approvals.request(aiStep.onLowConfidence.action, {
        workflow: workflowName,
        user,
        input,
        risk
      });

      return {
        status: "waiting_for_human_approval",
        action: aiStep.onLowConfidence.action,
        confidence: risk.confidence
      };
    }

    await this.requirePermission(user, "IssueRefund");
    await this.requirePermission(user, "NotifyCustomer");

    const completedActions = [];

    try {
      await this.issueRefund(payment, input.request.amount, user);
      completedActions.push({ name: "issue_refund", args: [payment, input.request.amount] });

      await this.notifyCustomer(payment.customer_email, user);
      completedActions.push({ name: "notify_customer", args: [payment.customer_email] });

      await this.audit.write("refund_workflow_completed", { workflow: workflowName, user });
      return { status: "completed" };
    } catch (error) {
      await this.rollback(completedActions.reverse(), user, error);
      throw error;
    }
  }

  async issueRefund(payment, amount, user) {
    const action = this.contract.actions.issue_refund;
    await this.requireActionPermissions(user, action);
    await this.connectors.payment_gateway.refund(payment.id, amount);
    await this.writeActionAudit(action, user);
  }

  async notifyCustomer(email, user) {
    const action = this.contract.actions.notify_customer;
    await this.requireActionPermissions(user, action);
    await this.connectors.mailer.send(email);
    await this.writeActionAudit(action, user);
  }

  async requireActionPermissions(user, action) {
    for (const permission of action.requires) {
      await this.requirePermission(user, permission);
    }
  }

  async requirePermission(user, permission) {
    const allowed = await this.permissions.has(user, permission);
    if (!allowed) {
      throw new ContractViolation(`missing permission: ${permission}`);
    }
  }

  async writeActionAudit(action, user) {
    for (const event of action.audit || []) {
      await this.audit.write(event, { user });
    }
  }

  async rollback(completedActions, user, cause) {
    for (const completed of completedActions) {
      const action = this.contract.actions[completed.name];
      if (!action.rollback) {
        continue;
      }

      await this.audit.write("rollback_started", {
        user,
        action: completed.name,
        cause: cause.message
      });

      await this.rollbacks[action.rollback](...completed.args);
    }
  }
}

function createDemoRuntime(scenario) {
  const paymentsDb = new Map([
    [
      "pay_123",
      {
        id: { value: "pay_123" },
        amount: { currency: "KZT", cents: 129900 },
        customer_email: "customer@example.com"
      }
    ]
  ]);

  return new NumRuntime({
    contract,
    permissions: {
      async has(user, permission) {
        return user.permissions.includes(permission);
      }
    },
    audit: {
      async write(event, context) {
        console.log("audit", JSON.stringify({ event, user: context.user.id }));
      }
    },
    approvals: {
      async request(action, context) {
        console.log(
          "approval",
          JSON.stringify({
            action,
            user: context.user.id,
            confidence: context.risk.confidence
          })
        );
      }
    },
    connectors: {
      payments: {
        async find(paymentId) {
          console.log("connector payments.find", paymentId.value);
          return paymentsDb.get(paymentId.value);
        }
      },
      ai: {
        async assess_refund_risk() {
          console.log("connector ai.assess_refund_risk");
          if (scenario === "approval") {
            return { value: "Medium", confidence: 0.62 };
          }
          return { value: "Low", confidence: 0.91 };
        }
      },
      payment_gateway: {
        async refund(paymentId, amount) {
          console.log(
            "connector payment_gateway.refund",
            JSON.stringify({ paymentId: paymentId.value, amount })
          );
        }
      },
      mailer: {
        async send(email) {
          console.log("connector mailer.send", email);
          if (scenario === "rollback") {
            throw new Error("mailer is unavailable");
          }
        }
      }
    },
    rollbacks: {
      async reverse_refund(payment, amount) {
        console.log(
          "rollback reverse_refund",
          JSON.stringify({ paymentId: payment.id.value, amount })
        );
      }
    }
  });
}

async function main() {
  const scenario = process.argv[2] || "success";
  const runtime = createDemoRuntime(scenario);
  const user =
    scenario === "denied"
      ? { id: "agent_7", permissions: ["ViewBilling"] }
      : {
          id: "manager_1",
          permissions: ["ViewBilling", "IssueRefund", "NotifyCustomer"]
        };

  const result = await runtime.run("process_refund", {
    user,
    input: {
      request: {
        payment_id: { value: "pay_123" },
        reason: "duplicate charge",
        amount: { currency: "KZT", cents: 129900 }
      }
    }
  });

  console.log("result", JSON.stringify(result));
}

main().catch((error) => {
  console.error(error.name, error.message);
  process.exitCode = 1;
});
