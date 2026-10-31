CREATE TABLE products (
    id BIGSERIAL PRIMARY KEY,
    name VARCHAR(120) NOT NULL,
    sku VARCHAR(64) NOT NULL UNIQUE,
    description TEXT NULL,
    price_cents BIGINT NOT NULL CHECK (price_cents >= 0)
);
