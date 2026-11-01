CREATE TABLE IF NOT EXISTS categories (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    name VARCHAR(120) NOT NULL UNIQUE
) ENGINE=InnoDB;

INSERT INTO categories (id, name) VALUES
    (1, 'General'),
    (2, 'Office');

CREATE TABLE IF NOT EXISTS products (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    name VARCHAR(120) NOT NULL,
    sku VARCHAR(64) NOT NULL UNIQUE,
    description TEXT NULL,
    price_cents BIGINT NOT NULL CHECK (price_cents >= 0),
    category_id BIGINT NOT NULL DEFAULT 1,
    CONSTRAINT products_category_fk FOREIGN KEY (category_id) REFERENCES categories(id)
) ENGINE=InnoDB;
