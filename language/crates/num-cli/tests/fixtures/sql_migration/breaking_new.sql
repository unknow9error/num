CREATE TABLE refunds (
    tenant_id UUID,
    id UUID,
    amount INTEGER NOT NULL,
    PRIMARY KEY (tenant_id, id)
);
