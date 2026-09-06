CREATE TABLE algebra_people (id integer PRIMARY KEY);
INSERT INTO algebra_people (id) VALUES (1), (2), (3);

CREATE TABLE algebra_names (id integer PRIMARY KEY, name text);
INSERT INTO algebra_names (id, name) VALUES (1, 'Ada');

CREATE TABLE algebra_nullable (id integer PRIMARY KEY, label text);
INSERT INTO algebra_nullable (id, label) VALUES (1, 'visible'), (2, NULL);

CREATE TABLE algebra_blocked (id integer PRIMARY KEY);
INSERT INTO algebra_blocked (id) VALUES (2);

-- #60：两条不同的中间路径到同一终点必须保留 bag 重数；person/1 是唯一
-- EXISTS(name) 且 NOT EXISTS(blocked) 的起点。
CREATE TABLE algebra_links (source_id integer NOT NULL, target_id integer NOT NULL);
INSERT INTO algebra_links (source_id, target_id) VALUES (1, 2), (2, 3), (1, 4), (4, 3);

CREATE TABLE algebra_values (id integer PRIMARY KEY, value numeric(10, 1) NOT NULL);
CREATE TABLE algebra_other_values (id integer NOT NULL, value numeric(10, 1) NOT NULL);
INSERT INTO algebra_values (id, value) VALUES (1, 1.0), (3, 3.0);
INSERT INTO algebra_other_values (id, value) VALUES (1, 1.0), (1, 2.0), (3, 4.0), (3, 5.0);

-- #58：以 PostgreSQL numeric 的原始精确词法驱动 decimal 算术与 ROUND，避免
-- 测试只在 Rust 内存 literals 上通过而绕过实际 adapter 输入。
CREATE TABLE algebra_money (id integer PRIMARY KEY, amount numeric(38, 2) NOT NULL, category text NOT NULL);
INSERT INTO algebra_money (id, amount, category) VALUES (1, 2.50, 'profit'), (2, -1.50, 'loss'), (3, 0.10, 'profit');
