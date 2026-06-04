export type Permission = "ViewBilling" | "IssueRefund" | "NotifyCustomer";
export type RiskLevel = "Low" | "Medium" | "High";

export interface Money<C extends string> {
  currency: C;
  cents: number;
}

export interface PaymentId {
  value: string;
}

export interface RefundRequest {
  payment_id: PaymentId;
  reason: string;
  amount: Money<"KZT">;
}

export interface Payment {
  id: PaymentId;
  amount: Money<"KZT">;
  customer_email: string;
}

export interface Uncertain<T> {
  value: T;
  confidence: number;
}

export interface NumConnectors {
  payments: {
    find(paymentId: PaymentId): Promise<Payment>;
  };
  ai: {
    assess_refund_risk(request: RefundRequest): Promise<Uncertain<RiskLevel>>;
  };
  payment_gateway: {
    refund(paymentId: PaymentId, amount: Money<"KZT">): Promise<void>;
  };
  mailer: {
    send(email: string): Promise<void>;
  };
}
