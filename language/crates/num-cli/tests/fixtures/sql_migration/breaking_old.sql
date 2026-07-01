CREATE TABLE refunds (
    id UUID PRIMARY KEY,
    amount NUMERIC(12,2) NOT NULL,
    note TEXT
);

CREATE TABLE audit_logs (
    id UUID PRIMARY KEY
);
