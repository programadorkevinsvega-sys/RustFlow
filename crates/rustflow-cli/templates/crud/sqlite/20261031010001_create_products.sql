CREATE TABLE IF NOT EXISTS categories (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE
);

INSERT INTO categories (id, name) VALUES
    (1, 'General'),
    (2, 'Office');

CREATE TABLE IF NOT EXISTS products (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    sku TEXT NOT NULL UNIQUE,
    description TEXT NULL,
    price_cents BIGINT NOT NULL CHECK (price_cents >= 0),
    category_id INTEGER NOT NULL DEFAULT 1 REFERENCES categories(id)
);
