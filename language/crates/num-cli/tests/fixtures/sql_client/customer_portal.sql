CREATE TABLE customers (
    id UUID PRIMARY KEY,
    email TEXT NOT NULL,
    display_name TEXT,
    created_at TIMESTAMP NOT NULL
);

CREATE TABLE refunds (
    tenant_id UUID,
    id UUID,
    customer_id UUID NOT NULL,
    amount NUMERIC(12,2) NOT NULL,
    approved BOOLEAN NOT NULL,
    note TEXT,
    PRIMARY KEY (tenant_id, id),
    CONSTRAINT refunds_customer_fk FOREIGN KEY (customer_id) REFERENCES customers(id)
);

CREATE INDEX idx_refunds_customer_id ON refunds (customer_id);
CREATE UNIQUE INDEX idx_customers_email ON customers (email);
