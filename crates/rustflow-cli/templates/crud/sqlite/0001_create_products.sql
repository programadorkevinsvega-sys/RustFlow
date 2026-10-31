CREATE TABLE products (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    sku TEXT NOT NULL UNIQUE,
    description TEXT NULL,
    price_cents BIGINT NOT NULL CHECK (price_cents >= 0)
);
