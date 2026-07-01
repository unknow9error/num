CREATE TABLE refunds (
    id UUID PRIMARY KEY,
    amount NUMERIC(12,2) NOT NULL,
    note TEXT
);

CREATE TABLE refund_events (
    id UUID PRIMARY KEY,
    refund_id UUID NOT NULL
);
