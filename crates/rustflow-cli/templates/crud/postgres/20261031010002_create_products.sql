CREATE TABLE IF NOT EXISTS categories (
    id BIGSERIAL PRIMARY KEY,
    name VARCHAR(120) NOT NULL UNIQUE
);

INSERT INTO categories (id, name) VALUES
    (1, 'General'),
    (2, 'Office');

CREATE TABLE IF NOT EXISTS products (
    id BIGSERIAL PRIMARY KEY,
    name VARCHAR(120) NOT NULL,
    sku VARCHAR(64) NOT NULL UNIQUE,
    description TEXT NULL,
    price_cents BIGINT NOT NULL CHECK (price_cents >= 0),
    category_id BIGINT NOT NULL DEFAULT 1 REFERENCES categories(id)
);
